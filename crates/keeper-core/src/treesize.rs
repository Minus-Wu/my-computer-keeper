use crate::drive::{
    inspect_path_capabilities, is_drive_root, normalize_display_path, query_volume_space,
    VolumeCapabilities,
};
use crate::explorer::{classify_extension, FileCategory};
use crate::{KeeperError, KeeperResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeSizeNode {
    pub name: String,
    pub path: String,
    pub is_dir: bool,
    pub size_bytes: u64,
    pub file_count: usize,
    pub percentage: f32,
    pub category: FileCategory,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CategoryStat {
    pub category: FileCategory,
    pub label: String,
    pub size_bytes: u64,
    pub file_count: usize,
    pub percentage: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargestFileItem {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub category: FileCategory,
    pub is_image: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeSizeReport {
    pub root_path: String,
    pub parent_path: Option<String>,
    pub capabilities: VolumeCapabilities,
    pub total_size_bytes: u64,
    pub total_files: usize,
    pub total_dirs: usize,
    pub scan_duration_ms: u128,
    pub truncated_by_safety_limit: bool,
    pub drive_total_bytes: Option<u64>,
    pub drive_free_bytes: Option<u64>,
    pub drive_used_bytes: Option<u64>,
    pub is_drive_root: bool,
    pub children: Vec<TreeSizeNode>,
    pub category_breakdown: Vec<CategoryStat>,
    pub largest_files: Vec<LargestFileItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScanStatus {
    Scanning,
    Completed,
    Cancelled,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeSizeProgress {
    pub current_path: String,
    pub processed_files: usize,
    pub processed_dirs: usize,
    pub processed_bytes: u64,
    pub elapsed_ms: u128,
    pub estimated_percent: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TreeSizeScanState {
    pub scan_id: String,
    pub target_path: String,
    pub status: ScanStatus,
    pub progress: TreeSizeProgress,
    pub report: Option<TreeSizeReport>,
    pub error: Option<String>,
}

fn category_label(cat: FileCategory) -> &'static str {
    match cat {
        FileCategory::Directory => "資料夾",
        FileCategory::Image => "圖片與設計",
        FileCategory::Video => "影片多媒體",
        FileCategory::Audio => "音訊音樂",
        FileCategory::Code => "程式碼與設定",
        FileCategory::Archive => "壓縮封存檔",
        FileCategory::Document => "文件與報表",
        FileCategory::Executable => "程式與二進位",
        FileCategory::Other => "其他檔案",
    }
}

/// 判斷路徑是否為 Windows 重解析點（Reparse Point / Directory Junction / 符號連結）
/// 避免在走訪如 Application Data 或相容性 Junction 時發生無限迴圈或過度重複計算
#[cfg(windows)]
fn is_reparse_point(entry: &walkdir::DirEntry) -> bool {
    use std::os::windows::fs::MetadataExt;
    if entry.path_is_symlink() {
        return true;
    }
    if let Ok(meta) = fs::symlink_metadata(entry.path()) {
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        return (meta.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT) != 0;
    }
    false
}

#[cfg(not(windows))]
fn is_reparse_point(entry: &walkdir::DirEntry) -> bool {
    entry.path_is_symlink()
}

/// 執行 TreeSize 磁碟空間分析（支援進度回呼與中途取消）
pub fn analyze_directory_with_progress<F>(
    path_str: &str,
    custom_max_depth: Option<usize>,
    cancel_flag: Option<Arc<AtomicBool>>,
    mut progress_callback: F,
) -> KeeperResult<TreeSizeReport>
where
    F: FnMut(&TreeSizeProgress),
{
    let start = Instant::now();
    let root = Path::new(path_str);
    if !root.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let clean_root = normalize_display_path(&canonical);
    let capabilities = inspect_path_capabilities(&clean_root);
    let is_root = is_drive_root(&clean_root);
    let volume_space = query_volume_space(&clean_root);

    let max_depth = custom_max_depth
        .unwrap_or(capabilities.max_scan_depth)
        .min(capabilities.max_scan_depth);

    // 本機磁碟不應受限於 25 萬項人工上限；網路磁碟機則維持保護性上限
    let max_entries = if capabilities.is_network_drive {
        capabilities.max_scan_entries
    } else {
        usize::MAX
    };

    let mut total_size_bytes = 0u64;
    let mut total_files = 0usize;
    let mut total_dirs = 0usize;
    let mut truncated = false;

    // 記錄第一層子項目：(first_path_string, is_dir, size_bytes, file_count, category)
    let mut top_child_map: HashMap<String, (String, bool, u64, usize, FileCategory)> =
        HashMap::new();
    let mut category_map: HashMap<FileCategory, (u64, usize)> = HashMap::new();

    // 高效 Top 20 巨型檔案追踪（避免為百萬檔案做大量字串複製與排序）
    const TOP_FILES_LIMIT: usize = 20;
    let mut largest_files: Vec<LargestFileItem> = Vec::with_capacity(TOP_FILES_LIMIT + 1);
    let mut min_largest_size = 0u64;

    let mut walker = WalkDir::new(&clean_root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter();

    let mut last_progress_emit = Instant::now();
    let mut last_emit_files = 0usize;

    while let Some(entry_res) = walker.next() {
        if let Some(ref cancel) = cancel_flag {
            if cancel.load(Ordering::Relaxed) {
                return Err(KeeperError::OperationCancelled);
            }
        }

        if total_files + total_dirs >= max_entries {
            truncated = true;
            break;
        }

        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };

        let depth = entry.depth();
        if depth == 0 {
            continue;
        }

        // 避免 Windows Reparse Points (如 Application Data / Junction points) 造成的無限循環或走訪異常
        if entry.file_type().is_dir() && is_reparse_point(&entry) {
            walker.skip_current_dir();
            continue;
        }

        // 若為磁碟根目錄下的系統保留目錄（無存取權限且易導致 I/O 卡頓），跳過內部深入遞迴
        if is_root && depth == 1 && entry.file_type().is_dir() {
            let fname = entry.file_name().to_string_lossy();
            if fname.eq_ignore_ascii_case("System Volume Information")
                || fname.eq_ignore_ascii_case("$Recycle.Bin")
                || fname.eq_ignore_ascii_case("Config.Msi")
            {
                walker.skip_current_dir();
                let first_path = clean_root.join(fname.as_ref());
                top_child_map.entry(fname.to_string()).or_insert_with(|| {
                    (
                        normalize_display_path(&first_path)
                            .to_string_lossy()
                            .to_string(),
                        true,
                        0,
                        0,
                        FileCategory::Directory,
                    )
                });
                continue;
            }
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        // 找出相對於 clean_root 的第一層子目錄或檔案名稱
        let rel = match entry.path().strip_prefix(&clean_root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let first_comp = match rel.components().next() {
            Some(c) => c.as_os_str().to_string_lossy().to_string(),
            None => continue,
        };

        // 結構化推導第一層是否為目錄：depth > 1 必定為目錄，depth == 1 則由 meta 決定
        let first_is_dir = if depth > 1 { true } else { meta.is_dir() };

        if meta.is_dir() {
            total_dirs += 1;
            top_child_map.entry(first_comp.clone()).or_insert_with(|| {
                let first_path = clean_root.join(&first_comp);
                (
                    normalize_display_path(&first_path)
                        .to_string_lossy()
                        .to_string(),
                    true,
                    0,
                    0,
                    FileCategory::Directory,
                )
            });
        } else {
            let file_size = meta.len();
            total_files += 1;
            total_size_bytes = total_size_bytes.saturating_add(file_size);

            let ext = entry
                .path()
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let (cat, is_image) = classify_extension(&ext, false);

            let top_entry = top_child_map.entry(first_comp.clone()).or_insert_with(|| {
                let first_path = clean_root.join(&first_comp);
                (
                    normalize_display_path(&first_path)
                        .to_string_lossy()
                        .to_string(),
                    first_is_dir,
                    0,
                    0,
                    if first_is_dir {
                        FileCategory::Directory
                    } else {
                        cat
                    },
                )
            });
            top_entry.2 = top_entry.2.saturating_add(file_size);
            top_entry.3 += 1;

            let cat_stat = category_map.entry(cat).or_insert((0, 0));
            cat_stat.0 = cat_stat.0.saturating_add(file_size);
            cat_stat.1 += 1;

            if largest_files.len() < TOP_FILES_LIMIT || file_size > min_largest_size {
                let norm_path = normalize_display_path(entry.path());
                largest_files.push(LargestFileItem {
                    name: entry.file_name().to_string_lossy().to_string(),
                    path: norm_path.to_string_lossy().to_string(),
                    size_bytes: file_size,
                    category: cat,
                    is_image,
                });
                largest_files.sort_by_key(|f| std::cmp::Reverse(f.size_bytes));
                if largest_files.len() > TOP_FILES_LIMIT {
                    largest_files.truncate(TOP_FILES_LIMIT);
                }
                min_largest_size = largest_files.last().map(|f| f.size_bytes).unwrap_or(0);
            }
        }

        // 定期產生進度更新（每隔 80ms 或每累積 300 個項目）
        let now = Instant::now();
        if now.duration_since(last_progress_emit).as_millis() >= 80
            || (total_files.saturating_sub(last_emit_files)) >= 300
        {
            last_progress_emit = now;
            last_emit_files = total_files;
            let elapsed_ms = start.elapsed().as_millis();
            let estimated_percent = if is_root {
                volume_space.and_then(|(_, _, used)| {
                    if used > 0 {
                        Some(((total_size_bytes as f64 / used as f64) * 100.0).min(99.9) as f32)
                    } else {
                        None
                    }
                })
            } else {
                None
            };

            progress_callback(&TreeSizeProgress {
                current_path: entry.path().to_string_lossy().to_string(),
                processed_files: total_files,
                processed_dirs: total_dirs,
                processed_bytes: total_size_bytes,
                elapsed_ms,
                estimated_percent,
            });
        }
    }

    // 發出最後一次完整掃描進度
    progress_callback(&TreeSizeProgress {
        current_path: clean_root.to_string_lossy().to_string(),
        processed_files: total_files,
        processed_dirs: total_dirs,
        processed_bytes: total_size_bytes,
        elapsed_ms: start.elapsed().as_millis(),
        estimated_percent: if is_root { Some(100.0) } else { None },
    });

    let (drive_total_bytes, drive_free_bytes, drive_used_bytes) = match volume_space {
        Some((total, free, used)) => (Some(total), Some(free), Some(used)),
        None => (None, None, None),
    };

    // 若掃描目標為磁碟機根目錄（如 C:\），以磁碟實體已用空間為百分比基準；
    // 否則以實際掃描到的檔案總量為百分比基準
    let percentage_base = if is_root {
        drive_used_bytes
            .unwrap_or(total_size_bytes)
            .max(total_size_bytes)
    } else {
        total_size_bytes
    };

    let mut children: Vec<TreeSizeNode> = top_child_map
        .into_iter()
        .map(
            |(name, (path, is_dir, size_bytes, file_count, category))| {
                let percentage = if percentage_base > 0 {
                    ((size_bytes as f64 / percentage_base as f64) * 100.0) as f32
                } else {
                    0.0
                };
                TreeSizeNode {
                    name,
                    path,
                    is_dir,
                    size_bytes,
                    file_count,
                    percentage,
                    category,
                }
            },
        )
        .collect();

    // 若掃描磁碟機根目錄且磁碟實體已用空間大於掃描檔案總量（例如 Windows VSS 還原點、MFT 等未直接列舉空間）
    if is_root {
        if let Some(used) = drive_used_bytes {
            let unallocated_system = used.saturating_sub(total_size_bytes);
            if unallocated_system >= 5 * 1024 * 1024 {
                let percentage = if percentage_base > 0 {
                    ((unallocated_system as f64 / percentage_base as f64) * 100.0) as f32
                } else {
                    0.0
                };
                let sys_path = clean_root.join("System Volume Information");
                children.push(TreeSizeNode {
                    name: "🛡️ 系統保留/受保護空間 (VSS還原點與MFT)".to_string(),
                    path: normalize_display_path(&sys_path)
                        .to_string_lossy()
                        .to_string(),
                    is_dir: false,
                    size_bytes: unallocated_system,
                    file_count: 0,
                    percentage,
                    category: FileCategory::Other,
                });
            }
        }
    }

    children.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));

    let mut category_breakdown: Vec<CategoryStat> = category_map
        .into_iter()
        .map(|(category, (size_bytes, file_count))| {
            let percentage = if total_size_bytes > 0 {
                ((size_bytes as f64 / total_size_bytes as f64) * 100.0) as f32
            } else {
                0.0
            };
            CategoryStat {
                category,
                label: category_label(category).to_string(),
                size_bytes,
                file_count,
                percentage,
            }
        })
        .collect();
    category_breakdown.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));

    let parent_path = clean_root
        .parent()
        .map(|p| normalize_display_path(p).to_string_lossy().to_string())
        .filter(|p| !p.is_empty() && p != &clean_root.to_string_lossy());

    Ok(TreeSizeReport {
        root_path: clean_root.to_string_lossy().to_string(),
        parent_path,
        capabilities,
        total_size_bytes,
        total_files,
        total_dirs,
        scan_duration_ms: start.elapsed().as_millis(),
        truncated_by_safety_limit: truncated,
        drive_total_bytes,
        drive_free_bytes,
        drive_used_bytes,
        is_drive_root: is_root,
        children,
        category_breakdown,
        largest_files,
    })
}

/// 執行 TreeSize 磁碟空間分析（向後相容版同步 API）
pub fn analyze_directory(
    path_str: &str,
    custom_max_depth: Option<usize>,
) -> KeeperResult<TreeSizeReport> {
    analyze_directory_with_progress(path_str, custom_max_depth, None, |_| {})
}

// ==========================================
// 全域背景非同步掃描任務管理器
// ==========================================

struct ActiveScan {
    scan_id: String,
    target_path: String,
    status: ScanStatus,
    progress: TreeSizeProgress,
    cancel_flag: Arc<AtomicBool>,
    report: Option<TreeSizeReport>,
    error: Option<String>,
}

static SCAN_STATE: OnceLock<Mutex<Option<ActiveScan>>> = OnceLock::new();

fn get_scan_store() -> &'static Mutex<Option<ActiveScan>> {
    SCAN_STATE.get_or_init(|| Mutex::new(None))
}

/// 啟動背景非同步 TreeSize 空間掃描
pub fn start_scan(path: &str, custom_max_depth: Option<usize>) -> String {
    let store = get_scan_store();
    let scan_id = format!(
        "scan_{}_{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    );

    // 若有前一個未完成的掃描，發出取消訊號並終止舊任務
    {
        let lock = store.lock().unwrap();
        if let Some(ref current) = *lock {
            if current.status == ScanStatus::Scanning {
                current.cancel_flag.store(true, Ordering::SeqCst);
            }
        }
    }

    let cancel_flag = Arc::new(AtomicBool::new(false));
    let initial_state = ActiveScan {
        scan_id: scan_id.clone(),
        target_path: path.to_string(),
        status: ScanStatus::Scanning,
        progress: TreeSizeProgress {
            current_path: path.to_string(),
            processed_files: 0,
            processed_dirs: 0,
            processed_bytes: 0,
            elapsed_ms: 0,
            estimated_percent: None,
        },
        cancel_flag: Arc::clone(&cancel_flag),
        report: None,
        error: None,
    };

    {
        let mut lock = store.lock().unwrap();
        *lock = Some(initial_state);
    }

    let path_buf = path.to_string();
    let thread_scan_id = scan_id.clone();
    let thread_cancel = Arc::clone(&cancel_flag);

    let _ = std::thread::Builder::new()
        .name("treesize-scanner".to_string())
        .spawn(move || {
            let res = analyze_directory_with_progress(
                &path_buf,
                custom_max_depth,
                Some(Arc::clone(&thread_cancel)),
                |prog| {
                    let s = get_scan_store();
                    if let Ok(mut lock) = s.lock() {
                        if let Some(ref mut scan) = *lock {
                            if scan.scan_id == thread_scan_id {
                                scan.progress = prog.clone();
                            }
                        }
                    }
                },
            );

            let s = get_scan_store();
            if let Ok(mut lock) = s.lock() {
                if let Some(ref mut scan) = *lock {
                    if scan.scan_id == thread_scan_id {
                        match res {
                            Ok(report) => {
                                scan.status = ScanStatus::Completed;
                                scan.report = Some(report);
                            }
                            Err(KeeperError::OperationCancelled) => {
                                scan.status = ScanStatus::Cancelled;
                            }
                            Err(err) => {
                                scan.status = ScanStatus::Failed;
                                scan.error = Some(err.to_string());
                            }
                        }
                    }
                }
            }
        });

    scan_id
}

/// 查詢背景非同步 TreeSize 掃描進度與狀態
pub fn get_scan_progress(scan_id: &str) -> Option<TreeSizeScanState> {
    let store = get_scan_store();
    let lock = store.lock().ok()?;
    let scan = lock.as_ref()?;
    if scan.scan_id != scan_id {
        return None;
    }
    Some(TreeSizeScanState {
        scan_id: scan.scan_id.clone(),
        target_path: scan.target_path.clone(),
        status: scan.status,
        progress: scan.progress.clone(),
        report: scan.report.clone(),
        error: scan.error.clone(),
    })
}

/// 取消指定 scan_id 的 TreeSize 空間掃描
pub fn cancel_scan(scan_id: &str) -> bool {
    let store = get_scan_store();
    if let Ok(mut lock) = store.lock() {
        if let Some(ref mut scan) = *lock {
            if scan.scan_id == scan_id {
                scan.cancel_flag.store(true, Ordering::SeqCst);
                if scan.status == ScanStatus::Scanning {
                    scan.status = ScanStatus::Cancelled;
                }
                return true;
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;
    use tempfile::tempdir;

    #[test]
    fn test_treesize_analysis() {
        let dir = tempdir().expect("tempdir");
        let sub_a = dir.path().join("sub_a");
        let sub_b = dir.path().join("sub_b");
        fs::create_dir_all(&sub_a).unwrap();
        fs::create_dir_all(&sub_b).unwrap();

        fs::write(sub_a.join("photo.png"), vec![1u8; 4000]).unwrap();
        fs::write(sub_b.join("code.rs"), vec![2u8; 1000]).unwrap();

        let report = analyze_directory(&dir.path().to_string_lossy(), None).unwrap();
        assert_eq!(report.total_size_bytes, 5000);
        assert_eq!(report.total_files, 2);
        assert_eq!(report.children.len(), 2);
        assert_eq!(report.children[0].name, "sub_a");
        assert_eq!(report.children[0].size_bytes, 4000);
        assert!(report.parent_path.is_some());
        assert!(!report.is_drive_root);
    }

    #[test]
    fn test_treesize_with_progress_callback() {
        let dir = tempdir().expect("tempdir");
        let sub = dir.path().join("folder");
        fs::create_dir_all(&sub).unwrap();
        for i in 0..10 {
            fs::write(sub.join(format!("file_{}.txt", i)), vec![0u8; 100]).unwrap();
        }

        let progress_calls = Arc::new(AtomicUsize::new(0));
        let progress_calls_clone = Arc::clone(&progress_calls);

        let report = analyze_directory_with_progress(
            &dir.path().to_string_lossy(),
            None,
            None,
            move |_prog| {
                progress_calls_clone.fetch_add(1, Ordering::SeqCst);
            },
        )
        .unwrap();

        assert_eq!(report.total_files, 10);
        assert_eq!(report.total_size_bytes, 1000);
        assert!(progress_calls.load(Ordering::SeqCst) > 0);
    }

    #[test]
    fn test_treesize_cancellation() {
        let dir = tempdir().expect("tempdir");
        let sub = dir.path().join("large_dir");
        fs::create_dir_all(&sub).unwrap();
        for i in 0..20 {
            fs::write(sub.join(format!("dummy_{}.bin", i)), vec![1u8; 50]).unwrap();
        }

        let cancel_flag = Arc::new(AtomicBool::new(true)); // 一開始即標記為取消
        let res = analyze_directory_with_progress(
            &dir.path().to_string_lossy(),
            None,
            Some(cancel_flag),
            |_| {},
        );

        assert!(matches!(res, Err(KeeperError::OperationCancelled)));
    }

    #[test]
    fn test_async_scan_manager() {
        let dir = tempdir().expect("tempdir");
        let sub = dir.path().join("async_test");
        fs::create_dir_all(&sub).unwrap();
        fs::write(sub.join("test.bin"), vec![9u8; 500]).unwrap();

        let scan_id = start_scan(&dir.path().to_string_lossy(), None);
        assert!(!scan_id.is_empty());

        // 等待背景任務完成
        let start = Instant::now();
        loop {
            if let Some(state) = get_scan_progress(&scan_id) {
                if state.status == ScanStatus::Completed {
                    assert_eq!(state.progress.processed_files, 1);
                    assert_eq!(state.progress.processed_bytes, 500);
                    assert!(state.report.is_some());
                    break;
                }
            }
            if start.elapsed().as_secs() > 5 {
                panic!("Async scan timed out in unit test");
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
    }

    #[test]
    fn test_drive_root_helper() {
        assert!(is_drive_root(Path::new(r"C:\")));
        assert!(is_drive_root(Path::new(r"D:")));
        assert!(is_drive_root(Path::new(r"\\?\E:\")));
        assert!(!is_drive_root(Path::new(r"C:\Windows")));
        assert!(!is_drive_root(Path::new(r"C:\Users\test")));
    }
}

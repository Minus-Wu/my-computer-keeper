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

/// 執行 TreeSize 磁碟空間分析
/// 針對本機磁碟提供無項目上限的深度掃描與精準容量對齊；針對網路磁碟機自動套用受控深度與項目上限保護
pub fn analyze_directory(
    path_str: &str,
    custom_max_depth: Option<usize>,
) -> KeeperResult<TreeSizeReport> {
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

    let walker = WalkDir::new(&clean_root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter();

    for (idx, entry_res) in walker.enumerate() {
        if idx >= max_entries {
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
    }

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

#[cfg(test)]
mod tests {
    use super::*;
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
    fn test_drive_root_helper() {
        assert!(is_drive_root(Path::new(r"C:\")));
        assert!(is_drive_root(Path::new(r"D:")));
        assert!(is_drive_root(Path::new(r"\\?\E:\")));
        assert!(!is_drive_root(Path::new(r"C:\Windows")));
        assert!(!is_drive_root(Path::new(r"C:\Users\test")));
    }
}

use crate::drive::{inspect_path_capabilities, normalize_display_path, VolumeCapabilities};
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

/// 執行 TreeSize 磁碟空間分析（針對本機與網路磁碟機自動套用對應掃描深度與項目上限）
pub fn analyze_directory(path_str: &str, custom_max_depth: Option<usize>) -> KeeperResult<TreeSizeReport> {
    let start = Instant::now();
    let root = Path::new(path_str);
    if !root.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let canonical = fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf());
    let clean_root = normalize_display_path(&canonical);
    let capabilities = inspect_path_capabilities(&clean_root);

    let max_depth = custom_max_depth
        .unwrap_or(capabilities.max_scan_depth)
        .min(capabilities.max_scan_depth);
    let max_entries = capabilities.max_scan_entries;

    let mut total_size_bytes = 0u64;
    let mut total_files = 0usize;
    let mut total_dirs = 0usize;
    let mut truncated = false;

    // 記錄第一層子項目（供 Treemap 與階層列表下鑽使用）
    let mut top_child_map: HashMap<String, (String, bool, u64, usize, FileCategory)> =
        HashMap::new();
    let mut category_map: HashMap<String, (FileCategory, u64, usize)> = HashMap::new();
    let mut all_files: Vec<LargestFileItem> = Vec::new();

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
        if entry.depth() == 0 {
            continue;
        }

        let entry_path = normalize_display_path(entry.path());
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        // 找出屬於 clean_root 下第一層的名稱與路徑
        let rel = match entry_path.strip_prefix(&clean_root) {
            Ok(r) => r,
            Err(_) => continue,
        };
        let first_comp = match rel.components().next() {
            Some(c) => c.as_os_str().to_string_lossy().to_string(),
            None => continue,
        };
        let first_path = clean_root.join(&first_comp);
        let first_is_dir = first_path.is_dir();

        if meta.is_dir() {
            total_dirs += 1;
            top_child_map.entry(first_comp.clone()).or_insert_with(|| {
                (
                    first_path.to_string_lossy().to_string(),
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

            let ext = entry_path
                .extension()
                .map(|e| e.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default();
            let (cat, is_image) = classify_extension(&ext, false);

            let top_entry = top_child_map.entry(first_comp.clone()).or_insert_with(|| {
                (
                    first_path.to_string_lossy().to_string(),
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

            let cat_key = format!("{:?}", cat);
            let cat_stat = category_map.entry(cat_key).or_insert((cat, 0, 0));
            cat_stat.1 = cat_stat.1.saturating_add(file_size);
            cat_stat.2 += 1;

            all_files.push(LargestFileItem {
                name: entry.file_name().to_string_lossy().to_string(),
                path: entry_path.to_string_lossy().to_string(),
                size_bytes: file_size,
                category: cat,
                is_image,
            });
        }
    }

    let mut children: Vec<TreeSizeNode> = top_child_map
        .into_iter()
        .map(|(name, (path, is_dir, size_bytes, file_count, category))| {
            let percentage = if total_size_bytes > 0 {
                ((size_bytes as f64 / total_size_bytes as f64) * 100.0) as f32
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
        })
        .collect();
    children.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));

    let mut category_breakdown: Vec<CategoryStat> = category_map
        .into_values()
        .map(|(category, size_bytes, file_count)| {
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

    all_files.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
    all_files.truncate(20);

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
        children,
        category_breakdown,
        largest_files: all_files,
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
    }
}

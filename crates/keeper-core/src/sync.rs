use crate::drive::{inspect_path_capabilities, normalize_display_path, VolumeCapabilities};
use crate::{KeeperError, KeeperResult};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;
use walkdir::WalkDir;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareMode {
    TimeAndSize,
    Blake3Hash,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncStrategy {
    Mirror,
    Update,
    TwoWay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffStatus {
    OnlyLeft,
    OnlyRight,
    LeftNewer,
    RightNewer,
    DifferentContent,
    Identical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncAction {
    CopyLeftToRight,
    CopyRightToLeft,
    DeleteRight,
    SkipIdentical,
    ConflictManual,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncDiffItem {
    pub relative_path: String,
    pub left_size: Option<u64>,
    pub right_size: Option<u64>,
    pub left_modified_unix: Option<u64>,
    pub right_modified_unix: Option<u64>,
    pub status: DiffStatus,
    pub proposed_action: SyncAction,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncComparisonReport {
    pub left_root: String,
    pub right_root: String,
    pub left_capabilities: VolumeCapabilities,
    pub right_capabilities: VolumeCapabilities,
    pub compare_mode: CompareMode,
    pub strategy: SyncStrategy,
    pub timestamp_tolerance_secs: u64,
    pub only_left_count: usize,
    pub only_right_count: usize,
    pub left_newer_count: usize,
    pub right_newer_count: usize,
    pub different_content_count: usize,
    pub identical_count: usize,
    pub bytes_to_transfer: u64,
    pub network_advisory: Option<String>,
    pub items: Vec<SyncDiffItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncExecutionSummary {
    pub dry_run: bool,
    pub copied_left_to_right: usize,
    pub copied_right_to_left: usize,
    pub deleted_right: usize,
    pub bytes_copied: u64,
    pub errors: Vec<String>,
}

#[derive(Debug, Clone)]
struct ScannedFileMeta {
    abs_path: PathBuf,
    size: u64,
    modified_unix: u64,
}

fn scan_flat_files(root: &Path, max_depth: usize, max_entries: usize) -> HashMap<String, ScannedFileMeta> {
    let mut map = HashMap::new();
    for (idx, entry) in WalkDir::new(root)
        .max_depth(max_depth)
        .follow_links(false)
        .into_iter()
        .flatten()
        .enumerate()
    {
        if idx >= max_entries {
            break;
        }
        if !entry.file_type().is_file() {
            continue;
        }
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let rel = match entry.path().strip_prefix(root) {
            Ok(r) => r.to_string_lossy().replace('\\', "/"),
            Err(_) => continue,
        };
        let modified_unix = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        map.insert(
            rel,
            ScannedFileMeta {
                abs_path: entry.path().to_path_buf(),
                size: meta.len(),
                modified_unix,
            },
        );
    }
    map
}

fn compute_blake3_hash(path: &Path) -> KeeperResult<blake3::Hash> {
    let mut file = fs::File::open(path)?;
    let mut hasher = blake3::Hasher::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buffer)?;
        if n == 0 {
            break;
        }
        hasher.update(&buffer[..n]);
    }
    Ok(hasher.finalize())
}

fn decide_action(status: DiffStatus, strategy: SyncStrategy) -> SyncAction {
    match (strategy, status) {
        (_, DiffStatus::Identical) => SyncAction::SkipIdentical,
        (SyncStrategy::Mirror, DiffStatus::OnlyLeft)
        | (SyncStrategy::Mirror, DiffStatus::LeftNewer)
        | (SyncStrategy::Mirror, DiffStatus::RightNewer)
        | (SyncStrategy::Mirror, DiffStatus::DifferentContent) => SyncAction::CopyLeftToRight,
        (SyncStrategy::Mirror, DiffStatus::OnlyRight) => SyncAction::DeleteRight,

        (SyncStrategy::Update, DiffStatus::OnlyLeft)
        | (SyncStrategy::Update, DiffStatus::LeftNewer)
        | (SyncStrategy::Update, DiffStatus::DifferentContent) => SyncAction::CopyLeftToRight,
        (SyncStrategy::Update, DiffStatus::OnlyRight)
        | (SyncStrategy::Update, DiffStatus::RightNewer) => SyncAction::SkipIdentical,

        (SyncStrategy::TwoWay, DiffStatus::OnlyLeft)
        | (SyncStrategy::TwoWay, DiffStatus::LeftNewer) => SyncAction::CopyLeftToRight,
        (SyncStrategy::TwoWay, DiffStatus::OnlyRight)
        | (SyncStrategy::TwoWay, DiffStatus::RightNewer) => SyncAction::CopyRightToLeft,
        (SyncStrategy::TwoWay, DiffStatus::DifferentContent) => SyncAction::ConflictManual,
    }
}

/// 比對左右兩個資料夾並產生 FreeFileSync 差異報告（支援網路磁碟機 2 秒時間戳容差）
pub fn compare_directories(
    left_str: &str,
    right_str: &str,
    compare_mode: CompareMode,
    strategy: SyncStrategy,
) -> KeeperResult<SyncComparisonReport> {
    let left_path = Path::new(left_str);
    let right_path = Path::new(right_str);

    if !left_path.exists() {
        return Err(KeeperError::PathNotFound(format!(
            "左側來源目錄不存在: {}",
            left_str
        )));
    }
    if !right_path.exists() {
        return Err(KeeperError::PathNotFound(format!(
            "右側目標目錄不存在: {}",
            right_str
        )));
    }

    let left_clean = normalize_display_path(&fs::canonicalize(left_path).unwrap_or_else(|_| left_path.to_path_buf()));
    let right_clean = normalize_display_path(&fs::canonicalize(right_path).unwrap_or_else(|_| right_path.to_path_buf()));

    let left_caps = inspect_path_capabilities(&left_clean);
    let right_caps = inspect_path_capabilities(&right_clean);
    let involves_network = left_caps.is_network_drive || right_caps.is_network_drive;

    let tolerance_secs = left_caps
        .sync_timestamp_tolerance_secs
        .max(right_caps.sync_timestamp_tolerance_secs);

    let max_depth = left_caps.max_scan_depth.min(right_caps.max_scan_depth);
    let max_entries = left_caps.max_scan_entries.min(right_caps.max_scan_entries);

    let left_files = scan_flat_files(&left_clean, max_depth, max_entries);
    let right_files = scan_flat_files(&right_clean, max_depth, max_entries);

    let mut all_keys: BTreeSet<String> = BTreeSet::new();
    for k in left_files.keys() {
        all_keys.insert(k.clone());
    }
    for k in right_files.keys() {
        all_keys.insert(k.clone());
    }

    let mut only_left_count = 0usize;
    let mut only_right_count = 0usize;
    let mut left_newer_count = 0usize;
    let mut right_newer_count = 0usize;
    let mut different_content_count = 0usize;
    let mut identical_count = 0usize;
    let mut bytes_to_transfer = 0u64;
    let mut items = Vec::with_capacity(all_keys.len());

    // 若涉及網路磁碟且選用 BLAKE3 雜湊，對超過 50MB 的檔案改用時間+大小比對，防止跨網路讀取大檔造成凍結
    let max_network_hash_bytes = 50 * 1024 * 1024u64;

    for rel in all_keys {
        let l = left_files.get(&rel);
        let r = right_files.get(&rel);

        let status = match (l, r) {
            (Some(_), None) => {
                only_left_count += 1;
                DiffStatus::OnlyLeft
            }
            (None, Some(_)) => {
                only_right_count += 1;
                DiffStatus::OnlyRight
            }
            (Some(lm), Some(rm)) => {
                if lm.size != rm.size {
                    if lm.modified_unix > rm.modified_unix + tolerance_secs {
                        left_newer_count += 1;
                        DiffStatus::LeftNewer
                    } else if rm.modified_unix > lm.modified_unix + tolerance_secs {
                        right_newer_count += 1;
                        DiffStatus::RightNewer
                    } else {
                        different_content_count += 1;
                        DiffStatus::DifferentContent
                    }
                } else {
                    match compare_mode {
                        CompareMode::TimeAndSize => {
                            let diff = lm.modified_unix.abs_diff(rm.modified_unix);
                            if diff <= tolerance_secs {
                                identical_count += 1;
                                DiffStatus::Identical
                            } else if lm.modified_unix > rm.modified_unix {
                                left_newer_count += 1;
                                DiffStatus::LeftNewer
                            } else {
                                right_newer_count += 1;
                                DiffStatus::RightNewer
                            }
                        }
                        CompareMode::Blake3Hash => {
                            if involves_network && lm.size > max_network_hash_bytes {
                                let diff = lm.modified_unix.abs_diff(rm.modified_unix);
                                if diff <= tolerance_secs {
                                    identical_count += 1;
                                    DiffStatus::Identical
                                } else if lm.modified_unix > rm.modified_unix {
                                    left_newer_count += 1;
                                    DiffStatus::LeftNewer
                                } else {
                                    right_newer_count += 1;
                                    DiffStatus::RightNewer
                                }
                            } else {
                                let lh = compute_blake3_hash(&lm.abs_path).ok();
                                let rh = compute_blake3_hash(&rm.abs_path).ok();
                                if lh.is_some() && lh == rh {
                                    identical_count += 1;
                                    DiffStatus::Identical
                                } else {
                                    different_content_count += 1;
                                    DiffStatus::DifferentContent
                                }
                            }
                        }
                    }
                }
            }
            (None, None) => unreachable!(),
        };

        let proposed_action = decide_action(status, strategy);
        match proposed_action {
            SyncAction::CopyLeftToRight => {
                if let Some(lm) = l {
                    bytes_to_transfer = bytes_to_transfer.saturating_add(lm.size);
                }
            }
            SyncAction::CopyRightToLeft => {
                if let Some(rm) = r {
                    bytes_to_transfer = bytes_to_transfer.saturating_add(rm.size);
                }
            }
            _ => {}
        }

        items.push(SyncDiffItem {
            relative_path: rel,
            left_size: l.map(|m| m.size),
            right_size: r.map(|m| m.size),
            left_modified_unix: l.map(|m| m.modified_unix),
            right_modified_unix: r.map(|m| m.modified_unix),
            status,
            proposed_action,
        });
    }

    let network_advisory = if involves_network {
        Some(format!(
            "🌐 偵測到網路磁碟機參與同步：已自動啟用 {} 秒 SMB 時間戳記容差，並針對大型遠端檔案啟用頻寬保護機制。",
            tolerance_secs
        ))
    } else {
        None
    };

    Ok(SyncComparisonReport {
        left_root: left_clean.to_string_lossy().to_string(),
        right_root: right_clean.to_string_lossy().to_string(),
        left_capabilities: left_caps,
        right_capabilities: right_caps,
        compare_mode,
        strategy,
        timestamp_tolerance_secs: tolerance_secs,
        only_left_count,
        only_right_count,
        left_newer_count,
        right_newer_count,
        different_content_count,
        identical_count,
        bytes_to_transfer,
        network_advisory,
        items,
    })
}

/// 執行目錄同步（支援 Dry-Run 預演模式或實際同步）
pub fn execute_sync(
    left_str: &str,
    right_str: &str,
    compare_mode: CompareMode,
    strategy: SyncStrategy,
    dry_run: bool,
) -> KeeperResult<SyncExecutionSummary> {
    let report = compare_directories(left_str, right_str, compare_mode, strategy)?;
    let left_base = Path::new(&report.left_root);
    let right_base = Path::new(&report.right_root);

    let mut copied_left_to_right = 0usize;
    let mut copied_right_to_left = 0usize;
    let mut deleted_right = 0usize;
    let mut bytes_copied = 0u64;
    let mut errors = Vec::new();

    for item in &report.items {
        let rel_path = Path::new(&item.relative_path);
        let left_target = left_base.join(rel_path);
        let right_target = right_base.join(rel_path);

        match item.proposed_action {
            SyncAction::CopyLeftToRight => {
                if !dry_run {
                    if let Some(parent) = right_target.parent() {
                        if let Err(e) = fs::create_dir_all(parent) {
                            errors.push(format!("建立目錄失敗 {}: {}", parent.display(), e));
                            continue;
                        }
                    }
                    match fs::copy(&left_target, &right_target) {
                        Ok(bytes) => {
                            copied_left_to_right += 1;
                            bytes_copied = bytes_copied.saturating_add(bytes);
                        }
                        Err(e) => errors.push(format!("複製失敗 {}: {}", item.relative_path, e)),
                    }
                } else {
                    copied_left_to_right += 1;
                    bytes_copied = bytes_copied.saturating_add(item.left_size.unwrap_or(0));
                }
            }
            SyncAction::CopyRightToLeft => {
                if !dry_run {
                    if let Some(parent) = left_target.parent() {
                        if let Err(e) = fs::create_dir_all(parent) {
                            errors.push(format!("建立目錄失敗 {}: {}", parent.display(), e));
                            continue;
                        }
                    }
                    match fs::copy(&right_target, &left_target) {
                        Ok(bytes) => {
                            copied_right_to_left += 1;
                            bytes_copied = bytes_copied.saturating_add(bytes);
                        }
                        Err(e) => errors.push(format!("反向複製失敗 {}: {}", item.relative_path, e)),
                    }
                } else {
                    copied_right_to_left += 1;
                    bytes_copied = bytes_copied.saturating_add(item.right_size.unwrap_or(0));
                }
            }
            SyncAction::DeleteRight => {
                if !dry_run {
                    match fs::remove_file(&right_target) {
                        Ok(_) => deleted_right += 1,
                        Err(e) => errors.push(format!("刪除失敗 {}: {}", item.relative_path, e)),
                    }
                } else {
                    deleted_right += 1;
                }
            }
            SyncAction::SkipIdentical | SyncAction::ConflictManual => {}
        }
    }

    Ok(SyncExecutionSummary {
        dry_run,
        copied_left_to_right,
        copied_right_to_left,
        deleted_right,
        bytes_copied,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_freefilesync_compare_and_mirror_execution() {
        let left_dir = tempdir().unwrap();
        let right_dir = tempdir().unwrap();

        fs::write(left_dir.path().join("common.txt"), "same-content").unwrap();
        fs::write(right_dir.path().join("common.txt"), "same-content").unwrap();
        fs::write(left_dir.path().join("only_left.txt"), "new-file-data").unwrap();
        fs::write(right_dir.path().join("obsolete.txt"), "old-data").unwrap();

        let report = compare_directories(
            &left_dir.path().to_string_lossy(),
            &right_dir.path().to_string_lossy(),
            CompareMode::Blake3Hash,
            SyncStrategy::Mirror,
        )
        .unwrap();

        assert_eq!(report.identical_count, 1);
        assert_eq!(report.only_left_count, 1);
        assert_eq!(report.only_right_count, 1);

        let exec = execute_sync(
            &left_dir.path().to_string_lossy(),
            &right_dir.path().to_string_lossy(),
            CompareMode::Blake3Hash,
            SyncStrategy::Mirror,
            false,
        )
        .unwrap();

        assert_eq!(exec.copied_left_to_right, 1);
        assert_eq!(exec.deleted_right, 1);
        assert!(right_dir.path().join("only_left.txt").exists());
        assert!(!right_dir.path().join("obsolete.txt").exists());
    }
}

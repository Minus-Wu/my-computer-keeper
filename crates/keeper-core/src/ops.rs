use crate::drive::{inspect_path_capabilities, list_all_drives, normalize_display_path, VolumeCapabilities};
use crate::explorer::classify_extension;
use crate::{KeeperError, KeeperResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateGroup {
    pub blake3_hash: String,
    pub file_size_bytes: u64,
    pub wasted_bytes: u64,
    pub is_image: bool,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DuplicateScanReport {
    pub root_path: String,
    pub capabilities: VolumeCapabilities,
    pub total_groups: usize,
    pub total_wasted_bytes: u64,
    pub groups: Vec<DuplicateGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JunkArtifactItem {
    pub kind: String,
    pub path: String,
    pub size_bytes: u64,
    pub file_count: usize,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JunkScanReport {
    pub scanned_root: String,
    pub total_reclaimable_bytes: u64,
    pub items: Vec<JunkArtifactItem>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PathHealthStatus {
    ValidLocal,
    MissingDirectory,
    NetworkPathWarning,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EnvPathDiagnosis {
    pub index: usize,
    pub path: String,
    pub status: PathHealthStatus,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartupEntry {
    pub name: String,
    pub command_or_path: String,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSmartSummary {
    pub mount_point: String,
    pub name: String,
    pub is_network_drive: bool,
    pub smart_supported: bool,
    pub health_badge: String,
    pub usage_percent: f32,
    pub free_bytes: u64,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemMaintenanceOverview {
    pub path_diagnostics: Vec<EnvPathDiagnosis>,
    pub broken_path_count: usize,
    pub network_path_count: usize,
    pub startup_entries: Vec<StartupEntry>,
    pub disk_health_summaries: Vec<DiskSmartSummary>,
    pub temp_dir_path: String,
    pub temp_dir_size_bytes: u64,
}

fn hash_file_blake3(path: &Path) -> Option<String> {
    let mut file = fs::File::open(path).ok()?;
    let mut hasher = blake3::Hasher::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf).ok()?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Some(hasher.finalize().to_hex().to_string())
}

/// 掃描重複檔案與重複圖片（針對網路磁碟機自動套用檔案大小上限以保護頻寬）
pub fn find_duplicates(path_str: &str) -> KeeperResult<DuplicateScanReport> {
    let root = Path::new(path_str);
    if !root.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let clean_root = normalize_display_path(&fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()));
    let capabilities = inspect_path_capabilities(&clean_root);

    let max_depth = capabilities.max_scan_depth.min(8);
    let max_entries = capabilities.max_scan_entries.min(15_000);
    let max_file_size = if capabilities.is_network_drive {
        25 * 1024 * 1024 // 網路磁碟機限制 25MB
    } else {
        500 * 1024 * 1024
    };

    let mut size_groups: HashMap<u64, Vec<std::path::PathBuf>> = HashMap::new();
    for (idx, entry) in WalkDir::new(&clean_root)
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
        if let Ok(meta) = entry.metadata() {
            let len = meta.len();
            if len > 0 && len <= max_file_size {
                size_groups
                    .entry(len)
                    .or_default()
                    .push(normalize_display_path(entry.path()));
            }
        }
    }

    let mut groups = Vec::new();
    let mut total_wasted_bytes = 0u64;

    for (file_size, candidates) in size_groups {
        if candidates.len() < 2 {
            continue;
        }
        let mut hash_map: HashMap<String, Vec<String>> = HashMap::new();
        for p in candidates {
            if let Some(h) = hash_file_blake3(&p) {
                hash_map
                    .entry(h)
                    .or_default()
                    .push(p.to_string_lossy().to_string());
            }
        }
        for (hash, files) in hash_map {
            if files.len() >= 2 {
                let wasted = file_size.saturating_mul((files.len() - 1) as u64);
                total_wasted_bytes = total_wasted_bytes.saturating_add(wasted);
                let first_ext = Path::new(&files[0])
                    .extension()
                    .map(|e| e.to_string_lossy().to_ascii_lowercase())
                    .unwrap_or_default();
                let (_, is_image) = classify_extension(&first_ext, false);

                groups.push(DuplicateGroup {
                    blake3_hash: hash[..16].to_string(),
                    file_size_bytes: file_size,
                    wasted_bytes: wasted,
                    is_image,
                    files,
                });
            }
        }
    }

    groups.sort_by_key(|a| std::cmp::Reverse(a.wasted_bytes));

    Ok(DuplicateScanReport {
        root_path: clean_root.to_string_lossy().to_string(),
        capabilities,
        total_groups: groups.len(),
        total_wasted_bytes,
        groups,
    })
}

fn compute_dir_size_fast(dir: &Path, max_items: usize) -> (u64, usize) {
    let mut bytes = 0u64;
    let mut count = 0usize;
    for (idx, entry) in WalkDir::new(dir).follow_links(false).into_iter().flatten().enumerate() {
        if idx >= max_items {
            break;
        }
        if entry.file_type().is_file() {
            if let Ok(m) = entry.metadata() {
                bytes = bytes.saturating_add(m.len());
                count += 1;
            }
        }
    }
    (bytes, count)
}

/// 掃描指定目錄下的開發者建置快取 (`target`, `node_modules`, `.venv`, `__pycache__`, `dist`)
pub fn scan_junk_artifacts(path_str: &str) -> KeeperResult<JunkScanReport> {
    let root = Path::new(path_str);
    if !root.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let clean_root = normalize_display_path(&fs::canonicalize(root).unwrap_or_else(|_| root.to_path_buf()));
    let mut items = Vec::new();
    let mut total_reclaimable_bytes = 0u64;

    let known_junk_dirs: HashMap<&str, (&str, &str)> = HashMap::from([
        ("target", ("Rust Build Cache", "Cargo 編譯產物快取目錄")),
        ("node_modules", ("Node.js Packages", "NPM/PNPM/Yarn 套件依賴快取")),
        (".venv", ("Python VirtualEnv", "Python 虛擬環境目錄")),
        ("__pycache__", ("Python Bytecode", "Python 編譯位元組快取")),
        (".next", ("Next.js Cache", "Next.js 建置快取目錄")),
        (".vite", ("Vite Cache", "Vite 預編譯快取目錄")),
        ("dist", ("Frontend Bundle", "前端打包輸出目錄")),
    ]);

    let mut it = WalkDir::new(&clean_root)
        .max_depth(5)
        .follow_links(false)
        .into_iter();

    while let Some(entry_res) = it.next() {
        let entry = match entry_res {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_dir() || entry.depth() == 0 {
            continue;
        }
        let dir_name = entry.file_name().to_string_lossy();
        if let Some(&(kind, desc)) = known_junk_dirs.get(dir_name.as_ref()) {
            let p = normalize_display_path(entry.path());
            let (size_bytes, file_count) = compute_dir_size_fast(&p, 15_000);
            if size_bytes > 0 {
                total_reclaimable_bytes = total_reclaimable_bytes.saturating_add(size_bytes);
                items.push(JunkArtifactItem {
                    kind: kind.to_string(),
                    path: p.to_string_lossy().to_string(),
                    size_bytes,
                    file_count,
                    description: desc.to_string(),
                });
            }
            it.skip_current_dir();
        }
    }

    items.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));

    Ok(JunkScanReport {
        scanned_root: clean_root.to_string_lossy().to_string(),
        total_reclaimable_bytes,
        items,
    })
}

/// 執行電腦本體健康度、PATH 環境變數、開機啟動項與磁碟（含網路磁碟）體檢
pub fn inspect_system_maintenance() -> SystemMaintenanceOverview {
    let mut path_diagnostics = Vec::new();
    let mut broken_path_count = 0usize;
    let mut network_path_count = 0usize;

    if let Ok(path_var) = std::env::var("PATH") {
        for (idx, raw_entry) in std::env::split_paths(&path_var).enumerate() {
            let s = raw_entry.to_string_lossy().trim().to_string();
            if s.is_empty() {
                continue;
            }
            let p = Path::new(&s);
            let caps = inspect_path_capabilities(p);
            if caps.is_network_drive {
                network_path_count += 1;
                path_diagnostics.push(EnvPathDiagnosis {
                    index: idx + 1,
                    path: s,
                    status: PathHealthStatus::NetworkPathWarning,
                    note: "🌐 指向網路磁碟機，當網路離線時可能會造成終端機或程式啟動卡頓".to_string(),
                });
            } else if !p.exists() {
                broken_path_count += 1;
                path_diagnostics.push(EnvPathDiagnosis {
                    index: idx + 1,
                    path: s,
                    status: PathHealthStatus::MissingDirectory,
                    note: "⚠️ 目錄已不存在（軟體卸載殘留），建議從 PATH 清理".to_string(),
                });
            } else {
                path_diagnostics.push(EnvPathDiagnosis {
                    index: idx + 1,
                    path: s,
                    status: PathHealthStatus::ValidLocal,
                    note: "正常本機路徑".to_string(),
                });
            }
        }
    }

    // 讀取啟動資料夾項目
    let mut startup_entries = Vec::new();
    if let Ok(appdata) = std::env::var("APPDATA") {
        let startup_dir = Path::new(&appdata)
            .join("Microsoft")
            .join("Windows")
            .join("Start Menu")
            .join("Programs")
            .join("Startup");
        if let Ok(rd) = fs::read_dir(&startup_dir) {
            for item in rd.flatten() {
                let name = item.file_name().to_string_lossy().to_string();
                if name.eq_ignore_ascii_case("desktop.ini") {
                    continue;
                }
                startup_entries.push(StartupEntry {
                    name,
                    command_or_path: item.path().to_string_lossy().to_string(),
                    source: "Startup Folder (使用者啟動資料夾)".to_string(),
                });
            }
        }
    }

    // 整理所有磁碟機（含網路磁碟機有限功能標示）的健康度摘要
    let drives = list_all_drives();
    let disk_health_summaries: Vec<DiskSmartSummary> = drives
        .into_iter()
        .map(|d| {
            let is_net = d.capabilities.is_network_drive;
            let smart_supported = d.capabilities.supports_smart_health;
            let health_badge = if is_net {
                "🌐 網路磁碟 (不適用 S.M.A.R.T.)".to_string()
            } else if d.usage_percent > 90.0 {
                "⚠️ 空間吃緊 (<10% 可用)".to_string()
            } else {
                "🟢 良好 (Healthy)".to_string()
            };
            let note = if is_net {
                format!(
                    "遠端掛載路徑：{}（已自動套用網路磁碟有限功能保護）",
                    d.capabilities
                        .remote_unc_path
                        .as_deref()
                        .unwrap_or("SMB/UNC Share")
                )
            } else {
                format!("檔案系統：{} | 支援完整 MFT 掃描與本機回收筒", d.file_system)
            };

            DiskSmartSummary {
                mount_point: d.mount_point,
                name: d.name,
                is_network_drive: is_net,
                smart_supported,
                health_badge,
                usage_percent: d.usage_percent,
                free_bytes: d.available_bytes,
                note,
            }
        })
        .collect();

    let temp_dir = std::env::temp_dir();
    let (temp_size, _) = compute_dir_size_fast(&temp_dir, 4_000);

    SystemMaintenanceOverview {
        path_diagnostics,
        broken_path_count,
        network_path_count,
        startup_entries,
        disk_health_summaries,
        temp_dir_path: normalize_display_path(&temp_dir)
            .to_string_lossy()
            .to_string(),
        temp_dir_size_bytes: temp_size,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_find_duplicates_and_junk_scan() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("dup1.png"), b"identical-image-bytes").unwrap();
        fs::write(dir.path().join("dup2.png"), b"identical-image-bytes").unwrap();
        fs::write(dir.path().join("unique.txt"), b"unique-bytes").unwrap();

        let dup_report = find_duplicates(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(dup_report.total_groups, 1);
        assert!(dup_report.groups[0].is_image);
        assert_eq!(dup_report.groups[0].files.len(), 2);

        let fake_nm = dir.path().join("node_modules");
        fs::create_dir_all(&fake_nm).unwrap();
        fs::write(fake_nm.join("pkg.js"), vec![0u8; 2048]).unwrap();

        let junk_report = scan_junk_artifacts(&dir.path().to_string_lossy()).unwrap();
        assert_eq!(junk_report.items.len(), 1);
        assert_eq!(junk_report.total_reclaimable_bytes, 2048);
    }
}

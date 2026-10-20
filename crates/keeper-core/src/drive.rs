use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use sysinfo::Disks;

/// 磁碟機類型定義
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DriveKind {
    Fixed,
    Removable,
    Network,
    CdRom,
    RamDisk,
    Unknown,
}

/// LockHunter 在不同磁碟類型上的支援範圍
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LockHunterScope {
    /// 本機磁碟：完整支援所有本機行程鎖定查詢、強制釋放與下次重開機刪除
    FullLocal,
    /// 網路磁碟：僅能查詢「當前電腦」開啟該遠端檔案的行程，無法查詢遠端 NAS/Server 其他主機鎖定，亦不支援開機刪除
    LocalProcessOnly,
}

/// 磁碟區功能權限與網路磁碟機「有限功能」安全降級矩陣
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeCapabilities {
    pub drive_kind: DriveKind,
    pub is_network_drive: bool,
    pub remote_unc_path: Option<String>,
    pub supports_mft_fast_scan: bool,
    pub supports_recycle_bin: bool,
    pub supports_smart_health: bool,
    pub supports_boot_delete: bool,
    pub lockhunter_scope: LockHunterScope,
    pub max_scan_threads: usize,
    pub max_scan_depth: usize,
    pub max_scan_entries: usize,
    pub max_auto_preview_bytes: u64,
    pub sync_timestamp_tolerance_secs: u64,
    pub limitations_notice: Vec<String>,
}

impl VolumeCapabilities {
    /// 本機磁碟（Fixed / Removable）的完整功能配置
    pub fn full_local(drive_kind: DriveKind) -> Self {
        Self {
            drive_kind,
            is_network_drive: false,
            remote_unc_path: None,
            supports_mft_fast_scan: drive_kind == DriveKind::Fixed,
            supports_recycle_bin: drive_kind == DriveKind::Fixed,
            supports_smart_health: matches!(drive_kind, DriveKind::Fixed | DriveKind::Removable),
            supports_boot_delete: drive_kind == DriveKind::Fixed,
            lockhunter_scope: LockHunterScope::FullLocal,
            max_scan_threads: 16,
            max_scan_depth: 64,
            max_scan_entries: 5_000_000,
            max_auto_preview_bytes: 100 * 1024 * 1024, // 100 MB
            sync_timestamp_tolerance_secs: 0,
            limitations_notice: Vec::new(),
        }
    }

    /// 網路磁碟機（SMB / Mapped Drive / UNC）的「有限功能（Safe Mode）」配置
    pub fn limited_network(remote_unc_path: Option<String>) -> Self {
        let unc_label = remote_unc_path
            .as_deref()
            .unwrap_or("遠端網路共享 (SMB/UNC)");
        Self {
            drive_kind: DriveKind::Network,
            is_network_drive: true,
            remote_unc_path: remote_unc_path.clone(),
            supports_mft_fast_scan: false,
            supports_recycle_bin: false,
            supports_smart_health: false,
            supports_boot_delete: false,
            lockhunter_scope: LockHunterScope::LocalProcessOnly,
            max_scan_threads: 4,
            max_scan_depth: 6,
            max_scan_entries: 25_000,
            max_auto_preview_bytes: 15 * 1024 * 1024, // 15 MB
            sync_timestamp_tolerance_secs: 2,
            limitations_notice: vec![
                format!("已啟用網路磁碟有限功能模式 ({})", unc_label),
                "檔案總管：超過 15 MB 的遠端圖片需手動點擊載入，且刪除檔案時將直接永久刪除（不支援本機資源回收筒）。".to_string(),
                "TreeSize：停用底層 MFT 讀取，並限制掃描深度 (6 層) 與併發執行緒 (4)，保護遠端 SMB 伺服器。".to_string(),
                "LockHunter：僅能檢測「本機行程」開啟的檔案句柄，無法查看遠端伺服器其他使用者鎖定，且不支援重開機刪除。".to_string(),
                "FreeFileSync：自動套用 2 秒時間戳記容差（相容 NAS/Linux 檔案系統），避免因精度差異誤判修改。".to_string(),
                "系統維運：不支援遠端網路磁碟的 S.M.A.R.T. 硬體健康度讀取。".to_string(),
            ],
        }
    }
}

/// 磁碟機完整資訊
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DriveInfo {
    pub mount_point: String,
    pub name: String,
    pub file_system: String,
    pub total_bytes: u64,
    pub available_bytes: u64,
    pub used_bytes: u64,
    pub usage_percent: f32,
    pub capabilities: VolumeCapabilities,
}

/// 判斷字串路徑是否為 UNC 網路路徑（如 `\\server\share` 或 `//server/share`，排除 `\\?\C:\` 本機長路徑前綴）
pub fn is_unc_network_path(path_str: &str) -> bool {
    let trimmed = path_str.trim();
    if let Some(rest) = trimmed.strip_prefix(r"\\?\") {
        return rest.to_ascii_uppercase().starts_with("UNC\\");
    }
    if trimmed.starts_with(r"\\.\") {
        return false;
    }
    trimmed.starts_with(r"\\") || trimmed.starts_with("//")
}

#[cfg(windows)]
/// 判斷路徑是否為磁碟機根目錄（例如 `C:\`、`D:`、`\\?\C:\`）
pub fn is_drive_root(path: &Path) -> bool {
    let s = path.to_string_lossy();
    let trimmed = s.trim_start_matches(r"\\?\").trim();
    if trimmed.len() <= 3 {
        let bytes = trimmed.as_bytes();
        if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
            return bytes.len() == 2 || (bytes.len() == 3 && (bytes[2] == b'\\' || bytes[2] == b'/'));
        }
    }
    false
}

/// 查詢指定路徑所屬磁碟區的空間狀態：(總容量, 可用容量, 已用容量)
#[cfg(windows)]
pub fn query_volume_space(path: &Path) -> Option<(u64, u64, u64)> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;

    let path_str = path.to_string_lossy();
    let mut normalized = path_str.to_string();
    if !normalized.ends_with('\\') && !normalized.ends_with('/') {
        normalized.push('\\');
    }
    let wide: Vec<u16> = std::ffi::OsStr::new(&normalized)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    let mut free_bytes_available: u64 = 0;
    let mut total_number_of_bytes: u64 = 0;
    let mut total_number_of_free_bytes: u64 = 0;

    let res = unsafe {
        GetDiskFreeSpaceExW(
            wide.as_ptr(),
            &mut free_bytes_available as *mut _ as *mut _,
            &mut total_number_of_bytes as *mut _ as *mut _,
            &mut total_number_of_free_bytes as *mut _ as *mut _,
        )
    };
    if res != 0 && total_number_of_bytes > 0 {
        let used = total_number_of_bytes.saturating_sub(total_number_of_free_bytes);
        Some((total_number_of_bytes, total_number_of_free_bytes, used))
    } else {
        None
    }
}

#[cfg(not(windows))]
pub fn query_volume_space(_path: &Path) -> Option<(u64, u64, u64)> {
    None
}

fn query_windows_drive_type_and_unc(drive_root: &str) -> (DriveKind, Option<String>) {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::NetworkManagement::WNet::WNetGetConnectionW;
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;

    let mut normalized_root = drive_root.to_string();
    if !normalized_root.ends_with('\\') {
        normalized_root.push('\\');
    }
    let wide_root: Vec<u16> = std::ffi::OsStr::new(&normalized_root)
        .encode_wide()
        .chain(std::iter::once(0))
        .collect();

    // DRIVE_UNKNOWN=0, DRIVE_NO_ROOT_DIR=1, DRIVE_REMOVABLE=2, DRIVE_FIXED=3, DRIVE_REMOTE=4, DRIVE_CDROM=5, DRIVE_RAMDISK=6
    let raw_type = unsafe { GetDriveTypeW(wide_root.as_ptr()) };
    let kind = match raw_type {
        2 => DriveKind::Removable,
        3 => DriveKind::Fixed,
        4 => DriveKind::Network,
        5 => DriveKind::CdRom,
        6 => DriveKind::RamDisk,
        _ => DriveKind::Unknown,
    };

    let mut unc_path = None;
    if kind == DriveKind::Network {
        // WNetGetConnectionW 需要不帶結尾反斜線的磁碟代號，例如 "Z:"
        let local_name = normalized_root.trim_end_matches('\\');
        let wide_local: Vec<u16> = std::ffi::OsStr::new(local_name)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();
        let mut buffer = vec![0u16; 512];
        let mut buf_len = buffer.len() as u32;
        let status = unsafe {
            WNetGetConnectionW(wide_local.as_ptr(), buffer.as_mut_ptr(), &mut buf_len)
        };
        if status == 0 {
            let end = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            let remote = String::from_utf16_lossy(&buffer[..end]);
            if !remote.is_empty() {
                unc_path = Some(remote);
            }
        }
    }

    (kind, unc_path)
}

#[cfg(not(windows))]
fn query_windows_drive_type_and_unc(_drive_root: &str) -> (DriveKind, Option<String>) {
    (DriveKind::Fixed, None)
}

/// 擷取路徑的磁碟代號（例如 `C:\Users\minus` -> `Some("C:")`）
fn extract_drive_prefix(path_str: &str) -> Option<String> {
    let clean = path_str.strip_prefix(r"\\?\").unwrap_or(path_str);
    let bytes = clean.as_bytes();
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        Some(clean[..2].to_ascii_uppercase())
    } else {
        None
    }
}

/// 檢查任意路徑的磁碟功能與是否為網路磁碟機（含有限功能配置）
pub fn inspect_path_capabilities(path: &Path) -> VolumeCapabilities {
    let raw_str = path.to_string_lossy();
    if is_unc_network_path(&raw_str) {
        return VolumeCapabilities::limited_network(Some(raw_str.into_owned()));
    }

    if let Some(drive_prefix) = extract_drive_prefix(&raw_str) {
        let (kind, unc) = query_windows_drive_type_and_unc(&drive_prefix);
        if kind == DriveKind::Network {
            return VolumeCapabilities::limited_network(unc);
        }
        return VolumeCapabilities::full_local(kind);
    }

    // 若為相對路徑，解析當前工作目錄再判斷
    if let Ok(abs) = std::env::current_dir().map(|cwd| cwd.join(path)) {
        let abs_str = abs.to_string_lossy();
        if is_unc_network_path(&abs_str) {
            return VolumeCapabilities::limited_network(Some(abs_str.into_owned()));
        }
        if let Some(drive_prefix) = extract_drive_prefix(&abs_str) {
            let (kind, unc) = query_windows_drive_type_and_unc(&drive_prefix);
            if kind == DriveKind::Network {
                return VolumeCapabilities::limited_network(unc);
            }
            return VolumeCapabilities::full_local(kind);
        }
    }

    VolumeCapabilities::full_local(DriveKind::Fixed)
}

/// 列出系統所有磁碟機（包含本機硬碟、外接隨身碟、以及已掛載的 Windows 網路磁碟機）
pub fn list_all_drives() -> Vec<DriveInfo> {
    let disks = Disks::new_with_refreshed_list();
    let mut results: Vec<DriveInfo> = Vec::new();
    let mut seen_mounts = std::collections::HashSet::new();

    for disk in disks.list() {
        let mount = disk.mount_point().to_string_lossy().to_string();
        let norm_key = mount.to_ascii_uppercase();
        seen_mounts.insert(norm_key);

        let capabilities = inspect_path_capabilities(Path::new(&mount));
        let total = disk.total_space();
        let avail = disk.available_space();
        let used = total.saturating_sub(avail);
        let usage_percent = if total > 0 {
            ((used as f64 / total as f64) * 100.0) as f32
        } else {
            0.0
        };

        let raw_name = disk.name().to_string_lossy().to_string();
        let display_name = if capabilities.is_network_drive {
            capabilities
                .remote_unc_path
                .clone()
                .unwrap_or_else(|| {
                    if raw_name.is_empty() {
                        "網路磁碟機 (Network Drive)".to_string()
                    } else {
                        raw_name
                    }
                })
        } else if raw_name.is_empty() {
            "本機磁碟 (Local Disk)".to_string()
        } else {
            raw_name
        };

        let fs = disk.file_system().to_string_lossy().to_string();
        results.push(DriveInfo {
            mount_point: mount,
            name: display_name,
            file_system: if fs.is_empty() {
                if capabilities.is_network_drive {
                    "SMB/CIFS".to_string()
                } else {
                    "NTFS".to_string()
                }
            } else {
                fs
            },
            total_bytes: total,
            available_bytes: avail,
            used_bytes: used,
            usage_percent,
            capabilities,
        });
    }

    // 在 Windows 下額外透過 GetLogicalDrives 補充可能未被 sysinfo 列出的網路磁碟機 (Mapped Network Drives)
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetLogicalDrives};

        let bitmask = unsafe { GetLogicalDrives() };
        for i in 0..26u32 {
            if (bitmask & (1 << i)) != 0 {
                let letter = (b'A' + i as u8) as char;
                let mount = format!("{}:\\", letter);
                if seen_mounts.contains(&mount.to_ascii_uppercase()) {
                    continue;
                }
                let capabilities = inspect_path_capabilities(Path::new(&mount));
                let wide_mount: Vec<u16> = std::ffi::OsStr::new(&mount)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect();
                let mut free_available: u64 = 0;
                let mut total_bytes: u64 = 0;
                let mut total_free: u64 = 0;
                let ok = unsafe {
                    GetDiskFreeSpaceExW(
                        wide_mount.as_ptr(),
                        &mut free_available,
                        &mut total_bytes,
                        &mut total_free,
                    )
                };
                let (total, avail) = if ok != 0 {
                    (total_bytes, free_available)
                } else {
                    (0, 0)
                };
                let used = total.saturating_sub(avail);
                let usage_percent = if total > 0 {
                    ((used as f64 / total as f64) * 100.0) as f32
                } else {
                    0.0
                };
                let name = capabilities
                    .remote_unc_path
                    .clone()
                    .unwrap_or_else(|| {
                        if capabilities.is_network_drive {
                            "網路磁碟機 (SMB)".to_string()
                        } else {
                            "本機磁碟".to_string()
                        }
                    });
                results.push(DriveInfo {
                    mount_point: mount,
                    name,
                    file_system: if capabilities.is_network_drive {
                        "SMB/CIFS".to_string()
                    } else {
                        "Unknown".to_string()
                    },
                    total_bytes: total,
                    available_bytes: avail,
                    used_bytes: used,
                    usage_percent,
                    capabilities,
                });
            }
        }
    }

    results.sort_by(|a, b| a.mount_point.cmp(&b.mount_point));
    results
}

/// 將任意路徑規範化為適合顯示的乾淨路徑（移除 Windows `\\?\` 前綴）
pub fn normalize_display_path(path: &Path) -> PathBuf {
    let s = path.to_string_lossy();
    if let Some(stripped) = s.strip_prefix(r"\\?\UNC\") {
        return PathBuf::from(format!(r"\\{}", stripped));
    }
    if let Some(stripped) = s.strip_prefix(r"\\?\") {
        return PathBuf::from(stripped);
    }
    path.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unc_network_path_detection() {
        assert!(is_unc_network_path(r"\\nas-server\media\photos"));
        assert!(is_unc_network_path("//192.168.1.50/share/docs"));
        assert!(is_unc_network_path(r"\\?\UNC\nas-server\share"));
        assert!(!is_unc_network_path(r"C:\Users\minus\Documents"));
        assert!(!is_unc_network_path(r"\\?\C:\Users\minus"));
    }

    #[test]
    fn test_network_drive_limited_capabilities() {
        let caps = inspect_path_capabilities(Path::new(r"\\192.168.1.100\Public\Images"));
        assert!(caps.is_network_drive);
        assert_eq!(caps.drive_kind, DriveKind::Network);
        assert!(!caps.supports_mft_fast_scan);
        assert!(!caps.supports_recycle_bin);
        assert!(!caps.supports_smart_health);
        assert!(!caps.supports_boot_delete);
        assert_eq!(caps.lockhunter_scope, LockHunterScope::LocalProcessOnly);
        assert_eq!(caps.sync_timestamp_tolerance_secs, 2);
        assert!(!caps.limitations_notice.is_empty());
    }

    #[test]
    fn test_local_drive_full_capabilities() {
        let caps = VolumeCapabilities::full_local(DriveKind::Fixed);
        assert!(!caps.is_network_drive);
        assert!(caps.supports_mft_fast_scan);
        assert!(caps.supports_recycle_bin);
        assert!(caps.supports_smart_health);
        assert!(caps.supports_boot_delete);
        assert_eq!(caps.lockhunter_scope, LockHunterScope::FullLocal);
    }
}

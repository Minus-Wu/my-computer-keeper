use crate::drive::{
    inspect_path_capabilities, normalize_display_path, LockHunterScope, VolumeCapabilities,
};
use crate::{KeeperError, KeeperResult};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::Path;
use sysinfo::{ProcessesToUpdate, System};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockingProcessInfo {
    pub pid: u32,
    pub process_name: String,
    pub app_type: String,
    pub exe_path: String,
    pub lock_reason: String,
    pub restartable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockHunterReport {
    pub target_path: String,
    pub capabilities: VolumeCapabilities,
    pub scope: LockHunterScope,
    pub is_locked: bool,
    pub scanned_files_count: usize,
    pub locking_processes: Vec<LockingProcessInfo>,
    pub advisory_notes: Vec<String>,
}

#[cfg(windows)]
fn query_restart_manager_for_files(file_paths: &[String]) -> Vec<LockingProcessInfo> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Foundation::ERROR_MORE_DATA;
    use windows_sys::Win32::System::RestartManager::{
        RmEndSession, RmGetList, RmRegisterResources, RmStartSession, CCH_RM_SESSION_KEY,
        RM_PROCESS_INFO,
    };

    if file_paths.is_empty() {
        return Vec::new();
    }

    let mut session_handle: u32 = 0;
    let mut session_key = [0u16; (CCH_RM_SESSION_KEY as usize) + 1];

    let start_res = unsafe { RmStartSession(&mut session_handle, 0, session_key.as_mut_ptr()) };
    if start_res != 0 {
        return Vec::new();
    }

    let wide_strings: Vec<Vec<u16>> = file_paths
        .iter()
        .map(|s| {
            std::ffi::OsStr::new(s)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect()
        })
        .collect();
    let ptrs: Vec<*const u16> = wide_strings.iter().map(|w| w.as_ptr()).collect();

    let mut found = Vec::new();
    let reg_res = unsafe {
        RmRegisterResources(
            session_handle,
            ptrs.len() as u32,
            ptrs.as_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        )
    };

    if reg_res == 0 {
        let mut needed: u32 = 0;
        let mut count: u32 = 0;
        let mut reboot_reasons: u32 = 0;

        let list_res = unsafe {
            RmGetList(
                session_handle,
                &mut needed,
                &mut count,
                std::ptr::null_mut(),
                &mut reboot_reasons,
            )
        };

        if list_res == ERROR_MORE_DATA && needed > 0 {
            count = needed;
            let mut proc_infos: Vec<RM_PROCESS_INFO> =
                vec![unsafe { std::mem::zeroed() }; count as usize];
            let get_res = unsafe {
                RmGetList(
                    session_handle,
                    &mut needed,
                    &mut count,
                    proc_infos.as_mut_ptr(),
                    &mut reboot_reasons,
                )
            };

            if get_res == 0 {
                for info in proc_infos.iter().take(count as usize) {
                    let pid = info.Process.dwProcessId;
                    let name_end = info
                        .strAppName
                        .iter()
                        .position(|&c| c == 0)
                        .unwrap_or(info.strAppName.len());
                    let app_name = String::from_utf16_lossy(&info.strAppName[..name_end]);

                    let app_type = match info.ApplicationType {
                        1 => "MainWindow App",
                        2 => "OtherWindow App",
                        3 => "Windows Service",
                        4 => "Explorer Shell",
                        5 => "Console Process",
                        1000 => "Critical System Process",
                        _ => "Background Process",
                    }
                    .to_string();

                    found.push(LockingProcessInfo {
                        pid,
                        process_name: if app_name.is_empty() {
                            format!("PID {}", pid)
                        } else {
                            app_name
                        },
                        app_type,
                        exe_path: String::new(),
                        lock_reason: "Windows Restart Manager 偵測到開啟的檔案句柄 (File Handle)"
                            .to_string(),
                        restartable: info.bRestartable != 0,
                    });
                }
            }
        }
    }

    unsafe {
        RmEndSession(session_handle);
    }

    found
}

#[cfg(not(windows))]
fn query_restart_manager_for_files(_file_paths: &[String]) -> Vec<LockingProcessInfo> {
    Vec::new()
}

/// 檢查指定檔案或資料夾被哪些處理程序鎖定（含網路磁碟機有限功能範圍提示）
pub fn inspect_file_locks(path_str: &str) -> KeeperResult<LockHunterReport> {
    let target = Path::new(path_str);
    if !target.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let canonical = std::fs::canonicalize(target).unwrap_or_else(|_| target.to_path_buf());
    let clean_target = normalize_display_path(&canonical);
    let capabilities = inspect_path_capabilities(&clean_target);
    let scope = capabilities.lockhunter_scope;

    let mut files_to_check = Vec::new();
    if clean_target.is_file() {
        files_to_check.push(clean_target.to_string_lossy().to_string());
    } else {
        let max_files = if capabilities.is_network_drive { 40 } else { 150 };
        for entry in WalkDir::new(&clean_target)
            .max_depth(3)
            .into_iter()
            .flatten()
        {
            if entry.file_type().is_file() {
                files_to_check.push(
                    normalize_display_path(entry.path())
                        .to_string_lossy()
                        .to_string(),
                );
                if files_to_check.len() >= max_files {
                    break;
                }
            }
        }
    }

    let scanned_files_count = files_to_check.len().max(1);
    let mut locking_processes = query_restart_manager_for_files(&files_to_check);
    let mut seen_pids: HashSet<u32> = locking_processes.iter().map(|p| p.pid).collect();

    // 結合 sysinfo 補齊執行檔路徑，並檢查是否有行程的執行檔或工作目錄 (CWD) 直接位於目標路徑下
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let target_upper = clean_target.to_string_lossy().to_ascii_uppercase();

    for p in locking_processes.iter_mut() {
        if let Some(proc_info) = sys.process(sysinfo::Pid::from_u32(p.pid)) {
            if let Some(exe) = proc_info.exe() {
                p.exe_path = exe.to_string_lossy().to_string();
            }
        }
    }

    for (pid_val, proc_info) in sys.processes() {
        let pid = pid_val.as_u32();
        if seen_pids.contains(&pid) {
            continue;
        }
        let exe_str = proc_info
            .exe()
            .map(|p| normalize_display_path(p).to_string_lossy().to_string())
            .unwrap_or_default();
        let cwd_str = proc_info
            .cwd()
            .map(|p| normalize_display_path(p).to_string_lossy().to_string())
            .unwrap_or_default();

        let exe_match =
            !exe_str.is_empty() && exe_str.to_ascii_uppercase().starts_with(&target_upper);
        let cwd_match =
            !cwd_str.is_empty() && cwd_str.to_ascii_uppercase().starts_with(&target_upper);

        if exe_match || cwd_match {
            seen_pids.insert(pid);
            locking_processes.push(LockingProcessInfo {
                pid,
                process_name: proc_info.name().to_string_lossy().to_string(),
                app_type: "Active Process".to_string(),
                exe_path: exe_str,
                lock_reason: if exe_match {
                    "執行檔映像 (Executable Image) 位於目標路徑中".to_string()
                } else {
                    "當前工作目錄 (Working Directory) 鎖定於目標資料夾".to_string()
                },
                restartable: true,
            });
        }
    }

    let mut advisory_notes = Vec::new();
    if capabilities.is_network_drive {
        advisory_notes.push(
            "🌐 網路磁碟機有限功能模式：目前僅能偵測「本機電腦」對此遠端路徑開啟的檔案句柄，無法查詢遠端 NAS 或其他連線電腦的鎖定狀態。".to_string(),
        );
        advisory_notes.push(
            "🌐 網路磁碟機不支援 Windows「下次重開機時刪除 (MoveFileExW Delayed Delete)」功能。".to_string(),
        );
    } else {
        advisory_notes.push(
            "本機磁碟完整模式：已透過 Windows Restart Manager 與行程工作目錄分析鎖定狀態。".to_string(),
        );
    }

    Ok(LockHunterReport {
        target_path: clean_target.to_string_lossy().to_string(),
        capabilities,
        scope,
        is_locked: !locking_processes.is_empty(),
        scanned_files_count,
        locking_processes,
        advisory_notes,
    })
}

/// 將頑固鎖定檔案排程於下次 Windows 重開機時由系統核心刪除（網路磁碟機不支援）
pub fn schedule_delete_on_reboot(path_str: &str) -> KeeperResult<String> {
    let path = Path::new(path_str);
    if !path.exists() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }

    let caps = inspect_path_capabilities(path);
    if caps.is_network_drive || !caps.supports_boot_delete {
        return Err(KeeperError::NetworkDriveRestriction(
            "網路磁碟機不支援排程於下次重開機時刪除（Windows 開機階段尚未掛載網路磁碟機）。"
                .to_string(),
        ));
    }

    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Storage::FileSystem::{
            MoveFileExW, MOVEFILE_DELAY_UNTIL_REBOOT,
        };

        let wide: Vec<u16> = std::ffi::OsStr::new(path_str)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let ok = unsafe {
            MoveFileExW(
                wide.as_ptr(),
                std::ptr::null(),
                MOVEFILE_DELAY_UNTIL_REBOOT,
            )
        };
        if ok != 0 {
            Ok(format!(
                "已將「{}」排程於下次 Windows 重新啟動時自動刪除。",
                path_str
            ))
        } else {
            Err(KeeperError::SystemError(
                "排程重開機刪除失敗，請以系統管理員身分執行本程式。".to_string(),
            ))
        }
    }

    #[cfg(not(windows))]
    {
        Err(KeeperError::SystemError(
            "重開機刪除功能僅支援 Windows 系統。".to_string(),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_inspect_unlocked_temp_file() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("unlocked.txt");
        std::fs::write(&file_path, "test lockhunter").unwrap();

        let report = inspect_file_locks(&file_path.to_string_lossy()).unwrap();
        assert_eq!(report.scope, LockHunterScope::FullLocal);
        assert!(report.scanned_files_count >= 1);
    }
}

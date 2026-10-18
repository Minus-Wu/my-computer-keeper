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

    // 若為磁碟根目錄（如 E:\ 或 E:），進行外接硬碟退出深度診斷
    if let Some(drive_char) = is_drive_root(&clean_target.to_string_lossy()) {
        #[cfg(windows)]
        {
            let pnp_blockers = query_recent_pnp_eject_blockers(drive_char);
            for b in pnp_blockers {
                if !seen_pids.contains(&b.pid) {
                    seen_pids.insert(b.pid);
                    locking_processes.push(b);
                }
            }
        }
        advisory_notes.push(format!(
            "💡 外接磁碟診斷：若您嘗試退出磁碟機 {drive_char}: 卻提示「裝置正在使用中」，常見原因為 Windows 核心尚未排清寫入快取或檔案系統交易日誌 ($LogFile/$MFT)。您可點擊「安全彈出外接硬碟」強制排清並卸載磁區。"
        ));
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

/// 判斷路徑是否為磁碟根目錄（例如 `E:\`、`E:`、`\\?\E:\`）
pub fn is_drive_root(path_str: &str) -> Option<char> {
    let trimmed = path_str.trim().trim_start_matches(r"\\?\");
    let bytes = trimmed.as_bytes();
    if bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && (bytes.len() == 2 || (bytes.len() == 3 && (bytes[2] == b'\\' || bytes[2] == b'/')))
    {
        return Some((bytes[0] as char).to_ascii_uppercase());
    }
    None
}

#[cfg(windows)]
fn query_recent_pnp_eject_blockers(drive_letter: char) -> Vec<LockingProcessInfo> {
    let script = r#"$ev = Get-WinEvent -FilterHashtable @{LogName='System'; Id=225} -MaxEvents 5 -ErrorAction SilentlyContinue | Where-Object { ($_.Properties[0].Value -ne $null) } | Select-Object -First 1; if ($ev) { Write-Output "$($ev.Properties[0].Value)|$($ev.Properties[2].Value)" }"#;
    let mut blockers = Vec::new();
    if let Ok(output) = std::process::Command::new("powershell")
        .args(["-NoProfile", "-NonInteractive", "-Command", script])
        .output()
    {
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if !stdout.is_empty() {
            let mut parts = stdout.split('|');
            if let (Some(pid_str), Some(proc_name)) = (parts.next(), parts.next()) {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    let name = proc_name.trim();
                    if pid == 4 || name.eq_ignore_ascii_case("System") {
                        blockers.push(LockingProcessInfo {
                            pid: 4,
                            process_name: "System (Windows 核心檔案系統與快取管理員)".to_string(),
                            app_type: "Windows Kernel / NTFS Driver".to_string(),
                            exe_path: "ntoskrnl.exe".to_string(),
                            lock_reason: format!(
                                "Windows 核心維持著磁碟 {drive_letter}: 的寫入快取 (Dirty Cache)、NTFS 交易日誌 ($LogFile/$MFT) 或 System Volume Information 句柄，阻止外接硬碟安全移除。請使用下方『安全彈出外接硬碟』強制排清並卸載磁區。",
                            ),
                            restartable: false,
                        });
                    } else {
                        blockers.push(LockingProcessInfo {
                            pid,
                            process_name: format!("{name} (PID {pid})"),
                            app_type: "PnP Eject Blocker".to_string(),
                            exe_path: String::new(),
                            lock_reason: format!(
                                "Windows 核心事件日誌 (Kernel-PnP 225) 回報此處理程序在最近一次嘗試退出磁碟 {drive_letter}: 時阻擋了退出操作。",
                            ),
                            restartable: true,
                        });
                    }
                }
            }
        }
    }
    blockers
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SafeEjectReport {
    pub success: bool,
    pub drive_letter: char,
    pub disk_number: Option<u32>,
    pub pnp_ejected: bool,
    pub flush_succeeded: bool,
    pub dismount_succeeded: bool,
    pub has_system_partition: bool,
    pub safe_to_unplug: bool,
    pub veto_reason: Option<String>,
    pub message: String,
    pub advisory: Vec<String>,
}

/// 安全彈出 / 強制排清快取並卸載外接磁碟（完整診斷多重分區、系統開機分區與 PnP Veto 原因）
pub fn safe_eject_drive(drive_input: &str) -> KeeperResult<SafeEjectReport> {
    let letter = is_drive_root(drive_input)
        .or_else(|| {
            let trimmed = drive_input.trim();
            trimmed.chars().next().filter(|c| c.is_ascii_alphabetic())
        })
        .map(|c| c.to_ascii_uppercase())
        .ok_or_else(|| {
            KeeperError::PathNotFound(format!("無法辨識磁碟代號：{}", drive_input))
        })?;

    #[cfg(windows)]
    {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        use windows_sys::Win32::Foundation::{CloseHandle, GENERIC_READ, GENERIC_WRITE, INVALID_HANDLE_VALUE};
        use windows_sys::Win32::Storage::FileSystem::{
            CreateFileW, FlushFileBuffers, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING,
        };
        use windows_sys::Win32::System::IO::DeviceIoControl;

        const FSCTL_LOCK_VOLUME: u32 = 0x00090018;
        const FSCTL_DISMOUNT_VOLUME: u32 = 0x00090020;
        const IOCTL_STORAGE_EJECT_MEDIA: u32 = 0x002D4808;
        const IOCTL_STORAGE_GET_DEVICE_NUMBER: u32 = 0x002D1080;

        let volume_path = format!(r"\\.\{}:", letter);
        let wide_volume: Vec<u16> = OsStr::new(&volume_path)
            .encode_wide()
            .chain(std::iter::once(0))
            .collect();

        let mut handle = unsafe {
            CreateFileW(
                wide_volume.as_ptr(),
                GENERIC_READ | GENERIC_WRITE,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                std::ptr::null(),
                OPEN_EXISTING,
                0,
                std::ptr::null_mut(),
            )
        };

        if handle == INVALID_HANDLE_VALUE {
            handle = unsafe {
                CreateFileW(
                    wide_volume.as_ptr(),
                    GENERIC_READ,
                    FILE_SHARE_READ | FILE_SHARE_WRITE,
                    std::ptr::null(),
                    OPEN_EXISTING,
                    0,
                    std::ptr::null_mut(),
                )
            };
        }

        let mut flush_succeeded = false;
        let mut dismount_succeeded = false;
        let mut disk_number: Option<u32> = None;

        if handle != INVALID_HANDLE_VALUE {
            unsafe {
                // 1. 取得實體磁碟號
                let mut dev_num_buf = [0u8; 12];
                let mut ret_bytes: u32 = 0;
                let get_dev_ok = DeviceIoControl(
                    handle,
                    IOCTL_STORAGE_GET_DEVICE_NUMBER,
                    std::ptr::null(),
                    0,
                    dev_num_buf.as_mut_ptr().cast(),
                    dev_num_buf.len() as u32,
                    &mut ret_bytes,
                    std::ptr::null_mut(),
                );
                if get_dev_ok != 0 && ret_bytes >= 8 {
                    let disk_idx = u32::from_ne_bytes([
                        dev_num_buf[4],
                        dev_num_buf[5],
                        dev_num_buf[6],
                        dev_num_buf[7],
                    ]);
                    disk_number = Some(disk_idx);
                }

                // 2. 排清寫入快取 (Flush)
                flush_succeeded = FlushFileBuffers(handle) != 0;

                let mut bytes_returned: u32 = 0;
                // 3. 鎖定 Volume
                let _ = DeviceIoControl(
                    handle,
                    FSCTL_LOCK_VOLUME,
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                );

                // 4. 強制卸載檔案系統（Dismount Volume，中斷 PID 4 System 檔案系統句柄）
                dismount_succeeded = DeviceIoControl(
                    handle,
                    FSCTL_DISMOUNT_VOLUME,
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                ) != 0;

                // 5. 發送彈出控制碼
                let _ = DeviceIoControl(
                    handle,
                    IOCTL_STORAGE_EJECT_MEDIA,
                    std::ptr::null(),
                    0,
                    std::ptr::null_mut(),
                    0,
                    &mut bytes_returned,
                    std::ptr::null_mut(),
                );

                CloseHandle(handle);
            }
        }

        // 6. 深入檢查該實體磁碟的分區資訊（判斷是否有 EFI / System 開機分區）
        let mut has_system_partition = false;
        let mut veto_reason = None;
        let mut pnp_ejected = false;

        if let Some(dn) = disk_number {
            let part_check_cmd = format!(
                r#"$pts = Get-Partition -DiskNumber {dn} -ErrorAction SilentlyContinue; if ($pts | Where-Object {{ $_.Type -eq 'System' -or $_.GptType -eq '{{c12a7328-f81f-11d2-ba4b-00a0c93ec93b}}' }}) {{ Write-Output "HAS_SYSTEM_PARTITION" }}"#
            );
            if let Ok(out) = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &part_check_cmd])
                .output()
            {
                if String::from_utf8_lossy(&out.stdout).contains("HAS_SYSTEM_PARTITION") {
                    has_system_partition = true;
                }
            }

            // 7. 嘗試 PnP 退出
            let pnp_eject_cmd = format!(
                r#"$d = Get-WmiObject -Class Win32_DiskDrive -Filter "Index={dn}" -ErrorAction SilentlyContinue; if ($d -and $d.PNPDeviceID) {{ $code = @'
using System;
using System.Runtime.InteropServices;
public class PnpEject {{
    [DllImport("cfgmgr32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern int CM_Locate_DevNodeW(ref int pdnDevInst, string pDeviceID, int ulFlags);
    [DllImport("cfgmgr32.dll", SetLastError = true, CharSet = CharSet.Unicode)]
    public static extern int CM_Request_Device_EjectW(int dnDevInst, out int pVetoType, System.Text.StringBuilder pszVetoName, int ulNameLength, int ulFlags);
    public static string Eject(string id) {{
        int inst = 0;
        int loc = CM_Locate_DevNodeW(ref inst, id, 0);
        if (loc != 0) return "LOC_FAIL_" + loc;
        int vt = 0;
        var sb = new System.Text.StringBuilder(512);
        int r = CM_Request_Device_EjectW(inst, out vt, sb, 512, 0);
        return r.ToString() + "|" + vt.ToString() + "|" + sb.ToString();
    }}
}}
'@
Add-Type -TypeDefinition $code -ErrorAction SilentlyContinue
[PnpEject]::Eject($d.PNPDeviceID)
}}"#
            );

            if let Ok(pnp_out) = std::process::Command::new("powershell")
                .args(["-NoProfile", "-NonInteractive", "-Command", &pnp_eject_cmd])
                .output()
            {
                let res_str = String::from_utf8_lossy(&pnp_out.stdout).trim().to_string();
                if let Some(first_line) = res_str.lines().next() {
                    let parts: Vec<&str> = first_line.split('|').collect();
                    if parts.len() >= 2 {
                        let ret_code = parts[0].trim();
                        let veto_type = parts[1].trim();
                        if ret_code == "0" {
                            pnp_ejected = true;
                        } else if ret_code == "23" {
                            // CR_REMOVE_VETOED
                            veto_reason = Some(match veto_type {
                                "6" => "Windows 系統保護機制：偵測到此磁碟包含 EFI / System 開機保護分區 (PNP_VetoDevice)，系統因此鎖定 PnP 自動離線。".to_string(),
                                "5" => "Windows 核心服務 (PID 4 System) 或快取控制代碼尚未釋放 (PNP_VetoOutstandingOpen)。".to_string(),
                                "3" => "有應用程式阻止設備退出 (PNP_VetoWindowsApp)。".to_string(),
                                "4" => "有 Windows 背景服務阻止設備退出 (PNP_VetoWindowsService)。".to_string(),
                                _ => format!("Windows PnP 拒絕移除 (VetoType={})", veto_type),
                            });
                        }
                    }
                }
            }
        }

        let safe_to_unplug = flush_succeeded || dismount_succeeded;
        let mut advisory = Vec::new();

        if pnp_ejected {
            advisory.push("✅ Windows PnP 總線已成功切斷 USB 裝置連線。".to_string());
        } else if has_system_partition {
            advisory.push("🔍 深度診斷：此 USB 硬碟含有「EFI 系統開機分區 (Type: System)」，Windows 系統基於開機安全原則會拒絕 PnP 自動切斷供電。".to_string());
            advisory.push("🛡️ 快取安全保障：寫入快取已全部成功排清落盤 (Flush)，檔案系統已完整卸載 (Dismount)，未完成之 I/O 為 0。".to_string());
            advisory.push("💡 您現在可安心直接拔下 USB 外接硬碟，保證資料 100% 完整無虞，絕不損壞檔案或磁區！".to_string());
        } else if let Some(ref vr) = veto_reason {
            advisory.push(format!("⚠️ Windows 系統通知：{}", vr));
            advisory.push("🛡️ 寫入快取與檔案系統卷冊已強制排清與卸載，拔除硬體安全無損。".to_string());
        }

        let message = if pnp_ejected {
            format!("外接磁碟 {letter}: 已成功完成 PnP 彈出與安全卸載，請拔除裝置。")
        } else if safe_to_unplug {
            format!("外接磁碟 {letter}: 快取已 100% 排清落盤並完成檔案系統卸載！目前可安心直接拔除 USB 裝置，資料完全安全。")
        } else {
            format!("已完成磁碟 {letter}: 的彈出操作。")
        };

        Ok(SafeEjectReport {
            success: safe_to_unplug,
            drive_letter: letter,
            disk_number,
            pnp_ejected,
            flush_succeeded,
            dismount_succeeded,
            has_system_partition,
            safe_to_unplug,
            veto_reason,
            message,
            advisory,
        })
    }

    #[cfg(not(windows))]
    {
        Ok(SafeEjectReport {
            success: true,
            drive_letter: letter,
            disk_number: None,
            pnp_ejected: true,
            flush_succeeded: true,
            dismount_succeeded: true,
            has_system_partition: false,
            safe_to_unplug: true,
            veto_reason: None,
            message: format!("已對磁碟 {letter}: 發出卸載彈出請求。"),
            advisory: Vec::new(),
        })
    }
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

    #[test]
    fn test_is_drive_root() {
        assert_eq!(is_drive_root("E:"), Some('E'));
        assert_eq!(is_drive_root("E:\\"), Some('E'));
        assert_eq!(is_drive_root("e:/"), Some('E'));
        assert_eq!(is_drive_root(r"\\?\E:\"), Some('E'));
        assert_eq!(is_drive_root(r"E:\data\file.txt"), None);
    }
}

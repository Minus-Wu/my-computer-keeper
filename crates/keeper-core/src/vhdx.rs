use crate::drive::inspect_path_capabilities;
use crate::{KeeperError, KeeperResult};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::{Mutex, MutexGuard};

/// 主程式收到此參數時，以提權子行程身分執行壓縮並結束
pub const COMPACT_WORKER_ARG: &str = "--compact-vhdx";

const MSG_IN_USE: &str = "虛擬磁碟正在使用中（WSL / Docker Desktop 可能仍在執行），請先結束 Docker Desktop 並執行 wsl --shutdown 後再試。";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VhdxCompactState {
    Idle,
    Running,
    Succeeded,
    Failed,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VhdxCompactStatus {
    pub state: VhdxCompactState,
    pub path: String,
    pub size_before_bytes: u64,
    pub size_after_bytes: Option<u64>,
    pub message: String,
}

static JOB: Mutex<Option<VhdxCompactStatus>> = Mutex::new(None);

fn lock_job() -> MutexGuard<'static, Option<VhdxCompactStatus>> {
    JOB.lock().unwrap_or_else(|e| e.into_inner())
}

fn is_virtual_disk_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("vhdx") || e.eq_ignore_ascii_case("vhd"))
        .unwrap_or(false)
}

/// 取得目前（或最近一次）虛擬磁碟壓縮作業狀態
pub fn compact_vhdx_status() -> VhdxCompactStatus {
    lock_job().clone().unwrap_or(VhdxCompactStatus {
        state: VhdxCompactState::Idle,
        path: String::new(),
        size_before_bytes: 0,
        size_after_bytes: None,
        message: String::new(),
    })
}

/// 於背景啟動虛擬磁碟壓縮（會跳出 UAC 系統管理員授權提示），立即回傳作業狀態供前端輪詢
pub fn start_compact_vhdx(path_str: &str) -> KeeperResult<VhdxCompactStatus> {
    let path = Path::new(path_str);
    if !path.is_file() {
        return Err(KeeperError::PathNotFound(path_str.to_string()));
    }
    // Windows 路徑不可能含有引號；排除以免破壞提權子行程的命令列參數
    if !is_virtual_disk_path(path) || !path.is_absolute() || path_str.contains('"') {
        return Err(KeeperError::SystemError(
            "僅支援壓縮本機的 .vhdx / .vhd 虛擬磁碟檔案。".to_string(),
        ));
    }
    if inspect_path_capabilities(path).is_network_drive {
        return Err(KeeperError::NetworkDriveRestriction(
            "網路磁碟機上的虛擬磁碟無法壓縮，請在檔案所在的電腦上執行。".to_string(),
        ));
    }
    ensure_not_in_use(path)?;

    let size_before = std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| KeeperError::Io(e.to_string()))?;

    let status = {
        let mut job = lock_job();
        if matches!(job.as_ref(), Some(j) if j.state == VhdxCompactState::Running) {
            return Err(KeeperError::SystemError(
                "已有虛擬磁碟壓縮作業正在進行中，請等待完成。".to_string(),
            ));
        }
        let status = VhdxCompactStatus {
            state: VhdxCompactState::Running,
            path: path_str.to_string(),
            size_before_bytes: size_before,
            size_after_bytes: None,
            message: "正在等待系統管理員授權並壓縮虛擬磁碟...".to_string(),
        };
        *job = Some(status.clone());
        status
    };

    let owned = path_str.to_string();
    std::thread::spawn(move || {
        let result = run_elevated_worker(&owned);
        let size_after = std::fs::metadata(&owned).map(|m| m.len()).ok();
        if let Some(job) = lock_job().as_mut() {
            job.size_after_bytes = size_after;
            match result {
                Ok(()) => {
                    job.state = VhdxCompactState::Succeeded;
                    job.message = "虛擬磁碟壓縮完成。".to_string();
                }
                Err(msg) => {
                    job.state = VhdxCompactState::Failed;
                    job.message = msg;
                }
            }
        }
    });

    Ok(status)
}

#[cfg(windows)]
fn ensure_not_in_use(path: &Path) -> KeeperResult<()> {
    use std::os::windows::fs::OpenOptionsExt;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    match std::fs::OpenOptions::new().read(true).share_mode(0).open(path) {
        Err(e) if e.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => {
            Err(KeeperError::SystemError(MSG_IN_USE.to_string()))
        }
        _ => Ok(()),
    }
}

#[cfg(not(windows))]
fn ensure_not_in_use(_path: &Path) -> KeeperResult<()> {
    Ok(())
}

#[cfg(windows)]
fn to_wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// 以 ShellExecuteEx "runas" 自我提權啟動本程式的壓縮子行程並等待結束
#[cfg(windows)]
fn run_elevated_worker(vhdx: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::System::Threading::{
        GetExitCodeProcess, WaitForSingleObject, INFINITE,
    };
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let result_file =
        std::env::temp_dir().join(format!("keeper-vhdx-{}-{}.txt", std::process::id(), nonce));
    let params = format!(
        "{COMPACT_WORKER_ARG} \"{}\" \"{}\"",
        vhdx,
        result_file.display()
    );

    let verb = to_wide(OsStr::new("runas"));
    let file = to_wide(exe.as_os_str());
    let params_w = to_wide(OsStr::new(&params));

    let mut info: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
    info.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
    info.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
    info.lpVerb = verb.as_ptr();
    info.lpFile = file.as_ptr();
    info.lpParameters = params_w.as_ptr();
    info.nShow = SW_HIDE;

    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        let code = unsafe { GetLastError() };
        if code == ERROR_CANCELLED {
            return Err("已取消系統管理員權限授權，未執行壓縮。".to_string());
        }
        return Err(format!(
            "無法以系統管理員身分啟動壓縮程序：{}",
            std::io::Error::from_raw_os_error(code as i32)
        ));
    }
    if info.hProcess.is_null() {
        return Err(format!("虛擬磁碟壓縮失敗（結束代碼 {}）。", -1));
    }

    let mut exit_code: u32 = 1;
    unsafe {
        WaitForSingleObject(info.hProcess, INFINITE);
        GetExitCodeProcess(info.hProcess, &mut exit_code);
        CloseHandle(info.hProcess);
    }

    let detail = std::fs::read_to_string(&result_file).unwrap_or_default();
    let _ = std::fs::remove_file(&result_file);

    if exit_code == 0 {
        Ok(())
    } else if detail.trim().is_empty() {
        Err(format!("虛擬磁碟壓縮失敗（結束代碼 {}）。", exit_code))
    } else {
        Err(detail.trim().to_string())
    }
}

#[cfg(not(windows))]
fn run_elevated_worker(_vhdx: &str) -> Result<(), String> {
    Err("虛擬磁碟壓縮僅支援 Windows 系統。".to_string())
}

/// 提權子行程進入點：執行壓縮，將結果訊息寫入 result_file，回傳行程結束代碼
pub fn run_compact_worker(vhdx: &str, result_file: &str) -> i32 {
    let outcome = if is_virtual_disk_path(Path::new(vhdx)) {
        compact_virtual_disk(vhdx)
    } else {
        Err("僅支援壓縮本機的 .vhdx / .vhd 虛擬磁碟檔案。".to_string())
    };
    let (code, msg) = match outcome {
        Ok(()) => (0, String::new()),
        Err(msg) => (1, msg),
    };
    if !result_file.is_empty() {
        let _ = std::fs::write(result_file, msg);
    }
    code
}

/// 透過 VirtDisk API：唯讀掛載（不指派磁碟代號）後壓縮，再卸載
#[cfg(windows)]
fn compact_virtual_disk(vhdx: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use windows_sys::Win32::Foundation::{CloseHandle, ERROR_SHARING_VIOLATION, ERROR_SUCCESS, HANDLE};
    use windows_sys::Win32::Storage::Vhd::{
        AttachVirtualDisk, CompactVirtualDisk, DetachVirtualDisk, OpenVirtualDisk,
        ATTACH_VIRTUAL_DISK_FLAG_NO_DRIVE_LETTER, ATTACH_VIRTUAL_DISK_FLAG_READ_ONLY,
        ATTACH_VIRTUAL_DISK_PARAMETERS, ATTACH_VIRTUAL_DISK_VERSION_1,
        COMPACT_VIRTUAL_DISK_FLAG_NONE, COMPACT_VIRTUAL_DISK_PARAMETERS,
        COMPACT_VIRTUAL_DISK_VERSION_1, DETACH_VIRTUAL_DISK_FLAG_NONE,
        OPEN_VIRTUAL_DISK_FLAG_NONE, OPEN_VIRTUAL_DISK_PARAMETERS, OPEN_VIRTUAL_DISK_VERSION_2,
        VIRTUAL_DISK_ACCESS_NONE, VIRTUAL_STORAGE_TYPE, VIRTUAL_STORAGE_TYPE_DEVICE_UNKNOWN,
        VIRTUAL_STORAGE_TYPE_VENDOR_UNKNOWN,
    };

    let wide_path = to_wide(OsStr::new(vhdx));
    let storage_type = VIRTUAL_STORAGE_TYPE {
        DeviceId: VIRTUAL_STORAGE_TYPE_DEVICE_UNKNOWN,
        VendorId: VIRTUAL_STORAGE_TYPE_VENDOR_UNKNOWN,
    };

    unsafe {
        let mut open_params: OPEN_VIRTUAL_DISK_PARAMETERS = std::mem::zeroed();
        open_params.Version = OPEN_VIRTUAL_DISK_VERSION_2;
        let mut handle: HANDLE = std::ptr::null_mut();
        let rc = OpenVirtualDisk(
            &storage_type,
            wide_path.as_ptr(),
            VIRTUAL_DISK_ACCESS_NONE,
            OPEN_VIRTUAL_DISK_FLAG_NONE,
            &open_params,
            &mut handle,
        );
        if rc == ERROR_SHARING_VIOLATION {
            return Err(MSG_IN_USE.to_string());
        }
        if rc != ERROR_SUCCESS {
            return Err(format!(
                "無法開啟虛擬磁碟：{}",
                std::io::Error::from_raw_os_error(rc as i32)
            ));
        }

        // 唯讀掛載可讓 NTFS 等可辨識的檔案系統進行感知式壓縮；掛載失敗時仍可直接壓縮
        let mut attach_params: ATTACH_VIRTUAL_DISK_PARAMETERS = std::mem::zeroed();
        attach_params.Version = ATTACH_VIRTUAL_DISK_VERSION_1;
        let attached = AttachVirtualDisk(
            handle,
            std::ptr::null_mut(),
            ATTACH_VIRTUAL_DISK_FLAG_READ_ONLY | ATTACH_VIRTUAL_DISK_FLAG_NO_DRIVE_LETTER,
            0,
            &attach_params,
            std::ptr::null(),
        ) == ERROR_SUCCESS;

        let mut compact_params: COMPACT_VIRTUAL_DISK_PARAMETERS = std::mem::zeroed();
        compact_params.Version = COMPACT_VIRTUAL_DISK_VERSION_1;
        let rc = CompactVirtualDisk(
            handle,
            COMPACT_VIRTUAL_DISK_FLAG_NONE,
            &compact_params,
            std::ptr::null(),
        );

        if attached {
            DetachVirtualDisk(handle, DETACH_VIRTUAL_DISK_FLAG_NONE, 0);
        }
        CloseHandle(handle);

        if rc == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(format!(
                "虛擬磁碟壓縮失敗：{}",
                std::io::Error::from_raw_os_error(rc as i32)
            ))
        }
    }
}

#[cfg(not(windows))]
fn compact_virtual_disk(_vhdx: &str) -> Result<(), String> {
    Err("虛擬磁碟壓縮僅支援 Windows 系統。".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_non_virtual_disk_files() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("data.bin");
        std::fs::write(&file, b"x").unwrap();
        assert!(start_compact_vhdx(&file.to_string_lossy()).is_err());
        assert_eq!(compact_vhdx_status().state, VhdxCompactState::Idle);
    }
}

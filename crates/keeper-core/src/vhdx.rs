use crate::drive::inspect_path_capabilities;
use crate::{KeeperError, KeeperResult};
use std::path::Path;

pub use crate::vhdx_workflow::{
    compact_vhdx_status, inspect_compact_environment, retry_vhdx_restore, start_compact_vhdx,
    VhdxCompactState, VhdxCompactStatus,
};

/// 主程式收到此參數時，以提權子行程身分執行壓縮並結束
pub const COMPACT_WORKER_ARG: &str = "--compact-vhdx";

const MSG_IN_USE: &str = "虛擬磁碟正在使用中（WSL / Docker Desktop 可能仍在執行），請先結束 Docker Desktop 並執行 wsl --shutdown 後再試。";

fn is_virtual_disk_path(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("vhdx") || e.eq_ignore_ascii_case("vhd"))
        .unwrap_or(false)
}

pub(crate) fn validate_compact_path(path_str: &str) -> KeeperResult<u64> {
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
    std::fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| KeeperError::Io(e.to_string()))
}

#[cfg(windows)]
pub(crate) fn ensure_not_in_use(path: &Path) -> KeeperResult<()> {
    use std::os::windows::fs::OpenOptionsExt;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    match std::fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(path)
    {
        Err(e) if e.raw_os_error() == Some(ERROR_SHARING_VIOLATION) => {
            Err(KeeperError::SystemError(MSG_IN_USE.to_string()))
        }
        Err(e) => Err(KeeperError::Io(e.to_string())),
        Ok(_) => Ok(()),
    }
}

#[cfg(not(windows))]
pub(crate) fn ensure_not_in_use(_path: &Path) -> KeeperResult<()> {
    Ok(())
}

#[cfg(windows)]
fn to_wide(s: &std::ffi::OsStr) -> Vec<u16> {
    use std::os::windows::ffi::OsStrExt;
    s.encode_wide().chain(std::iter::once(0)).collect()
}

/// 以 ShellExecuteEx "runas" 自我提權啟動本程式的壓縮子行程並等待結束
#[cfg(windows)]
pub(crate) fn run_elevated_worker(vhdx: &str, progress: impl Fn(u8)) -> Result<(), String> {
    use std::ffi::OsStr;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, ERROR_CANCELLED};
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{
        ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let scratch = tempfile::Builder::new()
        .prefix("keeper-vhdx-")
        .tempdir()
        .map_err(|e| e.to_string())?;
    let result_file = scratch.path().join("result.txt");
    let progress_file = scratch.path().join("progress.txt");
    let params = format!(
        "{COMPACT_WORKER_ARG} \"{}\" \"{}\" \"{}\"",
        vhdx,
        result_file.display(),
        progress_file.display()
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
        while WaitForSingleObject(info.hProcess, 500)
            == windows_sys::Win32::Foundation::WAIT_TIMEOUT
        {
            if let Ok(text) = std::fs::read_to_string(&progress_file) {
                if let Ok(percent) = text.trim().parse::<u8>() {
                    progress(percent.min(100));
                }
            }
        }
        GetExitCodeProcess(info.hProcess, &mut exit_code);
        CloseHandle(info.hProcess);
    }

    let detail = std::fs::read_to_string(&result_file).unwrap_or_default();
    let _ = std::fs::remove_file(&result_file);
    let _ = std::fs::remove_file(&progress_file);

    if exit_code == 0 {
        Ok(())
    } else if detail.trim().is_empty() {
        Err(format!("虛擬磁碟壓縮失敗（結束代碼 {}）。", exit_code))
    } else {
        Err(detail.trim().to_string())
    }
}

#[cfg(not(windows))]
pub(crate) fn run_elevated_worker(_vhdx: &str, _progress: impl Fn(u8)) -> Result<(), String> {
    Err("虛擬磁碟壓縮僅支援 Windows 系統。".to_string())
}

/// 提權子行程進入點：執行壓縮，將結果訊息寫入 result_file，回傳行程結束代碼
pub fn run_compact_worker(vhdx: &str, result_file: &str, progress_file: &str) -> i32 {
    let outcome = validate_compact_path(vhdx)
        .and_then(|_| ensure_not_in_use(Path::new(vhdx)))
        .map_err(|e| e.to_string())
        .and_then(|_| compact_virtual_disk(vhdx, progress_file));
    let (code, msg) = match outcome {
        Ok(()) => (0, String::new()),
        Err(msg) => (1, msg),
    };
    if !result_file.is_empty() {
        if let Err(error) = std::fs::write(result_file, msg) {
            eprintln!("Cannot write VHDX result: {error}");
            return 1;
        }
    }
    code
}

/// 透過 VirtDisk API：唯讀掛載（不指派磁碟代號）後壓縮，再卸載
#[cfg(windows)]
fn compact_virtual_disk(vhdx: &str, progress_file: &str) -> Result<(), String> {
    use std::ffi::OsStr;
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_IO_PENDING, ERROR_SHARING_VIOLATION, ERROR_SUCCESS, HANDLE,
    };
    use windows_sys::Win32::Storage::Vhd::{
        AttachVirtualDisk, CompactVirtualDisk, DetachVirtualDisk, GetVirtualDiskOperationProgress,
        OpenVirtualDisk, ATTACH_VIRTUAL_DISK_FLAG_NO_DRIVE_LETTER,
        ATTACH_VIRTUAL_DISK_FLAG_READ_ONLY, ATTACH_VIRTUAL_DISK_PARAMETERS,
        ATTACH_VIRTUAL_DISK_VERSION_1, COMPACT_VIRTUAL_DISK_FLAG_NONE,
        COMPACT_VIRTUAL_DISK_PARAMETERS, COMPACT_VIRTUAL_DISK_VERSION_1,
        DETACH_VIRTUAL_DISK_FLAG_NONE, OPEN_VIRTUAL_DISK_FLAG_NONE, OPEN_VIRTUAL_DISK_PARAMETERS,
        OPEN_VIRTUAL_DISK_VERSION_2, VIRTUAL_DISK_ACCESS_NONE, VIRTUAL_DISK_PROGRESS,
        VIRTUAL_STORAGE_TYPE, VIRTUAL_STORAGE_TYPE_DEVICE_UNKNOWN,
        VIRTUAL_STORAGE_TYPE_VENDOR_UNKNOWN,
    };
    use windows_sys::Win32::System::Threading::{CreateEventW, WaitForSingleObject};
    use windows_sys::Win32::System::IO::OVERLAPPED;

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
        let event = CreateEventW(std::ptr::null(), 1, 0, std::ptr::null());
        if event.is_null() {
            if attached {
                DetachVirtualDisk(handle, DETACH_VIRTUAL_DISK_FLAG_NONE, 0);
            }
            CloseHandle(handle);
            return Err(std::io::Error::last_os_error().to_string());
        }
        let mut overlapped: OVERLAPPED = std::mem::zeroed();
        overlapped.hEvent = event;
        let mut rc = CompactVirtualDisk(
            handle,
            COMPACT_VIRTUAL_DISK_FLAG_NONE,
            &compact_params,
            &overlapped,
        );
        if rc == ERROR_IO_PENDING {
            loop {
                WaitForSingleObject(event, 500);
                let mut operation: VIRTUAL_DISK_PROGRESS = std::mem::zeroed();
                let query = GetVirtualDiskOperationProgress(handle, &overlapped, &mut operation);
                if query != ERROR_SUCCESS {
                    // The OVERLAPPED storage and disk handle must outlive the operation.
                    WaitForSingleObject(event, u32::MAX);
                    rc = query;
                    break;
                }
                if operation.CompletionValue > 0 && !progress_file.is_empty() {
                    let percent = ((operation.CurrentValue as u128 * 100)
                        / operation.CompletionValue as u128)
                        .min(100);
                    if let Err(error) = std::fs::write(progress_file, percent.to_string()) {
                        eprintln!("Cannot write VHDX progress: {error}");
                    }
                }
                if operation.OperationStatus != ERROR_IO_PENDING {
                    rc = operation.OperationStatus;
                    break;
                }
            }
        }
        CloseHandle(event);

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
fn compact_virtual_disk(_vhdx: &str, _progress_file: &str) -> Result<(), String> {
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

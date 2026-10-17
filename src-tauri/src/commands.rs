use keeper_core::drive::{ inspect_path_capabilities, list_all_drives, DriveInfo, VolumeCapabilities };
use keeper_core::explorer::{
    delete_entry, list_directory, preview_image, DirectoryListing, ImagePreviewResult,
};
use keeper_core::lockhunter::{
    inspect_file_locks, schedule_delete_on_reboot, LockHunterReport,
};
use keeper_core::ops::{
    find_duplicates, inspect_system_maintenance, scan_junk_artifacts, DuplicateScanReport,
    JunkScanReport, SystemMaintenanceOverview,
};
use keeper_core::sync::{
    compare_directories, execute_sync, CompareMode, SyncComparisonReport, SyncExecutionSummary,
    SyncStrategy,
};
use keeper_core::taskmgr::{
    get_system_snapshot, kill_process, list_port_bindings, PortBindingEntry, SystemSnapshot,
};
use keeper_core::treesize::{ analyze_directory, TreeSizeReport };
use serde::Serialize;
use serde_json::{json, Value};
use std::path::Path;

fn to_api_value<T: Serialize>(res: Result<T, keeper_core::KeeperError>) -> Value {
    match res {
        Ok(data) => json!({ "ok": true, "data": data, "error": null }),
        Err(err) => json!({ "ok": false, "data": null, "error": err.to_string() }),
    }
}

/// 統一派發前端 IPC / RPC 指令至 keeper-core 引擎
pub fn dispatch_command(cmd: &str, payload: &Value) -> Value {
    match cmd {
        "get_drives" => {
            let drives: Vec<DriveInfo> = list_all_drives();
            json!({ "ok": true, "data": drives, "error": null })
        }
        "inspect_capabilities" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let caps: VolumeCapabilities = inspect_path_capabilities(Path::new(path));
            json!({ "ok": true, "data": caps, "error": null })
        }
        "list_directory" => {
            let default_dir = std::env::current_dir()
                .map(|p| p.to_string_lossy().to_string())
                .unwrap_or_else(|_| ".".to_string());
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&default_dir);
            let res: Result<DirectoryListing, _> = list_directory(path);
            to_api_value(res)
        }
        "preview_image" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let max_dimension = payload
                .get("max_dimension")
                .and_then(|v| v.as_u64())
                .unwrap_or(960) as u32;
            let force_network_load = payload
                .get("force_network_load")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let res: Result<ImagePreviewResult, _> =
                preview_image(path, max_dimension, force_network_load);
            to_api_value(res)
        }
        "delete_entry" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            let use_recycle_bin = payload
                .get("use_recycle_bin")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let confirm_permanent_on_network = payload
                .get("confirm_permanent_on_network")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            to_api_value(delete_entry(
                path,
                use_recycle_bin,
                confirm_permanent_on_network,
            ))
        }
        "analyze_treesize" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let max_depth = payload
                .get("max_depth")
                .and_then(|v| v.as_u64())
                .map(|d| d as usize);
            let res: Result<TreeSizeReport, _> = analyze_directory(path, max_depth);
            to_api_value(res)
        }
        "get_system_snapshot" => {
            let max_processes = payload
                .get("max_processes")
                .and_then(|v| v.as_u64())
                .unwrap_or(80) as usize;
            let snap: SystemSnapshot = get_system_snapshot(max_processes);
            json!({ "ok": true, "data": snap, "error": null })
        }
        "kill_process" => {
            let pid = payload.get("pid").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
            to_api_value(kill_process(pid))
        }
        "list_ports" => {
            let res: Result<Vec<PortBindingEntry>, _> = list_port_bindings();
            to_api_value(res)
        }
        "inspect_locks" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let res: Result<LockHunterReport, _> = inspect_file_locks(path);
            to_api_value(res)
        }
        "schedule_boot_delete" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or_default();
            to_api_value(schedule_delete_on_reboot(path))
        }
        "compare_sync" => {
            let left = payload
                .get("left_path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let right = payload
                .get("right_path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let compare_mode = match payload
                .get("compare_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("time_and_size")
            {
                "blake3_hash" => CompareMode::Blake3Hash,
                _ => CompareMode::TimeAndSize,
            };
            let strategy = match payload
                .get("strategy")
                .and_then(|v| v.as_str())
                .unwrap_or("mirror")
            {
                "update" => SyncStrategy::Update,
                "two_way" => SyncStrategy::TwoWay,
                _ => SyncStrategy::Mirror,
            };
            let res: Result<SyncComparisonReport, _> =
                compare_directories(left, right, compare_mode, strategy);
            to_api_value(res)
        }
        "execute_sync" => {
            let left = payload
                .get("left_path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let right = payload
                .get("right_path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let compare_mode = match payload
                .get("compare_mode")
                .and_then(|v| v.as_str())
                .unwrap_or("time_and_size")
            {
                "blake3_hash" => CompareMode::Blake3Hash,
                _ => CompareMode::TimeAndSize,
            };
            let strategy = match payload
                .get("strategy")
                .and_then(|v| v.as_str())
                .unwrap_or("mirror")
            {
                "update" => SyncStrategy::Update,
                "two_way" => SyncStrategy::TwoWay,
                _ => SyncStrategy::Mirror,
            };
            let dry_run = payload
                .get("dry_run")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let res: Result<SyncExecutionSummary, _> =
                execute_sync(left, right, compare_mode, strategy, dry_run);
            to_api_value(res)
        }
        "find_duplicates" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let res: Result<DuplicateScanReport, _> = find_duplicates(path);
            to_api_value(res)
        }
        "scan_junk" => {
            let path = payload
                .get("path")
                .and_then(|v| v.as_str())
                .unwrap_or(".");
            let res: Result<JunkScanReport, _> = scan_junk_artifacts(path);
            to_api_value(res)
        }
        "inspect_maintenance" => {
            let overview: SystemMaintenanceOverview = inspect_system_maintenance();
            json!({ "ok": true, "data": overview, "error": null })
        }
        _ => json!({
            "ok": false,
            "data": null,
            "error": format!("未知的指令: {}", cmd)
        }),
    }
}

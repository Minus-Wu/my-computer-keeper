use crate::{KeeperError, KeeperResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Command;
use sysinfo::{Pid, ProcessesToUpdate, System, Users};

#[cfg(windows)]
#[path = "taskmgr_windows.rs"]
mod windows_metrics;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessEntry {
    pub pid: u32,
    pub parent_pid: Option<u32>,
    pub name: String,
    pub cpu_usage: f32,
    pub memory_bytes: u64,
    pub disk_read_bytes: u64,
    pub disk_written_bytes: u64,
    pub status: String,
    pub exe_path: String,
    pub virtual_memory_bytes: u64,
    pub disk_read_bytes_per_sec: f64,
    pub disk_written_bytes_per_sec: f64,
    pub start_time: u64,
    pub run_time_secs: u64,
    pub command_line: String,
    pub working_directory: String,
    pub user_name: Option<String>,
    pub thread_count: Option<u32>,
    pub handle_count: Option<u32>,
    pub priority: Option<String>,
    pub gpu_usage: Option<f64>,
    pub gpu_memory_bytes: Option<u64>,
    pub gpu_dedicated_memory_bytes: Option<u64>,
    pub gpu_shared_memory_bytes: Option<u64>,
    pub gpu_engine: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemSnapshot {
    pub host_name: String,
    pub os_version: String,
    pub uptime_secs: u64,
    pub cpu_brand: String,
    pub global_cpu_usage: f32,
    pub per_core_usage: Vec<f32>,
    pub total_memory_bytes: u64,
    pub used_memory_bytes: u64,
    pub total_swap_bytes: u64,
    pub used_swap_bytes: u64,
    pub total_processes: usize,
    pub processes: Vec<ProcessEntry>,
    pub gpu_metrics_notice: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortBindingEntry {
    pub protocol: String,
    pub local_address: String,
    pub local_port: u16,
    pub remote_address: String,
    pub state: String,
    pub pid: u32,
    pub process_name: String,
    pub exe_path: String,
}

/// 擷取即時工作管理員系統快照與行程清單
pub fn get_system_snapshot(max_processes: usize) -> SystemSnapshot {
    let mut sys = System::new_all();
    #[cfg(windows)]
    let mut gpu_query = windows_metrics::GpuQuery::new();
    let sample_start = std::time::Instant::now();
    // CPU, disk rates, and GPU utilization need two samples of the same counters.
    std::thread::sleep(
        sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(std::time::Duration::from_millis(250)),
    );
    sys.refresh_all();
    let sample_secs = sample_start.elapsed().as_secs_f64();
    let users = Users::new_with_refreshed_list();
    #[cfg(windows)]
    let gpu = gpu_query.sample();
    #[cfg(windows)]
    let thread_counts = windows_metrics::thread_counts();

    let host_name = System::host_name().unwrap_or_else(|| "Local-PC".to_string());
    let os_version = format!(
        "{} {}",
        System::name().unwrap_or_else(|| "Windows".to_string()),
        System::os_version().unwrap_or_default()
    );
    let uptime_secs = System::uptime();

    let cpus = sys.cpus();
    let cpu_brand = cpus
        .first()
        .map(|c| c.brand().trim().to_string())
        .unwrap_or_else(|| "Generic CPU".to_string());
    let per_core_usage: Vec<f32> = cpus.iter().map(|c| c.cpu_usage()).collect();
    let global_cpu_usage = sys.global_cpu_usage();

    let total_memory_bytes = sys.total_memory();
    let used_memory_bytes = sys.used_memory();
    let total_swap_bytes = sys.total_swap();
    let used_swap_bytes = sys.used_swap();

    let all_procs = sys.processes();
    let total_processes = all_procs.len();

    let mut processes: Vec<ProcessEntry> = all_procs
        .values()
        .map(|p| {
            let disk = p.disk_usage();
            ProcessEntry {
                pid: p.pid().as_u32(),
                parent_pid: p.parent().map(|pp| pp.as_u32()),
                name: p.name().to_string_lossy().to_string(),
                cpu_usage: p.cpu_usage() / sys.cpus().len().max(1) as f32,
                memory_bytes: p.memory(),
                disk_read_bytes: disk.total_read_bytes,
                disk_written_bytes: disk.total_written_bytes,
                status: format!("{:?}", p.status()),
                exe_path: p
                    .exe()
                    .map(|e| e.to_string_lossy().to_string())
                    .unwrap_or_default(),
                virtual_memory_bytes: p.virtual_memory(),
                disk_read_bytes_per_sec: disk.read_bytes as f64 / sample_secs,
                disk_written_bytes_per_sec: disk.written_bytes as f64 / sample_secs,
                start_time: p.start_time(),
                run_time_secs: p.run_time(),
                command_line: p
                    .cmd()
                    .iter()
                    .map(|arg| arg.to_string_lossy())
                    .collect::<Vec<_>>()
                    .join(" "),
                working_directory: p
                    .cwd()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                user_name: p
                    .user_id()
                    .and_then(|id| users.get_user_by_id(id))
                    .map(|user| user.name().to_owned()),
                thread_count: None,
                handle_count: None,
                priority: None,
                gpu_usage: None,
                gpu_memory_bytes: None,
                gpu_dedicated_memory_bytes: None,
                gpu_shared_memory_bytes: None,
                gpu_engine: None,
            }
        })
        .collect();

    #[cfg(windows)]
    for process in &mut processes {
        let native = windows_metrics::process_details(process.pid);
        process.thread_count = thread_counts.get(&process.pid).copied();
        process.handle_count = native.0;
        process.priority = native.1;
        let metrics = gpu.for_pid(process.pid);
        process.gpu_usage = metrics.usage;
        process.gpu_dedicated_memory_bytes = metrics.dedicated;
        process.gpu_shared_memory_bytes = metrics.shared;
        process.gpu_memory_bytes = metrics
            .dedicated
            .zip(metrics.shared)
            .map(|(d, s)| d.saturating_add(s));
        process.gpu_engine = metrics.engine;
    }

    processes.sort_by_key(|a| std::cmp::Reverse(a.memory_bytes));
    processes.truncate(max_processes.clamp(10, 500));

    SystemSnapshot {
        host_name,
        os_version,
        uptime_secs,
        cpu_brand,
        global_cpu_usage,
        per_core_usage,
        total_memory_bytes,
        used_memory_bytes,
        total_swap_bytes,
        used_swap_bytes,
        total_processes,
        processes,
        #[cfg(windows)]
        gpu_metrics_notice: gpu.notice,
        #[cfg(not(windows))]
        gpu_metrics_notice: Some(
            "GPU counters are only supported on Windows with a WDDM driver.".into(),
        ),
    }
}

/// 終止指定 PID 的處理程序
pub fn kill_process(pid: u32) -> KeeperResult<String> {
    let mut sys = System::new();
    let target_pid = Pid::from_u32(pid);
    sys.refresh_processes(ProcessesToUpdate::Some(&[target_pid]), true);

    let process = sys
        .process(target_pid)
        .ok_or_else(|| KeeperError::SystemError(format!("找不到 PID 為 {} 的處理程序", pid)))?;

    let name = process.name().to_string_lossy().to_string();
    if process.kill() {
        Ok(format!("已成功終止處理程序「{}」(PID: {})。", name, pid))
    } else {
        Err(KeeperError::SystemError(format!(
            "無法終止處理程序「{}」(PID: {})，可能需要系統管理員權限。",
            name, pid
        )))
    }
}

fn split_addr_port(endpoint: &str) -> (String, u16) {
    if let Some(idx) = endpoint.rfind(':') {
        let addr = endpoint[..idx].to_string();
        let port = endpoint[idx + 1..].parse::<u16>().unwrap_or(0);
        (addr, port)
    } else {
        (endpoint.to_string(), 0)
    }
}

/// 查詢本機 TCP/UDP 連接埠佔用清單（Port Hunter）
pub fn list_port_bindings() -> KeeperResult<Vec<PortBindingEntry>> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let mut proc_map: HashMap<u32, (String, String)> = HashMap::new();
    for (pid, proc_info) in sys.processes() {
        proc_map.insert(
            pid.as_u32(),
            (
                proc_info.name().to_string_lossy().to_string(),
                proc_info
                    .exe()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_default(),
            ),
        );
    }

    let output = Command::new("netstat")
        .args(["-ano"])
        .output()
        .map_err(|e| KeeperError::SystemError(format!("無法執行 netstat: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut entries = Vec::new();

    for line in stdout.lines() {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() < 4 {
            continue;
        }
        let proto = parts[0].to_ascii_uppercase();
        if proto != "TCP" && proto != "UDP" {
            continue;
        }

        let (local_address, local_port) = split_addr_port(parts[1]);
        if local_port == 0 {
            continue;
        }

        let (remote_address, state, pid_str) = if proto == "TCP" && parts.len() >= 5 {
            (parts[2].to_string(), parts[3].to_string(), parts[4])
        } else {
            (parts[2].to_string(), "STATELESS".to_string(), parts[3])
        };

        let pid = pid_str.parse::<u32>().unwrap_or(0);
        let (process_name, exe_path) = proc_map
            .get(&pid)
            .cloned()
            .unwrap_or_else(|| ("System / Unknown".to_string(), String::new()));

        entries.push(PortBindingEntry {
            protocol: proto,
            local_address,
            local_port,
            remote_address,
            state,
            pid,
            process_name,
            exe_path,
        });
    }

    entries.sort_by_key(|a| a.local_port);
    entries.dedup_by(|a, b| {
        a.protocol == b.protocol && a.local_port == b.local_port && a.pid == b.pid
    });
    entries.truncate(300);

    Ok(entries)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_snapshot_returns_valid_data() {
        let snap = get_system_snapshot(25);
        assert!(snap.total_memory_bytes > 0);
        assert!(!snap.processes.is_empty());
        assert!(snap.processes.len() <= 25);
        for process in snap.processes {
            assert!(process.cpu_usage.is_finite());
            assert!(process.disk_read_bytes_per_sec.is_finite());
            if let Some(usage) = process.gpu_usage {
                assert!((0.0..=100.0).contains(&usage));
            }
            assert_eq!(
                process.gpu_memory_bytes,
                process
                    .gpu_dedicated_memory_bytes
                    .zip(process.gpu_shared_memory_bytes)
                    .map(|(dedicated, shared)| dedicated.saturating_add(shared))
            );
        }
    }

    #[test]
    fn test_split_addr_port_parsing() {
        assert_eq!(
            split_addr_port("127.0.0.1:8080"),
            ("127.0.0.1".to_string(), 8080)
        );
        assert_eq!(split_addr_port("[::1]:3000"), ("[::1]".to_string(), 3000));
    }
}

use std::collections::HashMap;
use windows_sys::Win32::{
    Foundation::{CloseHandle, INVALID_HANDLE_VALUE},
    System::{
        Diagnostics::ToolHelp::{
            CreateToolhelp32Snapshot, Process32FirstW, Process32NextW, PROCESSENTRY32W,
            TH32CS_SNAPPROCESS,
        },
        Performance::{
            PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData,
            PdhGetFormattedCounterArrayW, PdhOpenQueryW, PDH_CSTATUS_NEW_DATA,
            PDH_CSTATUS_VALID_DATA, PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PDH_MORE_DATA,
        },
        Threading::{
            GetPriorityClass, GetProcessHandleCount, OpenProcess, ABOVE_NORMAL_PRIORITY_CLASS,
            BELOW_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, IDLE_PRIORITY_CLASS,
            NORMAL_PRIORITY_CLASS, PROCESS_QUERY_LIMITED_INFORMATION, REALTIME_PRIORITY_CLASS,
        },
    },
};

type CounterValues = Vec<(String, Option<f64>)>;

pub struct GpuQuery {
    query: isize,
    counters: [Result<isize, String>; 3],
    collection_error: Option<String>,
}

impl GpuQuery {
    pub fn new() -> Self {
        let mut query = 0;
        let status = unsafe { PdhOpenQueryW(std::ptr::null(), 0, &mut query) };
        let paths = [
            r"\GPU Engine(*)\Utilization Percentage",
            r"\GPU Process Memory(*)\Dedicated Usage",
            r"\GPU Process Memory(*)\Shared Usage",
        ];
        let counters = paths.map(|path| {
            if status != 0 {
                return Err(format!("PdhOpenQuery: 0x{status:08X}"));
            }
            let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
            let mut counter = 0;
            let status = unsafe { PdhAddEnglishCounterW(query, wide.as_ptr(), 0, &mut counter) };
            if status == 0 {
                Ok(counter)
            } else {
                Err(format!("{path}: 0x{status:08X}"))
            }
        });
        let status = if query != 0 {
            unsafe { PdhCollectQueryData(query) }
        } else {
            status
        };
        Self {
            query,
            counters,
            collection_error: (status != 0).then(|| format!("GPU baseline: 0x{status:08X}")),
        }
    }

    pub fn sample(&mut self) -> GpuSnapshot {
        let status = if self.query != 0 {
            unsafe { PdhCollectQueryData(self.query) }
        } else {
            0
        };
        let mut errors = Vec::new();
        if let Some(error) = &self.collection_error {
            errors.push(error.clone());
        }
        if status != 0 {
            errors.push(format!("GPU sample: 0x{status:08X}"));
        }
        let values = self.counters.each_ref().map(|counter| {
            let result = counter
                .as_ref()
                .map_err(Clone::clone)
                .and_then(|counter| read_counter(*counter));
            match result {
                Ok(values) if status == 0 && self.collection_error.is_none() => Some(values),
                Ok(_) => None,
                Err(error) => {
                    errors.push(error);
                    None
                }
            }
        });
        let [engines, dedicated, shared] = values;
        GpuSnapshot {
            engines,
            dedicated,
            shared,
            notice: (!errors.is_empty()).then(|| errors.join("; ")),
        }
    }
}

impl Drop for GpuQuery {
    fn drop(&mut self) {
        if self.query != 0 {
            unsafe { PdhCloseQuery(self.query) };
        }
    }
}

fn read_counter(counter: isize) -> Result<CounterValues, String> {
    let mut bytes = 0;
    let mut count = 0;
    let status = unsafe {
        PdhGetFormattedCounterArrayW(
            counter,
            PDH_FMT_DOUBLE,
            &mut bytes,
            &mut count,
            std::ptr::null_mut(),
        )
    };
    if status == 0 && bytes == 0 {
        return Ok(Vec::new());
    }
    if status != PDH_MORE_DATA {
        return Err(format!("GPU counter size: 0x{status:08X}"));
    }
    // The buffer holds both aligned structs and the UTF-16 instance names they point to.
    for _ in 0..3 {
        let mut buffer = vec![0u64; (bytes as usize).div_ceil(std::mem::size_of::<u64>())];
        let items = buffer.as_mut_ptr().cast::<PDH_FMT_COUNTERVALUE_ITEM_W>();
        let status = unsafe {
            PdhGetFormattedCounterArrayW(counter, PDH_FMT_DOUBLE, &mut bytes, &mut count, items)
        };
        if status == PDH_MORE_DATA {
            continue;
        }
        if status != 0 {
            return Err(format!("GPU counter values: 0x{status:08X}"));
        }
        let mut values = Vec::with_capacity(count as usize);
        for item in unsafe { std::slice::from_raw_parts(items, count as usize) } {
            if item.szName.is_null() {
                continue;
            }
            let mut len = 0;
            unsafe {
                while *item.szName.add(len) != 0 {
                    len += 1;
                }
            }
            let name =
                String::from_utf16_lossy(unsafe { std::slice::from_raw_parts(item.szName, len) });
            let value = unsafe { item.FmtValue.Anonymous.doubleValue };
            let valid = matches!(
                item.FmtValue.CStatus,
                PDH_CSTATUS_VALID_DATA | PDH_CSTATUS_NEW_DATA
            ) && value.is_finite()
                && value >= 0.0;
            values.push((name, valid.then_some(value)));
        }
        return Ok(values);
    }
    Err("GPU counter instances changed during all three read attempts.".into())
}

fn instance_pid(name: &str) -> Option<u32> {
    name.strip_prefix("pid_")?.split('_').next()?.parse().ok()
}

pub struct GpuSnapshot {
    engines: Option<CounterValues>,
    dedicated: Option<CounterValues>,
    shared: Option<CounterValues>,
    pub notice: Option<String>,
}

pub struct ProcessGpuMetrics {
    pub usage: Option<f64>,
    pub dedicated: Option<u64>,
    pub shared: Option<u64>,
    pub engine: Option<String>,
}

impl GpuSnapshot {
    pub fn for_pid(&self, pid: u32) -> ProcessGpuMetrics {
        let mut engine_totals = HashMap::<String, f64>::new();
        let mut usage_valid = self.engines.is_some();
        if let Some(values) = &self.engines {
            for (name, value) in values
                .iter()
                .filter(|(name, _)| instance_pid(name) == Some(pid))
            {
                match value {
                    Some(value) => {
                        let engine = name
                            .split_once("_luid_")
                            .map(|(_, suffix)| suffix)
                            .unwrap_or(name);
                        let engine = engine.split('#').next().unwrap_or(engine).to_owned();
                        *engine_totals.entry(engine).or_default() += value;
                    }
                    None => usage_valid = false,
                }
            }
        }
        // Like Task Manager, show the busiest GPU engine, not a sum across independent engines.
        let busiest = engine_totals.into_iter().max_by(|a, b| a.1.total_cmp(&b.1));
        ProcessGpuMetrics {
            usage: usage_valid.then(|| {
                busiest
                    .as_ref()
                    .map_or(0.0, |(_, value)| value.clamp(0.0, 100.0))
            }),
            engine: usage_valid
                .then(|| {
                    busiest
                        .filter(|(_, value)| *value > 0.0)
                        .map(|(engine, _)| format!("luid_{engine}"))
                })
                .flatten(),
            dedicated: sum_memory(&self.dedicated, pid),
            shared: sum_memory(&self.shared, pid),
        }
    }
}

fn sum_memory(values: &Option<CounterValues>, pid: u32) -> Option<u64> {
    let values = values.as_ref()?;
    let mut total = 0u64;
    for (_, value) in values
        .iter()
        .filter(|(name, _)| instance_pid(name) == Some(pid))
    {
        total = total.saturating_add((*value)? as u64);
    }
    Some(total)
}

pub fn thread_counts() -> HashMap<u32, u32> {
    let snapshot = unsafe { CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0) };
    if snapshot == INVALID_HANDLE_VALUE {
        return HashMap::new();
    }
    let mut entry: PROCESSENTRY32W = unsafe { std::mem::zeroed() };
    entry.dwSize = std::mem::size_of::<PROCESSENTRY32W>() as u32;
    let mut counts = HashMap::new();
    let mut has_entry = unsafe { Process32FirstW(snapshot, &mut entry) } != 0;
    while has_entry {
        counts.insert(entry.th32ProcessID, entry.cntThreads);
        has_entry = unsafe { Process32NextW(snapshot, &mut entry) } != 0;
    }
    unsafe { CloseHandle(snapshot) };
    counts
}

pub fn process_details(pid: u32) -> (Option<u32>, Option<String>) {
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return (None, None);
    }
    let mut handles = 0;
    let handles = (unsafe { GetProcessHandleCount(process, &mut handles) } != 0).then_some(handles);
    let priority = match unsafe { GetPriorityClass(process) } {
        IDLE_PRIORITY_CLASS => Some("Idle"),
        BELOW_NORMAL_PRIORITY_CLASS => Some("Below normal"),
        NORMAL_PRIORITY_CLASS => Some("Normal"),
        ABOVE_NORMAL_PRIORITY_CLASS => Some("Above normal"),
        HIGH_PRIORITY_CLASS => Some("High"),
        REALTIME_PRIORITY_CLASS => Some("Realtime"),
        _ => None,
    };
    unsafe { CloseHandle(process) };
    (handles, priority.map(str::to_owned))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_uses_busiest_engine_and_sums_memory_across_adapters() {
        let gpu = GpuSnapshot {
            engines: Some(vec![
                ("pid_42_luid_a_phys_0_eng_0_engtype_3D".into(), Some(30.0)),
                ("pid_42_luid_a_phys_0_eng_0_engtype_3D#1".into(), Some(25.0)),
                ("pid_42_luid_b_phys_0_eng_1_engtype_Copy".into(), Some(40.0)),
                ("pid_420_luid_a_phys_0_eng_0_engtype_3D".into(), Some(99.0)),
            ]),
            dedicated: Some(vec![
                ("pid_42_luid_a".into(), Some(100.0)),
                ("pid_42_luid_b".into(), Some(200.0)),
            ]),
            shared: Some(vec![("pid_42_luid_a".into(), Some(50.0))]),
            notice: None,
        };
        let process = gpu.for_pid(42);
        assert_eq!(process.usage, Some(55.0));
        assert_eq!(process.dedicated, Some(300));
        assert_eq!(process.shared, Some(50));
        assert!(process.engine.unwrap().ends_with("3D"));
        assert_eq!(gpu.for_pid(7).usage, Some(0.0));
        assert_eq!(gpu.for_pid(7).dedicated, Some(0));
    }

    #[test]
    fn missing_or_invalid_gpu_counters_are_not_zero() {
        let gpu = GpuSnapshot {
            engines: Some(vec![("pid_42_luid_a".into(), None)]),
            dedicated: None,
            shared: Some(vec![("pid_42_luid_a".into(), None)]),
            notice: Some("Unavailable".into()),
        };
        let process = gpu.for_pid(42);
        assert_eq!(process.usage, None);
        assert_eq!(process.dedicated, None);
        assert_eq!(process.shared, None);
        assert_eq!(process.engine, None);
    }
}

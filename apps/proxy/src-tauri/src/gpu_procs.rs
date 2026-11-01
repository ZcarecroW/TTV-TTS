//! Per-process GPU load and video memory on Windows, from the performance
//! counters Task Manager uses ("GPU Engine" and "GPU Process Memory"). Works
//! for every vendor; NVML can't report per-process memory under WDDM.
//!
//! Instance names look like `pid_1234_luid_0x0_0xD1F2_phys_0_eng_3_engtype_3D`.
//! A process' load is its busiest engine type (like Task Manager's column).

use std::collections::HashMap;

use windows_sys::Win32::System::Performance::{
    PDH_FMT_COUNTERVALUE_ITEM_W, PDH_FMT_DOUBLE, PdhAddEnglishCounterW, PdhCloseQuery, PdhCollectQueryData,
    PdhGetFormattedCounterArrayW, PdhOpenQueryW,
};

const PDH_MORE_DATA: u32 = 0x8000_07D2;
/// Values above 100 are allowed (engine sums); not exported by windows-sys.
const PDH_FMT_NOCAP100: u32 = 0x0000_8000;

pub struct GpuCounters {
    query: isize,
    util: isize,
    mem: isize,
    primed: bool,
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

impl GpuCounters {
    pub fn new() -> Option<Self> {
        let mut query = 0isize;
        let mut util = 0isize;
        let mut mem = 0isize;
        // SAFETY: plain PDH calls with valid out-pointers; handles are closed in Drop.
        unsafe {
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query as *mut isize as _) != 0 {
                return None;
            }
            let ok1 = PdhAddEnglishCounterW(query as _, wide("\\GPU Engine(*)\\Utilization Percentage").as_ptr(), 0, &mut util as *mut isize as _);
            let ok2 = PdhAddEnglishCounterW(query as _, wide("\\GPU Process Memory(*)\\Dedicated Usage").as_ptr(), 0, &mut mem as *mut isize as _);
            if ok1 != 0 || ok2 != 0 {
                PdhCloseQuery(query as _);
                return None;
            }
        }
        Some(Self { query, util, mem, primed: false })
    }

    /// pid -> (load in percent, dedicated video memory in bytes).
    pub fn sample(&mut self) -> HashMap<u32, (f32, u64)> {
        let mut out = HashMap::new();
        // SAFETY: valid query handle.
        if unsafe { PdhCollectQueryData(self.query as _) } != 0 {
            return out;
        }
        // Rates need two readings; the first call only primes them.
        if !self.primed {
            self.primed = true;
            std::thread::sleep(std::time::Duration::from_millis(250));
            // SAFETY: as above.
            unsafe { PdhCollectQueryData(self.query as _) };
        }
        // engine load: per pid and engine type, summed; then the busiest type
        let mut per_type: HashMap<(u32, String), f64> = HashMap::new();
        for (name, value) in read(self.util) {
            let Some(pid) = pid_of(&name) else { continue };
            let ty = name.rsplit("engtype_").next().unwrap_or("").to_string();
            *per_type.entry((pid, ty)).or_insert(0.0) += value;
        }
        for ((pid, _), v) in per_type {
            let e = out.entry(pid).or_insert((0.0f32, 0u64));
            e.0 = e.0.max(v as f32);
        }
        for (name, value) in read(self.mem) {
            let Some(pid) = pid_of(&name) else { continue };
            out.entry(pid).or_insert((0.0, 0)).1 += value.max(0.0) as u64;
        }
        out
    }
}

impl Drop for GpuCounters {
    fn drop(&mut self) {
        // SAFETY: handle from PdhOpenQueryW, closed once.
        unsafe { PdhCloseQuery(self.query as _) };
    }
}

fn pid_of(instance: &str) -> Option<u32> {
    instance.strip_prefix("pid_")?.split('_').next()?.parse().ok()
}

/// All instances of a wildcard counter as (name, value).
fn read(counter: isize) -> Vec<(String, f64)> {
    let mut size: u32 = 0;
    let mut count: u32 = 0;
    // SAFETY: first call asks for the buffer size; the second fills a buffer of
    // exactly that size, which outlives every pointer read from it.
    unsafe {
        let r = PdhGetFormattedCounterArrayW(counter as _, PDH_FMT_DOUBLE | PDH_FMT_NOCAP100, &mut size, &mut count, std::ptr::null_mut());
        if r != PDH_MORE_DATA || size == 0 {
            return Vec::new();
        }
        let mut buf: Vec<u8> = vec![0; size as usize];
        let items = buf.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
        if PdhGetFormattedCounterArrayW(counter as _, PDH_FMT_DOUBLE | PDH_FMT_NOCAP100, &mut size, &mut count, items) != 0 {
            return Vec::new();
        }
        let slice = std::slice::from_raw_parts(items, count as usize);
        slice
            .iter()
            .filter(|it| it.FmtValue.CStatus == 0)
            .map(|it| {
                let mut len = 0;
                while *it.szName.add(len) != 0 {
                    len += 1;
                }
                let name = String::from_utf16_lossy(std::slice::from_raw_parts(it.szName, len));
                (name, it.FmtValue.Anonymous.doubleValue)
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instance_pids() {
        assert_eq!(pid_of("pid_1234_luid_0x00000000_0x0000D1F2_phys_0_eng_3_engtype_3D"), Some(1234));
        assert_eq!(pid_of("pid_88_luid_0x0_0x1_phys_0"), Some(88));
        assert_eq!(pid_of("_Total"), None);
    }

    #[test]
    fn counters_open_on_windows() {
        // The counters exist on Windows 10 1709+ with a WDDM 2.x driver.
        if let Some(mut c) = GpuCounters::new() {
            let _ = c.sample();
        }
    }
}

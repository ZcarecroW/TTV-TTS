//! Live load of the proxy PC (CPU, RAM, GPU usage/VRAM, NVIDIA temperature
//! and power) and its network addresses. Sampling runs on its own thread and
//! only while someone watches (window open or an app subscribed), so an idle
//! proxy costs nothing.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::{Duration, Instant};

use sysinfo::{Gpus, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use ttv_link::{GpuProcess, GpuStats, NetAddress, SystemStats};

use crate::state::AppState;

const SAMPLE_EVERY: Duration = Duration::from_millis(2000);
const ADDRESSES_EVERY: Duration = Duration::from_secs(60);
const VIRTUAL: &[&str] = &["vethernet", "wsl", "hyper-v", "virtualbox", "vbox", "vmware", "vmnet", "docker", "loopback", "npcap"];

struct Monitor {
    sys: Option<System>,
    /// None until first used; stays None where the OS gives no GPU list.
    gpus: Option<Option<Gpus>>,
    nvml: Option<nvml_wrapper::Nvml>,
    nvml_tried: bool,
    /// pid -> program name, so the GPU process list needs no full process scan.
    names: HashMap<u32, String>,
}

impl Monitor {
    fn new() -> Self {
        Self { sys: System::new().ok(), gpus: None, nvml: None, nvml_tried: false, names: HashMap::new() }
    }

    fn sample(&mut self) -> SystemStats {
        if let Some(sys) = self.sys.as_mut() {
            sys.refresh_cpu_usage();
            sys.refresh_memory();
        }
        let mut gpus = Vec::new();

        // NVIDIA: NVML (comes with the driver) gives usage, VRAM, temperature
        // and power. Loaded at runtime; missing on other systems.
        if !self.nvml_tried {
            self.nvml_tried = true;
            self.nvml = nvml_wrapper::Nvml::init().ok();
        }
        let mut nvidia = 0;
        let mut unnamed: Vec<u32> = Vec::new();
        if let Some(nvml) = &self.nvml {
            let count = nvml.device_count().unwrap_or(0);
            for i in 0..count {
                let Ok(d) = nvml.device_by_index(i) else { continue };
                let mem = d.memory_info().ok();
                let processes = gpu_processes(&d);
                unnamed.extend(processes.iter().filter(|p| !self.names.contains_key(&p.pid)).map(|p| p.pid));
                gpus.push(GpuStats {
                    name: d.name().unwrap_or_default(),
                    vendor: Some("NVIDIA".into()),
                    usage_percent: d.utilization_rates().ok().map(|u| u.gpu as f32),
                    vram_used: mem.as_ref().map(|m| m.used),
                    vram_total: mem.as_ref().map(|m| m.total),
                    temperature_c: d.temperature(nvml_wrapper::enum_wrappers::device::TemperatureSensor::Gpu).ok(),
                    power_w: d.power_usage().ok().map(|mw| mw as f32 / 1000.0),
                    processes,
                });
                nvidia += 1;
            }
        }
        // Everything else (AMD, Intel, Apple) through the OS.
        let g = self.gpus.get_or_insert_with(|| Gpus::new_with_refreshed_list().ok());
        if let Some(g) = g.as_mut() {
            g.refresh(true);
        }
        for gpu in g.iter().flat_map(|g| g.iter()) {
            let vendor = gpu.vendor().unwrap_or_default();
            if nvidia > 0 && vendor.to_ascii_lowercase().contains("nvidia") {
                continue;
            }
            gpus.push(GpuStats {
                name: gpu.model().unwrap_or("GPU").to_string(),
                vendor: (!vendor.is_empty()).then(|| vendor.to_string()),
                usage_percent: gpu.usage(),
                vram_used: gpu.used_memory(),
                vram_total: gpu.total_memory(),
                temperature_c: None,
                power_w: None,
                processes: Vec::new(),
            });
        }

        // Names for processes seen for the first time (one targeted lookup).
        if !unnamed.is_empty()
            && let Some(sys) = self.sys.as_mut()
        {
            let pids: Vec<Pid> = unnamed.iter().map(|p| Pid::from_u32(*p)).collect();
            sys.refresh_processes_specifics(ProcessesToUpdate::Some(&pids), false, ProcessRefreshKind::nothing());
            for pid in &unnamed {
                let name = sys.process(Pid::from_u32(*pid)).map(|p| p.name().to_string_lossy().into_owned()).unwrap_or_else(|| format!("PID {pid}"));
                self.names.insert(*pid, name);
            }
        }
        for g in &mut gpus {
            for p in &mut g.processes {
                if let Some(n) = self.names.get(&p.pid) {
                    p.name = n.clone();
                }
            }
        }
        if self.names.len() > 512 {
            self.names.clear();
        }
        let (cpu_percent, mem_used, mem_total) = match self.sys.as_ref() {
            Some(s) => (s.global_cpu_usage(), s.used_memory(), s.total_memory()),
            None => (0.0, 0, 0),
        };
        SystemStats { cpu_percent, mem_used, mem_total, gpus }
    }
}

/// Compute and graphics processes on an NVIDIA GPU, merged by pid, largest
/// video memory first. On Windows (WDDM) the driver often reports no
/// per-process memory for graphics processes; they are listed without it.
fn gpu_processes(d: &nvml_wrapper::Device) -> Vec<GpuProcess> {
    use nvml_wrapper::enums::device::UsedGpuMemory;
    let mut by_pid: HashMap<u32, Option<u64>> = HashMap::new();
    let lists = [d.running_compute_processes().unwrap_or_default(), d.running_graphics_processes().unwrap_or_default()];
    for p in lists.iter().flatten() {
        let mem = match p.used_gpu_memory {
            UsedGpuMemory::Used(b) => Some(b),
            UsedGpuMemory::Unavailable => None,
        };
        let e = by_pid.entry(p.pid).or_insert(None);
        *e = match (*e, mem) {
            (Some(a), Some(b)) => Some(a.max(b)),
            (a, b) => a.or(b),
        };
    }
    let mut out: Vec<GpuProcess> = by_pid.into_iter().map(|(pid, vram)| GpuProcess { pid, name: String::new(), vram }).collect();
    out.sort_by(|a, b| b.vram.unwrap_or(0).cmp(&a.vram.unwrap_or(0)).then(a.pid.cmp(&b.pid)));
    out.truncate(12);
    out
}

/// One sample without the monitor thread (CPU needs two readings).
pub async fn sample_once() -> SystemStats {
    tokio::task::spawn_blocking(|| {
        let mut m = Monitor::new();
        let _ = m.sample();
        std::thread::sleep(sysinfo::MINIMUM_CPU_UPDATE_INTERVAL.max(Duration::from_millis(250)));
        m.sample()
    })
    .await
    .unwrap_or_default()
}

/// IPv4 addresses of the real network interfaces, with their MAC address.
pub fn addresses() -> Vec<NetAddress> {
    let Ok(nets) = Networks::new_with_refreshed_list() else { return Vec::new() };
    let mut out = Vec::new();
    for (name, data) in nets.iter() {
        // Virtual adapters (WSL, Hyper-V, VirtualBox, VMware, Docker) can't be
        // reached from other PCs: listing them would only confuse.
        let lname = name.to_string_lossy().to_ascii_lowercase();
        if VIRTUAL.iter().any(|v| lname.contains(v)) {
            continue;
        }
        let mac = data.mac_address().to_string();
        if mac == "00:00:00:00:00:00" {
            continue;
        }
        for ip in data.ip_networks() {
            let std::net::IpAddr::V4(v4) = ip.addr else { continue };
            if v4.is_loopback() || v4.is_link_local() || v4.is_unspecified() {
                continue;
            }
            out.push(NetAddress { interface: name.to_string_lossy().into_owned(), ip: v4.to_string(), prefix: ip.prefix, mac: mac.clone() });
        }
    }
    // Private LAN addresses first (192.168.x, 10.x, 172.16-31.x).
    out.sort_by_key(|a| (!a.ip.parse::<std::net::Ipv4Addr>().is_ok_and(|ip| ip.is_private()), a.ip.clone()));
    out
}

pub fn spawn(state: Arc<AppState>) {
    std::thread::Builder::new()
        .name("ttv-sysmon".into())
        .spawn(move || {
            let mut monitor: Option<Monitor> = None;
            let mut last_addr: Option<Instant> = None;
            loop {
                if last_addr.is_none_or(|t| t.elapsed() > ADDRESSES_EVERY) {
                    last_addr = Some(Instant::now());
                    let a = addresses();
                    let changed = *state.addresses.read() != a;
                    if changed {
                        *state.addresses.write() = a;
                        state.publish();
                    }
                }
                if state.system_watchers.load(Ordering::SeqCst) > 0 {
                    let m = monitor.get_or_insert_with(Monitor::new);
                    let stats = m.sample();
                    state.publish_system(stats);
                    std::thread::sleep(SAMPLE_EVERY);
                } else {
                    // Nobody watches: drop the GPU handles and idle cheaply.
                    monitor = None;
                    std::thread::sleep(Duration::from_millis(500));
                }
            }
        })
        .expect("sysmon thread");
}

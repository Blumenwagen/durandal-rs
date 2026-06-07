use anyhow::Context;
use std::{
    cmp::Ordering, fs, os::unix::fs::MetadataExt, path::Path, process::Command, time::Duration,
};

#[derive(Debug, Clone, Default)]
pub struct Snapshot {
    pub host: HostInfo,
    pub cpu: CpuInfo,
    pub memory: MemoryInfo,
    pub swap: MemoryInfo,
    pub network: NetworkInfo,
    pub disks: Vec<DiskInfo>,
    pub top_processes: Vec<ProcessInfo>,
    pub docker: DockerInfo,
    pub gpus: Vec<GpuInfo>,
}

#[derive(Debug, Clone, Default)]
pub struct HostInfo {
    pub hostname: String,
    pub user: String,
    pub os: String,
    pub kernel: String,
    pub arch: String,
    pub uptime: String,
}

#[derive(Debug, Clone, Default)]
pub struct CpuInfo {
    pub percent: f64,
    pub model: String,
    pub cores: usize,
    pub threads: usize,
}

#[derive(Debug, Clone, Default)]
pub struct MemoryInfo {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub percent: f64,
    pub cached_bytes: u64,
    pub buffers_bytes: u64,
}

impl MemoryInfo {
    pub fn default_swap() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, Default)]
pub struct NetworkInfo {
    pub recv_bytes_per_sec: u64,
    pub sent_bytes_per_sec: u64,
}

#[derive(Debug, Clone, Default)]
pub struct DiskInfo {
    pub mountpoint: String,
    pub device: String,
    pub filesystem: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub percent: f64,
}

#[derive(Debug, Clone, Default)]
pub struct ProcessInfo {
    pub pid: i32,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub rss_bytes: u64,
    pub status: String,
    pub user: String,
    pub command: String,
}

#[derive(Debug, Clone, Default)]
pub struct DockerInfo {
    pub available: bool,
    pub error: Option<String>,
    pub containers: Vec<ContainerInfo>,
}

impl DockerInfo {
    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            available: false,
            error: Some(message.into()),
            containers: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct ContainerInfo {
    pub id: String,
    pub image: String,
    pub name: String,
    pub status: String,
    pub state: String,
    pub ports: String,
    pub running: bool,
}

#[derive(Debug, Clone, Default)]
pub struct GpuInfo {
    pub name: String,
    pub utilization_percent: f64,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub temperature_c: f64,
}

pub fn collect_snapshot(top_limit: usize) -> anyhow::Result<Snapshot> {
    let top_limit = top_limit.max(1);
    let memory_pair = collect_memory();
    let total_ram = memory_pair.0.total_bytes;
    Ok(Snapshot {
        host: collect_host(),
        cpu: collect_cpu(),
        memory: memory_pair.0,
        swap: memory_pair.1,
        network: collect_network(),
        disks: collect_disks(),
        top_processes: collect_processes(top_limit, total_ram).unwrap_or_default(),
        docker: crate::docker::collect_docker(Duration::from_millis(900)),
        gpus: collect_gpus(),
    })
}

fn collect_host() -> HostInfo {
    let hostname = fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| fs::read_to_string("/etc/hostname"))
        .unwrap_or_default()
        .trim()
        .to_string();
    let kernel = fs::read_to_string("/proc/sys/kernel/osrelease")
        .unwrap_or_default()
        .trim()
        .to_string();
    let os = fs::read_to_string("/etc/os-release")
        .ok()
        .and_then(|text| parse_os_release(&text))
        .unwrap_or_else(|| std::env::consts::OS.to_string());
    HostInfo {
        hostname,
        user: whoami::username(),
        os,
        kernel,
        arch: std::env::consts::ARCH.to_string(),
        uptime: format_uptime(read_first_number("/proc/uptime").unwrap_or(0.0) as u64),
    }
}

fn parse_os_release(text: &str) -> Option<String> {
    let mut pretty = None;
    let mut id = None;
    let mut version = None;
    for line in text.lines() {
        let (key, value) = line.split_once('=')?;
        let value = value.trim_matches('"').to_string();
        match key {
            "PRETTY_NAME" => pretty = Some(value),
            "ID" => id = Some(value),
            "VERSION_ID" => version = Some(value),
            _ => {}
        }
    }
    pretty.or_else(|| {
        Some(
            format!("{} {}", id?, version.unwrap_or_default())
                .trim()
                .to_string(),
        )
    })
}

fn collect_cpu() -> CpuInfo {
    let stat_a = read_cpu_stat();
    std::thread::sleep(Duration::from_millis(80));
    let stat_b = read_cpu_stat();
    let percent = match (stat_a, stat_b) {
        (Some(a), Some(b)) => cpu_delta_percent(a, b),
        _ => 0.0,
    };
    let cpuinfo = fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let mut model = String::new();
    let mut processors = 0usize;
    let mut cores = 0usize;
    for line in cpuinfo.lines() {
        if line.starts_with("processor") {
            processors += 1;
        }
        if model.is_empty() && line.starts_with("model name") {
            model = line
                .split_once(':')
                .map(|(_, v)| v.trim().to_string())
                .unwrap_or_default();
        }
        if cores == 0 && line.starts_with("cpu cores") {
            cores = line
                .split_once(':')
                .and_then(|(_, v)| v.trim().parse().ok())
                .unwrap_or(0);
        }
    }
    CpuInfo {
        percent,
        model,
        cores,
        threads: processors,
    }
}

#[derive(Copy, Clone)]
struct CpuTicks {
    idle: u64,
    total: u64,
}
fn read_cpu_stat() -> Option<CpuTicks> {
    let text = fs::read_to_string("/proc/stat").ok()?;
    let line = text.lines().find(|line| line.starts_with("cpu "))?;
    let values: Vec<u64> = line
        .split_whitespace()
        .skip(1)
        .filter_map(|v| v.parse().ok())
        .collect();
    if values.len() < 4 {
        return None;
    }
    let idle = values[3] + values.get(4).copied().unwrap_or(0);
    let total = values.iter().sum();
    Some(CpuTicks { idle, total })
}
fn cpu_delta_percent(a: CpuTicks, b: CpuTicks) -> f64 {
    let total = b.total.saturating_sub(a.total);
    let idle = b.idle.saturating_sub(a.idle);
    if total == 0 {
        0.0
    } else {
        ((total - idle) as f64 / total as f64 * 100.0).clamp(0.0, 100.0)
    }
}

fn collect_memory() -> (MemoryInfo, MemoryInfo) {
    let text = fs::read_to_string("/proc/meminfo").unwrap_or_default();
    let mut map = std::collections::HashMap::new();
    for line in text.lines() {
        if let Some((key, rest)) = line.split_once(':') {
            let kb = rest
                .split_whitespace()
                .next()
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(0);
            map.insert(key, kb * 1024);
        }
    }
    let total = *map.get("MemTotal").unwrap_or(&0);
    let available = *map.get("MemAvailable").unwrap_or(&0);
    let used = total.saturating_sub(available);
    let swap_total = *map.get("SwapTotal").unwrap_or(&0);
    let swap_free = *map.get("SwapFree").unwrap_or(&0);
    let swap_used = swap_total.saturating_sub(swap_free);
    (
        MemoryInfo {
            total_bytes: total,
            used_bytes: used,
            percent: percent(used, total),
            cached_bytes: *map.get("Cached").unwrap_or(&0),
            buffers_bytes: *map.get("Buffers").unwrap_or(&0),
        },
        MemoryInfo {
            total_bytes: swap_total,
            used_bytes: swap_used,
            percent: percent(swap_used, swap_total),
            ..Default::default()
        },
    )
}

fn collect_network() -> NetworkInfo {
    // Agent parity: one-shot snapshots cannot produce an interval rate without previous state.
    NetworkInfo::default()
}

fn collect_disks() -> Vec<DiskInfo> {
    let text = fs::read_to_string("/proc/mounts").unwrap_or_default();
    let mut disks = Vec::new();
    for line in text.lines() {
        let fields: Vec<_> = line.split_whitespace().collect();
        if fields.len() < 3 {
            continue;
        }
        let device = fields[0];
        let mountpoint = fields[1];
        let fs_type = fields[2];
        if skip_mount(mountpoint, fs_type) {
            continue;
        }
        if disks.iter().any(|d: &DiskInfo| d.device == device) {
            continue;
        }
        let output = Command::new("df")
            .args(["-B1", "--output=size,used", mountpoint])
            .output();
        if let Ok(out) = output
            && out.status.success()
            && let Some(line) = String::from_utf8_lossy(&out.stdout).lines().nth(1)
        {
            let nums: Vec<u64> = line
                .split_whitespace()
                .filter_map(|v| v.parse().ok())
                .collect();
            if nums.len() == 2 && nums[0] > 0 {
                disks.push(DiskInfo {
                    mountpoint: mountpoint.to_string(),
                    device: device.to_string(),
                    filesystem: fs_type.to_string(),
                    total_bytes: nums[0],
                    used_bytes: nums[1],
                    percent: percent(nums[1], nums[0]),
                });
            }
        }
    }
    disks
}

fn skip_mount(mountpoint: &str, fs_type: &str) -> bool {
    matches!(
        fs_type,
        "squashfs" | "tmpfs" | "devtmpfs" | "proc" | "sysfs" | "cgroup" | "cgroup2" | "overlay"
    ) || mountpoint.starts_with("/snap")
        || mountpoint.starts_with("/var/lib/docker")
        || mountpoint.starts_with("/run")
        || mountpoint.starts_with("/sys")
        || mountpoint.starts_with("/dev")
}

fn collect_processes(limit: usize, total_ram: u64) -> anyhow::Result<Vec<ProcessInfo>> {
    let mut processes = Vec::new();
    let page_size = 4096u64;
    for entry in fs::read_dir("/proc").context("read /proc")? {
        let entry = entry?;
        let file_name = entry.file_name();
        let pid: i32 = match file_name.to_string_lossy().parse() {
            Ok(pid) => pid,
            Err(_) => continue,
        };
        if let Some(proc) = read_process(pid, page_size, total_ram) {
            processes.push(proc);
        }
    }
    processes.sort_by(|a, b| {
        b.memory_percent
            .partial_cmp(&a.memory_percent)
            .unwrap_or(Ordering::Equal)
            .then_with(|| a.pid.cmp(&b.pid))
    });
    processes.truncate(limit);
    Ok(processes)
}

fn read_process(pid: i32, page_size: u64, total_ram: u64) -> Option<ProcessInfo> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let start = stat.find('(')?;
    let end = stat.rfind(')')?;
    let name = stat[start + 1..end].to_string();
    let fields: Vec<_> = stat[end + 2..].split_whitespace().collect();
    if fields.len() < 22 {
        return None;
    }
    let state = fields[0].to_string();
    let rss_pages: i64 = fields[21].parse().unwrap_or(0);
    let rss_bytes = (rss_pages.max(0) as u64).saturating_mul(page_size);
    let command = fs::read(format!("/proc/{pid}/cmdline"))
        .ok()
        .map(|bytes| {
            String::from_utf8_lossy(&bytes)
                .replace('\0', " ")
                .trim()
                .to_string()
        })
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| name.clone());
    let command = if command.len() > 40 {
        format!("{}...", &command[..37])
    } else {
        command
    };
    let user = fs::metadata(format!("/proc/{pid}"))
        .ok()
        .map(|m| m.uid().to_string())
        .unwrap_or_default();
    Some(ProcessInfo {
        pid,
        name,
        cpu_percent: 0.0,
        memory_percent: percent(rss_bytes, total_ram),
        rss_bytes,
        status: state,
        user,
        command,
    })
}

fn collect_gpus() -> Vec<GpuInfo> {
    let drm = Path::new("/sys/class/drm");
    let Ok(entries) = fs::read_dir(drm) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("card") || name.contains('-') {
                return None;
            }
            let device = entry.path().join("device");
            let vendor = fs::read_to_string(device.join("vendor")).unwrap_or_default();
            let label = match vendor.trim() {
                "0x1002" => "AMD Radeon",
                "0x10de" => "NVIDIA GPU",
                "0x8086" => "Intel Graphics",
                _ => "Generic GPU",
            };
            Some(GpuInfo {
                name: label.to_string(),
                ..Default::default()
            })
        })
        .collect()
}

fn read_first_number(path: &str) -> Option<f64> {
    fs::read_to_string(path)
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()
}

fn format_uptime(seconds: u64) -> String {
    let days = seconds / 86_400;
    let hours = (seconds % 86_400) / 3_600;
    let minutes = (seconds % 3_600) / 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{minutes}m")
    }
}

fn percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}

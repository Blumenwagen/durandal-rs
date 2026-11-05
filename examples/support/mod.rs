use crate::metrics::*;

pub fn snapshot() -> Snapshot {
    let gib = 1024 * 1024 * 1024;
    Snapshot {
        host: HostInfo {
            hostname: "sentinel-01".into(),
            user: "lapis".into(),
            os: "Arch Linux".into(),
            uptime: "12d 4h".into(),
            ..Default::default()
        },
        cpu: CpuInfo {
            percent: 42.0,
            per_core: vec![
                38.0, 24.0, 55.0, 12.0, 63.0, 42.0, 18.0, 35.0, 28.0, 34.0, 19.0, 48.0, 30.0, 72.0,
                54.0, 42.0,
            ],
            model: "AMD Ryzen 7 5800X".into(),
            cores: 8,
            threads: 16,
        },
        memory: MemoryInfo {
            total_bytes: 32 * gib,
            used_bytes: 14 * gib,
            percent: 43.75,
            cached_bytes: 6 * gib,
            buffers_bytes: 256 * 1024 * 1024,
        },
        swap: MemoryInfo {
            total_bytes: 4 * gib,
            used_bytes: 0,
            percent: 0.0,
            ..Default::default()
        },
        network: NetworkInfo {
            recv_bytes_per_sec: 245_760,
            sent_bytes_per_sec: 65_536,
        },
        disks: vec![DiskInfo {
            mountpoint: "/".into(),
            device: "/dev/nvme0n1p2".into(),
            filesystem: "ext4".into(),
            total_bytes: 512 * gib,
            used_bytes: 224 * gib,
            percent: 43.75,
        }],
        top_processes: (0..32)
            .map(|i| {
                let names = [
                    "redthread",
                    "postgres",
                    "crabitat",
                    "caddy",
                    "codex",
                    "fish",
                    "syncthing",
                    "durandal",
                ];
                ProcessInfo {
                    pid: 1200 + i,
                    name: names[i as usize % names.len()].into(),
                    command: format!(
                        "{} --serve --config /etc/service.toml",
                        names[i as usize % names.len()]
                    ),
                    cpu_percent: (32 - i) as f64 * 1.5,
                    memory_percent: (32 - i) as f64 / 8.0,
                    rss_bytes: (32 - i) as u64 * 16 * 1024 * 1024,
                    status: "S".into(),
                    user: "lapis".into(),
                }
            })
            .collect(),
        docker: DockerInfo {
            available: true,
            containers: vec![
                ContainerInfo {
                    id: "abc123".into(),
                    name: "db".into(),
                    image: "postgres:16".into(),
                    status: "Up 12 days".into(),
                    state: "running".into(),
                    running: true,
                    ..Default::default()
                },
                ContainerInfo {
                    id: "def456".into(),
                    name: "db-worker".into(),
                    image: "redis:7".into(),
                    status: "Up 12 days".into(),
                    state: "running".into(),
                    running: true,
                    ..Default::default()
                },
                ContainerInfo {
                    id: "xyz789".into(),
                    name: "backup".into(),
                    image: "alpine:3".into(),
                    status: "Exited (0) 2h ago".into(),
                    state: "exited".into(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Explicit demo data for reproducible Go/Rust visual comparison, never used by the live UI.
pub const CPU_HISTORY: [u16; 48] = [
    6, 9, 14, 18, 12, 8, 6, 7, 5, 9, 18, 37, 72, 95, 66, 42, 28, 16, 12, 15, 19, 25, 31, 24, 18,
    14, 12, 20, 44, 67, 81, 55, 33, 22, 15, 11, 8, 13, 21, 30, 24, 18, 15, 22, 36, 58, 51, 42,
];
pub fn history() -> std::collections::VecDeque<crate::tui::TelemetryPoint> {
    CPU_HISTORY
        .into_iter()
        .enumerate()
        .map(|(i, cpu)| crate::tui::TelemetryPoint {
            cpu_basis_points: cpu * 100,
            network: Some(if i == 47 {
                (245_760, 65_536)
            } else {
                (
                    u64::from(cpu) * 4096,
                    u64::from(CPU_HISTORY[(i + 13) % 48]) * 1024,
                )
            }),
        })
        .collect()
}

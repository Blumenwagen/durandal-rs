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

use crate::{
    metrics::*,
    ops::{Report, Status},
};
use chrono::Utc;
use serde::Serialize;

pub enum SchemaVersion {
    V1,
}
impl SchemaVersion {
    pub fn as_str(&self) -> &'static str {
        "durandal.agent.v1"
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Payload {
    pub schema: String,
    pub generated_at: String,
    pub host: HostSummary,
    pub health: HealthSummary,
    pub resources: ResourceSummary,
    pub top_processes: Vec<ProcessSummary>,
    pub docker: DockerSummary,
    pub agent_short_text: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct HostSummary {
    pub hostname: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub os: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub kernel: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub arch: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub uptime: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct HealthSummary {
    pub score: i32,
    pub status: String,
    pub alerts: Vec<AlertSummary>,
    pub recommendations: Vec<String>,
}
#[derive(Debug, Clone, Serialize)]
pub struct AlertSummary {
    pub severity: String,
    pub label: String,
    pub message: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct ResourceSummary {
    pub cpu: CpuSummary,
    pub memory: MemorySummary,
    pub swap: SwapSummary,
    pub network: NetworkSummary,
    pub disks: Vec<DiskSummary>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub gpus: Vec<GpuSummary>,
}
#[derive(Debug, Clone, Serialize)]
pub struct CpuSummary {
    pub percent: f64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub cores: usize,
    #[serde(skip_serializing_if = "is_zero_usize")]
    pub threads: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct MemorySummary {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub percent: f64,
    #[serde(rename = "cached_bytes", skip_serializing_if = "is_zero_u64")]
    pub cached: u64,
    #[serde(rename = "buffers_bytes", skip_serializing_if = "is_zero_u64")]
    pub buffers: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct SwapSummary {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub percent: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct NetworkSummary {
    pub recv_bytes_per_sec: u64,
    pub sent_bytes_per_sec: u64,
}
#[derive(Debug, Clone, Serialize)]
pub struct DiskSummary {
    pub mountpoint: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub device: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub filesystem: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub percent: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct GpuSummary {
    pub name: String,
    pub utilization_percent: f64,
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub memory_used_mb: u64,
    #[serde(skip_serializing_if = "is_zero_u64")]
    pub memory_total_mb: u64,
    #[serde(skip_serializing_if = "is_zero_f64")]
    pub temperature_c: f64,
}
#[derive(Debug, Clone, Serialize)]
pub struct ProcessSummary {
    pub pid: i32,
    pub name: String,
    pub cpu_percent: f64,
    pub memory_percent: f64,
    pub rss_bytes: u64,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub user: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub command: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct DockerSummary {
    pub available: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    pub total: usize,
    pub running: usize,
    pub stopped: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub containers: Vec<ContainerSummary>,
}
#[derive(Debug, Clone, Serialize)]
pub struct ContainerSummary {
    pub name: String,
    pub image: String,
    pub state: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub status: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub ports: String,
    pub running: bool,
}

pub fn build_payload(snapshot: &Snapshot, report: &Report, top_processes: usize) -> Payload {
    let mut payload = Payload {
        schema: SchemaVersion::V1.as_str().into(),
        generated_at: Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
        host: HostSummary {
            hostname: snapshot.host.hostname.clone(),
            user: snapshot.host.user.clone(),
            os: snapshot.host.os.clone(),
            kernel: snapshot.host.kernel.clone(),
            arch: snapshot.host.arch.clone(),
            uptime: snapshot.host.uptime.clone(),
        },
        health: HealthSummary {
            score: report.score,
            status: report.status.as_str().into(),
            alerts: report
                .alerts
                .iter()
                .map(|a| AlertSummary {
                    severity: a.severity.as_str().into(),
                    label: a.label.clone(),
                    message: a.message.clone(),
                })
                .collect(),
            recommendations: recommendations(snapshot, report),
        },
        resources: ResourceSummary {
            cpu: CpuSummary {
                percent: snapshot.cpu.percent,
                model: snapshot.cpu.model.clone(),
                cores: snapshot.cpu.cores,
                threads: snapshot.cpu.threads,
            },
            memory: MemorySummary {
                total_bytes: snapshot.memory.total_bytes,
                used_bytes: snapshot.memory.used_bytes,
                percent: snapshot.memory.percent,
                cached: snapshot.memory.cached_bytes,
                buffers: snapshot.memory.buffers_bytes,
            },
            swap: SwapSummary {
                total_bytes: snapshot.swap.total_bytes,
                used_bytes: snapshot.swap.used_bytes,
                percent: snapshot.swap.percent,
            },
            network: NetworkSummary {
                recv_bytes_per_sec: snapshot.network.recv_bytes_per_sec,
                sent_bytes_per_sec: snapshot.network.sent_bytes_per_sec,
            },
            disks: snapshot
                .disks
                .iter()
                .map(|d| DiskSummary {
                    mountpoint: d.mountpoint.clone(),
                    device: d.device.clone(),
                    filesystem: d.filesystem.clone(),
                    total_bytes: d.total_bytes,
                    used_bytes: d.used_bytes,
                    percent: d.percent,
                })
                .collect(),
            gpus: snapshot
                .gpus
                .iter()
                .map(|g| GpuSummary {
                    name: g.name.clone(),
                    utilization_percent: g.utilization_percent,
                    memory_used_mb: g.memory_used_mb,
                    memory_total_mb: g.memory_total_mb,
                    temperature_c: g.temperature_c,
                })
                .collect(),
        },
        top_processes: snapshot
            .top_processes
            .iter()
            .take(top_processes.max(1))
            .map(|p| ProcessSummary {
                pid: p.pid,
                name: p.name.clone(),
                cpu_percent: p.cpu_percent,
                memory_percent: p.memory_percent,
                rss_bytes: p.rss_bytes,
                status: p.status.clone(),
                user: p.user.clone(),
                command: p.command.clone(),
            })
            .collect(),
        docker: summarize_docker(&snapshot.docker),
        agent_short_text: String::new(),
    };
    payload.agent_short_text = short_text(&payload);
    payload
}

pub fn marshal_json(payload: &Payload, pretty: bool) -> anyhow::Result<String> {
    Ok(if pretty {
        serde_json::to_string_pretty(payload)?
    } else {
        serde_json::to_string(payload)?
    })
}

pub fn marshal_prometheus(payload: &Payload) -> String {
    let host = payload.host.hostname.as_str();
    let mut out = String::new();
    metric(
        &mut out,
        "durandal_health_score",
        "Sentinel health score from 0 to 100.",
        &format!(
            "{{host=\"{}\",status=\"{}\"}}",
            esc(host),
            esc(&payload.health.status)
        ),
        &payload.health.score.to_string(),
    );
    metric(
        &mut out,
        "durandal_cpu_usage_percent",
        "Current total CPU usage percent.",
        &format!("{{host=\"{}\"}}", esc(host)),
        &payload.resources.cpu.percent.to_string(),
    );
    metric(
        &mut out,
        "durandal_memory_usage_percent",
        "Current RAM usage percent.",
        &format!("{{host=\"{}\"}}", esc(host)),
        &payload.resources.memory.percent.to_string(),
    );
    metric(
        &mut out,
        "durandal_swap_usage_percent",
        "Current swap usage percent.",
        &format!("{{host=\"{}\"}}", esc(host)),
        &payload.resources.swap.percent.to_string(),
    );
    metric(
        &mut out,
        "durandal_network_recv_bytes_per_second",
        "Current network receive throughput in bytes per second.",
        &format!("{{host=\"{}\"}}", esc(host)),
        &payload.resources.network.recv_bytes_per_sec.to_string(),
    );
    metric(
        &mut out,
        "durandal_network_sent_bytes_per_second",
        "Current network send throughput in bytes per second.",
        &format!("{{host=\"{}\"}}", esc(host)),
        &payload.resources.network.sent_bytes_per_sec.to_string(),
    );
    out.push_str("# HELP durandal_docker_available Whether Docker data was collected successfully: 1 available, 0 unavailable.\n# TYPE durandal_docker_available gauge\n");
    out.push_str(&format!(
        "durandal_docker_available{{host=\"{}\"}} {}\n",
        esc(host),
        if payload.docker.available { 1 } else { 0 }
    ));
    out.push_str("# HELP durandal_docker_containers_total Total Docker containers seen by Durandal.\n# TYPE durandal_docker_containers_total gauge\n");
    out.push_str(&format!(
        "durandal_docker_containers_total{{host=\"{}\"}} {}\n",
        esc(host),
        payload.docker.total
    ));
    out.push_str("# HELP durandal_docker_containers_running Running Docker containers seen by Durandal.\n# TYPE durandal_docker_containers_running gauge\n");
    out.push_str(&format!(
        "durandal_docker_containers_running{{host=\"{}\"}} {}\n",
        esc(host),
        payload.docker.running
    ));
    out.push_str("# HELP durandal_docker_containers_stopped Stopped Docker containers seen by Durandal.\n# TYPE durandal_docker_containers_stopped gauge\n");
    out.push_str(&format!(
        "durandal_docker_containers_stopped{{host=\"{}\"}} {}\n",
        esc(host),
        payload.docker.stopped
    ));
    out.push_str("# EOF\n");
    out
}

fn metric(out: &mut String, name: &str, help: &str, labels: &str, value: &str) {
    out.push_str(&format!(
        "# HELP {name} {help}\n# TYPE {name} gauge\n{name}{labels} {value}\n"
    ));
}
fn summarize_docker(info: &DockerInfo) -> DockerSummary {
    let mut running = 0;
    let mut containers = Vec::new();
    for c in &info.containers {
        if c.running {
            running += 1;
        }
        containers.push(ContainerSummary {
            name: c.name.clone(),
            image: c.image.clone(),
            state: c.state.clone(),
            status: c.status.clone(),
            ports: c.ports.clone(),
            running: c.running,
        });
    }
    DockerSummary {
        available: info.available,
        error: info.error.clone(),
        total: info.containers.len(),
        running,
        stopped: info.containers.len().saturating_sub(running),
        containers,
    }
}
fn recommendations(snapshot: &Snapshot, report: &Report) -> Vec<String> {
    let mut recs = Vec::new();
    if report.status == Status::Clear {
        recs.push("Host is nominal; no immediate action required.".into());
    }
    if snapshot.cpu.percent >= 75.0 {
        recs.push(
            "CPU pressure is high; inspect top CPU processes before starting more heavy work."
                .into(),
        );
    }
    if snapshot.memory.percent >= 78.0 {
        recs.push("RAM pressure is high; check memory-heavy processes and consider stopping nonessential services.".into());
    }
    if snapshot.swap.percent >= 20.0 {
        recs.push("Swap is active; reduce memory pressure before latency-sensitive tasks.".into());
    }
    for disk in &snapshot.disks {
        if disk.percent >= 85.0 {
            recs.push(format!(
                "Disk {} is {:.0}% full; clean logs, caches, or old artifacts before large writes.",
                disk.mountpoint, disk.percent
            ));
        }
    }
    if snapshot.docker.available {
        let stopped = snapshot
            .docker
            .containers
            .iter()
            .filter(|c| !c.running)
            .count();
        if stopped > 0 {
            recs.push(format!("Docker has {stopped} stopped container(s); prune or restart intentionally if they matter."));
        }
    }
    if recs.is_empty() {
        recs.push("Review sentinel alerts first; they are sorted by urgency.".into());
    }
    recs
}
fn short_text(payload: &Payload) -> String {
    let mut parts = vec![
        format!("{} score {}", payload.health.status, payload.health.score),
        format!("CPU {:.0}%", payload.resources.cpu.percent),
        format!("RAM {:.0}%", payload.resources.memory.percent),
    ];
    if let Some(worst) = payload.resources.disks.iter().max_by(|a, b| {
        a.percent
            .partial_cmp(&b.percent)
            .unwrap_or(std::cmp::Ordering::Equal)
    }) {
        parts.push(format!("disk {} {:.0}%", worst.mountpoint, worst.percent));
    }
    if payload.docker.available {
        parts.push(format!(
            "docker {}/{} running",
            payload.docker.running, payload.docker.total
        ));
    }
    if let Some(alert) = payload.health.alerts.first() {
        parts.push(format!("top alert: {}", alert.message));
    }
    parts.join(" · ")
}
fn esc(value: &str) -> String {
    value
        .replace('\\', r"\\")
        .replace('\n', r"\n")
        .replace('"', r#"\""#)
}
fn is_zero_usize(v: &usize) -> bool {
    *v == 0
}
fn is_zero_u64(v: &u64) -> bool {
    *v == 0
}
fn is_zero_f64(v: &f64) -> bool {
    *v == 0.0
}

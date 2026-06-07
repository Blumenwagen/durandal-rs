use crate::metrics::Snapshot;

pub const MAX_ALERTS: usize = 7;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Status {
    Clear,
    Watch,
    Warn,
    Crit,
}

impl Status {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Clear => "CLEAR",
            Self::Watch => "WATCH",
            Self::Warn => "WARN",
            Self::Crit => "CRIT",
        }
    }
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Clear => 0,
            Self::Watch => 1,
            Self::Warn => 2,
            Self::Crit => 3,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Watch,
    Warning,
    Critical,
}

impl Severity {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Watch => "watch",
            Self::Warning => "warning",
            Self::Critical => "critical",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Alert {
    pub severity: Severity,
    pub label: String,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Report {
    pub score: i32,
    pub status: Status,
    pub alerts: Vec<Alert>,
}

pub fn evaluate_snapshot(snapshot: &Snapshot) -> Report {
    let mut alerts: Vec<Alert> = Vec::with_capacity(MAX_ALERTS + 4);
    let mut score = 100;
    let mut max_severity = Severity::Info;
    let mut add = |severity: Severity, penalty: i32, label: &str, message: String| {
        if severity > max_severity {
            max_severity = severity;
        }
        score -= penalty;
        alerts.push(Alert {
            severity,
            label: label.to_string(),
            message,
        });
    };

    let cpu = snapshot.cpu.percent;
    if cpu >= 90.0 {
        add(
            Severity::Critical,
            22,
            "CPU",
            format!("CPU saturated at {:.0}%", cpu),
        );
    } else if cpu >= 75.0 {
        add(
            Severity::Warning,
            12,
            "CPU",
            format!("CPU pressure at {:.0}%", cpu),
        );
    } else if cpu >= 60.0 {
        add(
            Severity::Watch,
            6,
            "CPU",
            format!("CPU warming at {:.0}%", cpu),
        );
    }

    let ram = snapshot.memory.percent;
    if ram >= 90.0 {
        add(
            Severity::Critical,
            20,
            "RAM",
            format!("RAM tight at {:.0}%", ram),
        );
    } else if ram >= 78.0 {
        add(
            Severity::Warning,
            11,
            "RAM",
            format!("RAM pressure at {:.0}%", ram),
        );
    } else if ram >= 65.0 {
        add(
            Severity::Watch,
            5,
            "RAM",
            format!("RAM watch at {:.0}%", ram),
        );
    }

    let swap = snapshot.swap.percent;
    if swap >= 50.0 {
        add(
            Severity::Critical,
            18,
            "SWAP",
            format!("Swap heavy at {:.0}%", swap),
        );
    } else if swap >= 20.0 {
        add(
            Severity::Warning,
            9,
            "SWAP",
            format!("Swap in use at {:.0}%", swap),
        );
    } else if swap >= 5.0 {
        add(
            Severity::Watch,
            3,
            "SWAP",
            format!("Swap trace at {:.0}%", swap),
        );
    }

    for disk in &snapshot.disks {
        if disk.percent >= 95.0 {
            add(
                Severity::Critical,
                20,
                "DISK",
                format!(
                    "Disk {} almost full at {:.0}%",
                    disk.mountpoint, disk.percent
                ),
            );
        } else if disk.percent >= 85.0 {
            add(
                Severity::Warning,
                12,
                "DISK",
                format!("Disk {} high at {:.0}%", disk.mountpoint, disk.percent),
            );
        } else if disk.percent >= 75.0 {
            add(
                Severity::Watch,
                6,
                "DISK",
                format!("Disk {} watch at {:.0}%", disk.mountpoint, disk.percent),
            );
        }
    }

    let network = snapshot.network.recv_bytes_per_sec + snapshot.network.sent_bytes_per_sec;
    if network >= 200 * 1024 * 1024 {
        add(
            Severity::Critical,
            14,
            "NET",
            format!(
                "Network flood {:.0} MiB/s",
                network as f64 / 1024.0 / 1024.0
            ),
        );
    } else if network >= 80 * 1024 * 1024 {
        add(
            Severity::Warning,
            8,
            "NET",
            format!("Network busy {:.0} MiB/s", network as f64 / 1024.0 / 1024.0),
        );
    }

    for proc in &snapshot.top_processes {
        if proc.cpu_percent >= 120.0 || proc.memory_percent >= 30.0 {
            add(
                Severity::Critical,
                14,
                "PROC",
                format!(
                    "Process {} hot: {:.0}% CPU / {:.0}% MEM",
                    proc.name, proc.cpu_percent, proc.memory_percent
                ),
            );
        } else if proc.cpu_percent >= 70.0 || proc.memory_percent >= 18.0 {
            add(
                Severity::Warning,
                8,
                "PROC",
                format!(
                    "Process {} elevated: {:.0}% CPU / {:.0}% MEM",
                    proc.name, proc.cpu_percent, proc.memory_percent
                ),
            );
        }
    }

    alerts.sort_by(|a, b| {
        b.severity
            .cmp(&a.severity)
            .then_with(|| a.label.cmp(&b.label))
    });
    alerts.truncate(MAX_ALERTS);
    if alerts.is_empty() {
        alerts.push(Alert {
            severity: Severity::Info,
            label: "OK".into(),
            message: "Nominal — no pressure signatures".into(),
        });
    }
    score = score.max(0);
    Report {
        score,
        status: worst_status(status_for_score(score), status_for_severity(max_severity)),
        alerts,
    }
}

fn status_for_score(score: i32) -> Status {
    match score {
        90..=i32::MAX => Status::Clear,
        75..=89 => Status::Watch,
        50..=74 => Status::Warn,
        _ => Status::Crit,
    }
}
fn status_for_severity(severity: Severity) -> Status {
    match severity {
        Severity::Critical => Status::Crit,
        Severity::Warning => Status::Warn,
        Severity::Watch => Status::Watch,
        Severity::Info => Status::Clear,
    }
}
fn worst_status(a: Status, b: Status) -> Status {
    a.max(b)
}

pub fn parse_threshold_rank(value: &str) -> Option<Status> {
    match value.trim().to_ascii_lowercase().as_str() {
        "watch" | "1" => Some(Status::Watch),
        "warn" | "warning" | "2" => Some(Status::Warn),
        "crit" | "critical" | "3" => Some(Status::Crit),
        _ => None,
    }
}

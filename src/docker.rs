use crate::{
    config::Config,
    metrics::{ContainerInfo, DockerInfo},
};
use serde::Deserialize;
use std::{
    process::Command,
    time::{Duration, Instant},
};

#[derive(Debug, Deserialize)]
#[allow(non_snake_case)]
struct DockerRow {
    ID: String,
    Image: String,
    Names: String,
    Status: String,
    State: String,
    Ports: String,
}

pub fn collect_docker(timeout: Duration) -> DockerInfo {
    let start = Instant::now();
    let output = Command::new("docker")
        .args(["ps", "-a", "--format", "{{json .}}"])
        .output();
    if start.elapsed() > timeout {
        return DockerInfo::unavailable("docker query timed out");
    }
    let Ok(out) = output else {
        return DockerInfo::unavailable("docker CLI not found");
    };
    if !out.status.success() {
        return DockerInfo::unavailable(short_error(&out.stderr, &out.stdout));
    }
    let text = String::from_utf8_lossy(&out.stdout);
    let mut info = DockerInfo {
        available: true,
        ..Default::default()
    };
    for line in text.lines().filter(|line| !line.trim().is_empty()) {
        let Ok(raw) = serde_json::from_str::<DockerRow>(line) else {
            continue;
        };
        let running = raw.State.eq_ignore_ascii_case("running");
        info.containers.push(ContainerInfo {
            id: raw.ID,
            image: raw.Image,
            name: raw.Names,
            status: raw.Status,
            state: raw.State,
            ports: raw.Ports,
            running,
        });
    }
    info.containers
        .sort_by(|a, b| b.running.cmp(&a.running).then_with(|| a.name.cmp(&b.name)));
    info
}

pub fn docker_action(action: &str, id: &str) -> anyhow::Result<String> {
    anyhow::ensure!(
        matches!(action, "start" | "stop" | "restart"),
        "unsupported docker action {action}"
    );
    let output = Command::new("docker").args([action, id]).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        anyhow::bail!(short_error(&output.stderr, &output.stdout))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerAction {
    Start,
    Stop,
    Restart,
}

impl DockerAction {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DockerActionPlan {
    pub command: Vec<String>,
    pub warning: String,
}

pub fn docker_action_plan(
    config: &Config,
    action: DockerAction,
    container_id: &str,
    confirmed: bool,
) -> anyhow::Result<DockerActionPlan> {
    anyhow::ensure!(
        config.docker_controls_enabled(),
        "docker controls are hidden by default; set [docker].controls = \"enabled\" first"
    );
    anyhow::ensure!(
        confirmed,
        "docker {} requires explicit confirmation",
        action.as_str()
    );
    let container_id = container_id.trim();
    anyhow::ensure!(!container_id.is_empty(), "container id/name is required");
    Ok(DockerActionPlan {
        command: vec!["docker".into(), action.as_str().into(), container_id.into()],
        warning: format!(
            "Ready to {} Docker container {container_id}; this is intentionally gated.",
            action.as_str()
        ),
    })
}

fn short_error(stderr: &[u8], stdout: &[u8]) -> String {
    let msg = if stderr.is_empty() {
        String::from_utf8_lossy(stdout)
    } else {
        String::from_utf8_lossy(stderr)
    };
    msg.lines()
        .next()
        .unwrap_or("docker unavailable")
        .trim()
        .to_string()
}

use durandal_rs::config::{Config, DockerControlsMode};
use durandal_rs::docker::{DockerAction, docker_action_plan};
use durandal_rs::metrics::{
    ContainerInfo, CpuInfo, DiskInfo, DockerInfo, HostInfo, MemoryInfo, NetworkInfo, ProcessInfo,
    Snapshot,
};
use durandal_rs::ops::evaluate_snapshot;
use durandal_rs::tui::{TuiState, render_dashboard_text};

fn docker_snapshot() -> Snapshot {
    Snapshot {
        host: HostInfo {
            hostname: "durandal-host".into(),
            user: "lapis".into(),
            uptime: "5m".into(),
            ..Default::default()
        },
        cpu: CpuInfo {
            percent: 42.0,
            model: "Test CPU".into(),
            cores: 4,
            threads: 8,
        },
        memory: MemoryInfo {
            total_bytes: 16 * 1024 * 1024 * 1024,
            used_bytes: 7 * 1024 * 1024 * 1024,
            percent: 44.0,
            cached_bytes: 2 * 1024 * 1024 * 1024,
            buffers_bytes: 256 * 1024 * 1024,
        },
        swap: MemoryInfo {
            total_bytes: 4 * 1024 * 1024 * 1024,
            used_bytes: 512 * 1024 * 1024,
            percent: 12.0,
            ..Default::default()
        },
        network: NetworkInfo {
            recv_bytes_per_sec: 24 * 1024,
            sent_bytes_per_sec: 8 * 1024,
        },
        disks: vec![DiskInfo {
            mountpoint: "/".into(),
            device: "/dev/vda1".into(),
            filesystem: "ext4".into(),
            total_bytes: 80 * 1024 * 1024 * 1024,
            used_bytes: 40 * 1024 * 1024 * 1024,
            percent: 50.0,
        }],
        top_processes: vec![ProcessInfo {
            pid: 4242,
            name: "redthread".into(),
            cpu_percent: 12.5,
            memory_percent: 3.0,
            rss_bytes: 256 * 1024 * 1024,
            status: "S".into(),
            user: "lapis".into(),
            command: "redthread serve".into(),
        }],
        docker: DockerInfo {
            available: true,
            containers: vec![
                ContainerInfo {
                    id: "abc123".into(),
                    image: "postgres:16".into(),
                    name: "db".into(),
                    status: "Up 2 hours".into(),
                    state: "running".into(),
                    ports: "5432/tcp".into(),
                    running: true,
                },
                ContainerInfo {
                    id: "def456".into(),
                    image: "redis:7".into(),
                    name: "cache".into(),
                    status: "Exited (0) 1 hour ago".into(),
                    state: "exited".into(),
                    ports: "6379/tcp".into(),
                    running: false,
                },
            ],
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn config_matches_go_durandal_docker_station_defaults() {
    let config = Config::default();

    assert_eq!(config.docker.controls, DockerControlsMode::Enabled);
    assert!(config.docker.controls_enabled());
    assert!(config.docker.show_panel_by_default);
}

#[test]
fn config_parser_accepts_explicit_docker_control_opt_in() {
    let config = Config::from_text("[docker]\ncontrols = \"enabled\"\n").expect("valid config");

    assert_eq!(config.docker.controls, DockerControlsMode::Enabled);
    assert!(config.docker.controls_enabled());
}

#[test]
fn default_tui_shows_go_durandal_docker_station() {
    let snapshot = docker_snapshot();
    let report = evaluate_snapshot(&snapshot);
    let state = TuiState::from_config(&Config::default());

    let text = render_dashboard_text(&snapshot, &report, &state);

    assert!(text.contains("D O C K E R"));
    assert!(text.contains("2 CONTAINERS"));
    assert!(text.contains("1 RUNNING"));
    assert!(text.contains("IDLE"));
    assert!(!text.contains("DOCKER HIDDEN"));
    assert!(!text.contains("controls locked"));
    assert!(!text.contains("durandal-rs"));
    assert!(!text.contains("DURANDAL-RS"));
}

#[test]
fn dashboard_text_uses_go_durandal_brutalist_visual_identity() {
    let snapshot = docker_snapshot();
    let report = evaluate_snapshot(&snapshot);
    let state = TuiState {
        docker_panel_visible: true,
        ..Default::default()
    };

    let text = render_dashboard_text(&snapshot, &report, &state);

    assert!(text.contains("█ DURANDAL"));
    assert!(text.contains("SYSTEMS MONITOR"));
    assert!(text.contains("L A P I S   S E N T I N E L"));
    assert!(text.contains("OPS READINESS"));
    assert!(text.contains("C P U"));
    assert!(text.contains("M E M O R Y"));
    assert!(text.contains("N E T W O R K"));
    assert!(text.contains("S T O R A G E"));
    assert!(text.contains("P R O C E S S E S"));
    assert!(text.contains("D O C K E R"));
    assert!(text.contains("▎"));
    assert!(text.contains("█"));
    assert!(text.contains("░"));
    assert!(text.contains("CPU▼"));
    assert!(text.contains("▼ DOWN"));
    assert!(text.contains("▲ UP"));
    assert!(text.contains("↑/k"));
    assert!(text.contains("SORT"));
}

#[test]
fn tui_shows_go_style_docker_panel_and_controls_text() {
    let snapshot = docker_snapshot();
    let report = evaluate_snapshot(&snapshot);
    let state = TuiState {
        docker_panel_visible: true,
        docker_controls_enabled: true,
        ..Default::default()
    };

    let text = render_dashboard_text(&snapshot, &report, &state);

    assert!(text.contains("D O C K E R"));
    assert!(text.contains("2 CONTAINERS"));
    assert!(text.contains("1 RUNNING"));
    assert!(text.contains("IDLE"));
    assert!(text.contains("STATE"));
    assert!(text.contains("NAME"));
    assert!(text.contains("IMAGE"));
    assert!(text.contains("db"));
    assert!(text.contains("cache"));
    assert!(!text.contains("read-only"));
    assert!(!text.contains("controls locked"));
}

#[test]
fn tui_state_uses_go_key_semantics_for_sort_docker_focus_and_dim() {
    let mut state = TuiState::default();
    assert!(state.docker_panel_visible);
    assert!(state.sort_by_cpu);
    assert!(!state.docker_focused);
    assert!(!state.dimmed);

    state.toggle_sort();
    assert!(!state.sort_by_cpu);
    state.toggle_docker_focus();
    assert!(state.docker_focused);
    state.toggle_dimmed();
    assert!(state.dimmed);
    assert!(
        state.docker_panel_visible,
        "dimming must not hide Docker like the old Rust-only UI"
    );
}

#[test]
fn docker_action_plan_matches_go_confirmation_model() {
    let config = Config::default();

    assert!(docker_action_plan(&config, DockerAction::Restart, "abc123", false).is_err());

    let plan =
        docker_action_plan(&config, DockerAction::Restart, "abc123", true).expect("confirmed plan");
    assert_eq!(plan.command, vec!["docker", "restart", "abc123"]);
    assert!(plan.warning.contains("restart"));
}

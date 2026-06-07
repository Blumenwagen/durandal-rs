use durandal_rs::agent::{SchemaVersion, build_payload};
use durandal_rs::metrics::{CpuInfo, DiskInfo, DockerInfo, MemoryInfo, NetworkInfo, Snapshot};
use durandal_rs::ops::{Severity, Status, evaluate_snapshot};

fn nominal_snapshot() -> Snapshot {
    Snapshot {
        host: Default::default(),
        cpu: CpuInfo {
            percent: 12.0,
            ..Default::default()
        },
        memory: MemoryInfo {
            percent: 35.0,
            ..Default::default()
        },
        swap: MemoryInfo::default_swap(),
        network: NetworkInfo::default(),
        disks: vec![DiskInfo {
            mountpoint: "/".into(),
            device: "/dev/root".into(),
            filesystem: "ext4".into(),
            total_bytes: 100,
            used_bytes: 40,
            percent: 40.0,
        }],
        top_processes: vec![],
        docker: DockerInfo::unavailable("docker CLI not found"),
        gpus: vec![],
    }
}

#[test]
fn sentinel_reports_nominal_clear_state() {
    let report = evaluate_snapshot(&nominal_snapshot());

    assert_eq!(report.score, 100);
    assert_eq!(report.status, Status::Clear);
    assert_eq!(report.alerts[0].severity, Severity::Info);
    assert!(report.alerts[0].message.contains("Nominal"));
}

#[test]
fn sentinel_escalates_critical_resource_pressure_even_when_score_is_not_low() {
    let mut snapshot = nominal_snapshot();
    snapshot.cpu.percent = 92.0;

    let report = evaluate_snapshot(&snapshot);

    assert_eq!(report.status, Status::Crit);
    assert!(report.score < 100);
    assert_eq!(report.alerts[0].label, "CPU");
    assert_eq!(report.alerts[0].severity, Severity::Critical);
}

#[test]
fn agent_payload_preserves_durandal_schema_and_short_text() {
    let snapshot = nominal_snapshot();
    let report = evaluate_snapshot(&snapshot);
    let payload = build_payload(&snapshot, &report, 5);

    assert_eq!(payload.schema, SchemaVersion::V1.as_str());
    assert_eq!(payload.health.status, "CLEAR");
    assert!(payload.agent_short_text.contains("CLEAR score 100"));
    assert!(!payload.docker.available);
}

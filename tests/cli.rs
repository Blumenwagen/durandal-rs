use assert_cmd::Command;
use predicates::prelude::*;

#[test]
fn agent_command_emits_versioned_json() {
    let mut cmd = Command::cargo_bin("durandal-rs").expect("binary exists");

    cmd.args(["agent", "--json", "--top", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("\"schema\":\"durandal.agent.v1\""))
        .stdout(predicate::str::contains("\"agent_short_text\""));
}

#[test]
fn check_rejects_invalid_threshold_before_collecting() {
    let mut cmd = Command::cargo_bin("durandal-rs").expect("binary exists");

    cmd.args(["check", "--fail-on", "oops"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("invalid --fail-on"));
}

#[test]
fn prometheus_command_exposes_core_metrics() {
    let mut cmd = Command::cargo_bin("durandal-rs").expect("binary exists");

    cmd.args(["prometheus", "--top", "1"])
        .assert()
        .success()
        .stdout(predicate::str::contains("# HELP durandal_health_score"))
        .stdout(predicate::str::contains("durandal_docker_available"));
}

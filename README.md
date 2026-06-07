# Durandal RS

[![CI](https://github.com/Blumenwagen/durandal-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/Blumenwagen/durandal-rs/actions/workflows/ci.yml)

Rust rewrite of [Durandal](https://github.com/Blumenwagen/durandal): a Linux terminal system monitor with the same brutalist, magenta-tinted visual identity, plus agent-readable ops snapshots for scripts, dashboards, and autonomous maintenance loops.

Durandal RS is kept in its own repository so the original Go implementation can stay stable while the Rust port grows toward feature parity.

## Updated project info

- **Status:** ready for an initial public `0.1.0` repository push.
- **Scope:** Linux-focused TUI monitor, Sentinel health scoring, JSON snapshots, Prometheus/OpenMetrics output, and health-check exit codes.
- **Parity target:** visually match the Go Durandal dashboard while moving toward a stronger Rust core.
- **Current caveat:** this is not a full replacement for Go Durandal yet. Interactive Docker start/stop/restart execution is represented by gated Rust action-planning helpers, but the live TUI currently exposes focus/sort/dim navigation rather than container mutation hotkeys.
- **Verification used for this push:** `cargo fmt -- --check`, `cargo test`, `cargo clippy --all-targets --all-features -- -D warnings`, `cargo build --release`, and live CLI smoke tests.

## What works now

- `agent`, `snapshot`, and `json` commands emit versioned `durandal.agent.v1` JSON.
- `prometheus` emits scrape-ready OpenMetrics-style text for the current snapshot.
- `check` prints a compact Sentinel health line and exits with status-shaped codes.
- The Ratatui/Crossterm TUI renders:
  - `LAPIS SENTINEL` ops-readiness score and alert strip
  - CPU, memory, swap, network, storage, and process panels
  - Docker inventory panel when Docker is available
  - Go Durandal-inspired block/rule styling and control hints
- Docker collection is read-only by default at collection time (`docker ps -a --format '{{json .}}'`).
- Docker action helpers require explicit confirmation before producing a `docker start|stop|restart` plan.

## Install / run locally

Requirements:

- Linux
- Rust stable with Edition 2024 support (`rustc 1.85+`)
- Docker CLI is optional; Durandal RS still runs when Docker is missing or unavailable.

```bash
git clone https://github.com/Blumenwagen/durandal-rs.git
cd durandal-rs
cargo run
```

Build an optimized binary:

```bash
cargo build --release
./target/release/durandal-rs --help
```

## CLI usage

```bash
# Launch the interactive TUI.
cargo run

# Compact one-shot JSON payload for agents, scripts, cron jobs, or dashboards.
cargo run -- agent --json --pretty --top 8

# Aliases for the same JSON snapshot shape.
cargo run -- snapshot --pretty --top 8
cargo run -- json --pretty --top 8

# Prometheus/OpenMetrics scrape text.
cargo run -- prometheus --top 5

# Human-readable health check.
# Exit code: 0 clear/below threshold, 1 WATCH, 2 WARN, 3 CRIT.
cargo run -- check --fail-on warn

# Full JSON payload while preserving health-check exit semantics.
cargo run -- check --json --pretty --fail-on crit
```

`check --fail-on` accepts `watch`, `warn` / `warning`, `crit` / `critical`, or numeric aliases `1`-`3`.

## Agent JSON schema

Durandal RS emits schema `durandal.agent.v1`.

Top-level fields:

- `schema` and `generated_at`
- `host`: hostname, user, OS, kernel, architecture, uptime
- `health`: Sentinel score, status, alerts, recommendations
- `resources`: CPU, memory, swap, network, disks, optional GPUs
- `top_processes`: bounded process list, controlled by `--top`
- `docker`: availability, counts, and container summaries
- `agent_short_text`: compact human-readable summary for logs or chat alerts

Example shape:

```json
{
  "schema": "durandal.agent.v1",
  "health": {
    "score": 100,
    "status": "CLEAR",
    "alerts": [
      {
        "severity": "info",
        "label": "OK",
        "message": "Nominal — no pressure signatures"
      }
    ]
  },
  "agent_short_text": "CLEAR score 100 · CPU 0% · RAM 32% · disk / 72% · docker 4/5 running · top alert: Nominal — no pressure signatures"
}
```

## Prometheus / OpenMetrics output

`durandal-rs prometheus` includes core gauges such as:

- `durandal_health_score`
- `durandal_cpu_usage_percent`
- `durandal_memory_usage_percent`
- `durandal_swap_usage_percent`
- `durandal_network_recv_bytes_per_second`
- `durandal_network_sent_bytes_per_second`
- `durandal_docker_available`
- `durandal_docker_containers_total`
- `durandal_docker_containers_running`
- `durandal_docker_containers_stopped`

Use `--top N` to cap process-cardinality surfaces where applicable.

## TUI controls

- `q` or `Esc`: quit
- `c`: toggle Docker focus styling
- `s` or `Tab`: toggle process sort mode
- `d`: toggle dimmed visual mode

The TUI is intentionally conservative about mutating the host. Docker inventory is collected from the local Docker CLI, but live container start/stop/restart hotkeys are still a parity gap for a later milestone.

## Development / quality gates

```bash
cargo fmt -- --check
cargo test
cargo clippy --all-targets --all-features -- -D warnings
cargo build --release
```

The CI workflow runs the same formatting, lint, and test gates on GitHub.

## Repository layout

- `src/metrics.rs`: host, resource, process, Docker, and GPU snapshot collection
- `src/ops.rs`: Sentinel score/status/alert evaluation
- `src/agent.rs`: JSON and Prometheus serialization
- `src/tui.rs`: Ratatui dashboard rendering and key handling
- `src/docker.rs`: Docker collection and gated action planning
- `tests/`: CLI, Sentinel, parity, and next-phase behavior tests

## Roadmap

- Wire guarded Docker start/stop/restart interactions into the live TUI.
- Add configuration-file discovery and documentation once config loading is user-facing.
- Expand parity tests against snapshots from Go Durandal.
- Add screenshots or terminal recordings once the Rust UI stabilizes visually.
- Package release artifacts after the first repository push and CI pass.

## License

MIT. See [LICENSE](LICENSE).

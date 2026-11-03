//! Deterministic native-widget rendering. All values are demo fixtures, never host telemetry.
mod support;
use clap::Parser;
use durandal_rs::{
    metrics, ops,
    tui::{TuiState, draw_dashboard},
};
use ratatui::{Terminal, backend::TestBackend};
use std::time::Instant;

#[derive(Parser)]
struct Args {
    #[arg(long, default_value_t = 120)]
    width: u16,
    #[arg(long, default_value_t = 36)]
    height: u16,
    #[arg(long, default_value = "overview", value_parser=["overview","docker","search","inspector","help","confirm","offline","alerts"])]
    mode: String,
    #[arg(long)]
    cells: bool,
}
fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let mut snapshot = support::snapshot();
    let mut state = TuiState {
        last_updated: Some(Instant::now()),
        process_cpu_available: true,
        network_rates_available: true,
        ..Default::default()
    };
    match args.mode.as_str() {
        "docker" => state.docker_focused = true,
        "search" => {
            state.filtering = true;
            state.filter = "crabitat".into();
        }
        "inspector" => state.inspector_open = true,
        "help" => state.help_open = true,
        "alerts" => state.alerts_open = true,
        "confirm" => state.process_kill_confirm = true,
        "offline" => {
            snapshot.docker =
                durandal_rs::metrics::DockerInfo::unavailable("Docker daemon is not running")
        }
        _ => {}
    }
    let report = ops::evaluate_snapshot(&snapshot);
    let mut terminal = Terminal::new(TestBackend::new(args.width, args.height))?;
    terminal.draw(|frame| draw_dashboard(frame, &snapshot, &report, &mut state))?;
    let buffer = terminal.backend().buffer();
    if args.cells {
        let cells: Vec<_> = buffer.content.iter().map(|c| serde_json::json!({"text":c.symbol(),"fg":format!("{:?}",c.fg),"bg":format!("{:?}",c.bg)})).collect();
        println!(
            "{}",
            serde_json::json!({"width":args.width,"height":args.height,"cells":cells})
        );
    } else {
        println!(
            "{}",
            durandal_rs::tui::render_dashboard_text_size(
                &snapshot,
                &report,
                &state,
                args.width,
                args.height
            )
        );
    }
    Ok(())
}

use crate::{agent, metrics, ops};
use clap::{Parser, Subcommand};
use std::io::Write;

#[derive(Parser)]
#[command(
    name = "durandal-rs",
    about = "Durandal rewritten in Rust: TUI system monitor plus agent-readable ops snapshots"
)]
pub struct Args {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    Agent(JsonArgs),
    Snapshot(JsonArgs),
    Json(JsonArgs),
    Prometheus(TopArgs),
    Check(CheckArgs),
}

#[derive(Parser, Clone)]
struct JsonArgs {
    #[arg(long, default_value_t = true)]
    json: bool,
    #[arg(long, default_value_t = false)]
    pretty: bool,
    #[arg(long, default_value_t = 8)]
    top: usize,
}
#[derive(Parser, Clone)]
struct TopArgs {
    #[arg(long, default_value_t = 8)]
    top: usize,
}
#[derive(Parser, Clone)]
struct CheckArgs {
    #[arg(long, default_value = "crit")]
    fail_on: String,
    #[arg(long, default_value_t = false)]
    json: bool,
    #[arg(long, default_value_t = false)]
    pretty: bool,
    #[arg(long, default_value_t = 5)]
    top: usize,
}

pub fn run() -> i32 {
    match try_run(std::env::args_os()) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("durandal-rs: {err}");
            1
        }
    }
}

pub fn try_run<I, T>(args: I) -> anyhow::Result<i32>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString> + Clone,
{
    let args = Args::parse_from(args);
    match args.command {
        None => crate::tui::run_tui().map(|_| 0),
        Some(Command::Agent(args) | Command::Snapshot(args) | Command::Json(args)) => {
            run_json(args)
        }
        Some(Command::Prometheus(args)) => run_prometheus(args),
        Some(Command::Check(args)) => run_check(args),
    }
}

fn run_json(args: JsonArgs) -> anyhow::Result<i32> {
    anyhow::ensure!(
        args.top > 0,
        "invalid --top {}; must be greater than zero",
        args.top
    );
    anyhow::ensure!(args.json, "durandal-rs agent currently emits JSON only");
    let snapshot = metrics::collect_snapshot(args.top)?;
    let report = ops::evaluate_snapshot(&snapshot);
    let payload = agent::build_payload(&snapshot, &report, args.top);
    println!("{}", agent::marshal_json(&payload, args.pretty)?);
    Ok(0)
}
fn run_prometheus(args: TopArgs) -> anyhow::Result<i32> {
    anyhow::ensure!(
        args.top > 0,
        "invalid --top {}; must be greater than zero",
        args.top
    );
    let snapshot = metrics::collect_snapshot(args.top)?;
    let report = ops::evaluate_snapshot(&snapshot);
    let payload = agent::build_payload(&snapshot, &report, args.top);
    print!("{}", agent::marshal_prometheus(&payload));
    std::io::stdout().flush().ok();
    Ok(0)
}
fn run_check(args: CheckArgs) -> anyhow::Result<i32> {
    let Some(threshold) = ops::parse_threshold_rank(&args.fail_on) else {
        anyhow::bail!(
            "invalid --fail-on {:?}; use watch, warn/warning, crit/critical, or 1-3",
            args.fail_on
        );
    };
    anyhow::ensure!(
        args.top > 0,
        "invalid --top {}; must be greater than zero",
        args.top
    );
    let snapshot = metrics::collect_snapshot(args.top)?;
    let report = ops::evaluate_snapshot(&snapshot);
    let payload = agent::build_payload(&snapshot, &report, args.top);
    if args.json {
        println!("{}", agent::marshal_json(&payload, args.pretty)?);
    } else {
        println!("{}", payload.agent_short_text);
        for rec in &payload.health.recommendations {
            println!("- {rec}");
        }
    }
    if report.status >= threshold {
        Ok(report.status.exit_code())
    } else {
        Ok(0)
    }
}

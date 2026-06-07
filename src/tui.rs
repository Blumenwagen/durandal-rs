use crate::{agent, config::Config, docker, metrics, ops};
use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    style::{Color, Modifier, Style},
    text::{Line, Span, Text},
    widgets::{Paragraph, Wrap},
};
use std::{
    io,
    process::Command,
    time::{Duration, Instant},
};

const DEFAULT_WIDTH: usize = 120;
const DEFAULT_HEIGHT: usize = 36;

const NEON_LIME: Color = Color::Rgb(191, 255, 0);
const HOT_PINK: Color = Color::Rgb(255, 0, 127);
const CYAN: Color = Color::Rgb(0, 240, 255);
const DEEP_BLACK: Color = Color::Rgb(10, 10, 10);
const DARK_NAVY: Color = Color::Rgb(18, 18, 31);
const MUTED_GREY: Color = Color::Rgb(138, 138, 158);
const DIM_GREY: Color = Color::Rgb(74, 74, 90);
const OFF_WHITE: Color = Color::Rgb(216, 216, 232);
const AMBER: Color = Color::Rgb(255, 184, 0);
const RED: Color = Color::Rgb(255, 34, 68);
const DIMMED_LIME: Color = Color::Rgb(111, 143, 0);
const DIMMED_PINK: Color = Color::Rgb(143, 0, 63);
const DIMMED_CYAN: Color = Color::Rgb(0, 127, 143);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiState {
    pub docker_panel_visible: bool,
    pub docker_controls_enabled: bool,
    pub docker_focused: bool,
    pub sort_by_cpu: bool,
    pub dimmed: bool,
    pub process_cursor: usize,
    pub process_offset: usize,
    pub docker_cursor: usize,
    pub docker_offset: usize,
    pub filtering: bool,
    pub filter: String,
    pub inspector_open: bool,
    pub docker_confirm: Option<DockerConfirm>,
    pub process_kill_confirm: bool,
    pub action_message: Option<ActionMessage>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerConfirm {
    Start,
    Stop,
    Restart,
}

impl DockerConfirm {
    fn as_action(self) -> docker::DockerAction {
        match self {
            Self::Start => docker::DockerAction::Start,
            Self::Stop => docker::DockerAction::Stop,
            Self::Restart => docker::DockerAction::Restart,
        }
    }

    fn as_str(self) -> &'static str {
        self.as_action().as_str()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActionMessage {
    pub text: String,
    pub ok: bool,
}

impl Default for TuiState {
    fn default() -> Self {
        Self {
            docker_panel_visible: true,
            docker_controls_enabled: true,
            docker_focused: false,
            sort_by_cpu: true,
            dimmed: false,
            process_cursor: 0,
            process_offset: 0,
            docker_cursor: 0,
            docker_offset: 0,
            filtering: false,
            filter: String::new(),
            inspector_open: false,
            docker_confirm: None,
            process_kill_confirm: false,
            action_message: None,
        }
    }
}

impl TuiState {
    pub fn from_config(config: &Config) -> Self {
        Self {
            docker_panel_visible: config.docker.show_panel_by_default,
            docker_controls_enabled: config.docker.controls_enabled(),
            ..Self::default()
        }
    }

    pub fn toggle_docker_focus(&mut self) {
        self.docker_focused = !self.docker_focused;
    }

    pub fn toggle_sort(&mut self) {
        self.sort_by_cpu = !self.sort_by_cpu;
        self.process_cursor = 0;
        self.process_offset = 0;
    }

    pub fn toggle_dimmed(&mut self) {
        self.dimmed = !self.dimmed;
    }
}

pub fn run_tui() -> anyhow::Result<()> {
    run_tui_with_config(Config::default())
}

pub fn run_tui_with_config(config: Config) -> anyhow::Result<()> {
    enable_raw_mode().context("enable raw mode")?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen).context("enter alternate screen")?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    let result = run_loop(&mut terminal, TuiState::from_config(&config));
    disable_raw_mode().ok();
    execute!(terminal.backend_mut(), LeaveAlternateScreen).ok();
    terminal.show_cursor().ok();
    result
}

fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut state: TuiState,
) -> anyhow::Result<()> {
    let mut snapshot = metrics::collect_snapshot(50).unwrap_or_default();
    let mut last_tick = Instant::now();
    loop {
        let report = ops::evaluate_snapshot(&snapshot);
        terminal.draw(|frame| draw(frame, &snapshot, &report, &state))?;
        if event::poll(Duration::from_millis(180))?
            && let Event::Key(key) = event::read()?
        {
            if handle_key(key, &mut state, &snapshot)? {
                break;
            }
        }
        if last_tick.elapsed() > Duration::from_secs(1) {
            snapshot = metrics::collect_snapshot(50).unwrap_or_default();
            last_tick = Instant::now();
        }
    }
    Ok(())
}

fn handle_key(
    key: KeyEvent,
    state: &mut TuiState,
    snapshot: &metrics::Snapshot,
) -> anyhow::Result<bool> {
    if state.filtering {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => state.filtering = false,
            KeyCode::Backspace => {
                state.filter.pop();
                state.process_cursor = 0;
                state.process_offset = 0;
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                state.filter.push(ch);
                state.process_cursor = 0;
                state.process_offset = 0;
            }
            _ => {}
        }
        return Ok(false);
    }

    if state.process_kill_confirm {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => confirm_process_kill(state, snapshot),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Char('q') => {
                state.process_kill_confirm = false;
            }
            _ => {}
        }
        return Ok(false);
    }

    if state.docker_confirm.is_some() {
        match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => confirm_docker_action(state, snapshot)?,
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc => state.docker_confirm = None,
            KeyCode::Char('q') => return Ok(true),
            _ => {}
        }
        return Ok(false);
    }

    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Esc => {
            if state.inspector_open {
                state.inspector_open = false;
            } else {
                state.docker_focused = false;
            }
        }
        KeyCode::Char('c') => state.toggle_docker_focus(),
        KeyCode::Char('s') | KeyCode::Tab if !state.docker_focused => state.toggle_sort(),
        KeyCode::Char('d') => state.toggle_dimmed(),
        KeyCode::Char('/') if !state.docker_focused => {
            state.filtering = true;
            state.filter.clear();
            state.process_cursor = 0;
            state.process_offset = 0;
        }
        KeyCode::Enter if !state.docker_focused => {
            state.inspector_open = !state.inspector_open;
        }
        KeyCode::Char('K') if !state.docker_focused => {
            if selected_process(snapshot, state).is_some() {
                state.process_kill_confirm = true;
                state.action_message = None;
            }
        }
        KeyCode::Char('x') if state.docker_focused => request_docker_toggle(state, snapshot),
        KeyCode::Char('r') if state.docker_focused => request_docker_restart(state, snapshot),
        KeyCode::Down | KeyCode::Char('j') => move_cursor(state, snapshot, 1),
        KeyCode::Up | KeyCode::Char('k') => move_cursor(state, snapshot, -1),
        _ => {}
    }

    Ok(false)
}

fn move_cursor(state: &mut TuiState, snapshot: &metrics::Snapshot, delta: isize) {
    if state.docker_focused {
        let len = snapshot.docker.containers.len();
        state.docker_cursor = moved_index(state.docker_cursor, len, delta);
    } else {
        let len = filtered_sorted_processes(snapshot, state).len();
        state.process_cursor = moved_index(state.process_cursor, len, delta);
    }
}

fn moved_index(current: usize, len: usize, delta: isize) -> usize {
    if len == 0 {
        return 0;
    }
    current
        .saturating_add_signed(delta)
        .min(len.saturating_sub(1))
}

fn request_docker_toggle(state: &mut TuiState, snapshot: &metrics::Snapshot) {
    if !state.docker_controls_enabled || !snapshot.docker.available {
        return;
    }
    let Some(container) = selected_container(snapshot, state) else {
        return;
    };
    state.docker_confirm = Some(if container.running {
        DockerConfirm::Stop
    } else {
        DockerConfirm::Start
    });
    state.action_message = None;
}

fn request_docker_restart(state: &mut TuiState, snapshot: &metrics::Snapshot) {
    if !state.docker_controls_enabled || !snapshot.docker.available {
        return;
    }
    let Some(container) = selected_container(snapshot, state) else {
        return;
    };
    if container.running {
        state.docker_confirm = Some(DockerConfirm::Restart);
        state.action_message = None;
    }
}

fn confirm_docker_action(state: &mut TuiState, snapshot: &metrics::Snapshot) -> anyhow::Result<()> {
    let Some(confirm) = state.docker_confirm else {
        return Ok(());
    };
    let Some(container) = selected_container(snapshot, state) else {
        state.docker_confirm = None;
        return Ok(());
    };
    match docker::docker_action(confirm.as_str(), &container.id) {
        Ok(_) => {
            state.action_message = Some(ActionMessage {
                text: format!("OK {} {}", confirm.as_str().to_uppercase(), container.name),
                ok: true,
            });
        }
        Err(err) => {
            state.action_message = Some(ActionMessage {
                text: format!(
                    "x {} {}: {err}",
                    confirm.as_str().to_uppercase(),
                    container.name
                ),
                ok: false,
            });
        }
    }
    state.docker_confirm = None;
    Ok(())
}

fn confirm_process_kill(state: &mut TuiState, snapshot: &metrics::Snapshot) {
    let Some(proc) = selected_process(snapshot, state) else {
        state.process_kill_confirm = false;
        return;
    };
    let result = Command::new("kill").arg(proc.pid.to_string()).output();
    state.action_message = Some(match result {
        Ok(out) if out.status.success() => ActionMessage {
            text: format!("OK KILL {} ({})", proc.pid, proc.name),
            ok: true,
        },
        Ok(out) => ActionMessage {
            text: format!(
                "x KILL {} ({}): {}",
                proc.pid,
                proc.name,
                short_command_error(&out.stderr, &out.stdout)
            ),
            ok: false,
        },
        Err(err) => ActionMessage {
            text: format!("x KILL {} ({}): {err}", proc.pid, proc.name),
            ok: false,
        },
    });
    state.process_kill_confirm = false;
}

fn short_command_error(stderr: &[u8], stdout: &[u8]) -> String {
    let msg = if stderr.is_empty() {
        String::from_utf8_lossy(stdout)
    } else {
        String::from_utf8_lossy(stderr)
    };
    msg.lines()
        .next()
        .unwrap_or("command failed")
        .trim()
        .to_string()
}

pub fn render_dashboard_text(
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &TuiState,
) -> String {
    render_dashboard_text_width(snapshot, report, state, DEFAULT_WIDTH)
}

fn render_dashboard_text_width(
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &TuiState,
    width: usize,
) -> String {
    render_dashboard_text_size(snapshot, report, state, width, DEFAULT_HEIGHT)
}

fn render_dashboard_text_size(
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &TuiState,
    width: usize,
    height: usize,
) -> String {
    let width = width.max(80);
    let height = height.max(15);
    let usable_h = height.saturating_sub(2).max(13);

    let mut left_w = width * 35 / 100;
    if left_w < 30 {
        left_w = 30;
    }
    let mut right_w = width.saturating_sub(left_w);
    if right_w < 30 {
        right_w = 30;
        left_w = width.saturating_sub(right_w);
    }

    let gpu_h = if snapshot.gpus.is_empty() {
        0
    } else {
        usable_h * 15 / 100
    };
    let sentinel_h = (usable_h * 16 / 100).max(5);
    let cpu_h = usable_h * 22 / 100;
    let mem_h = usable_h * 22 / 100;
    let net_h = usable_h * 17 / 100;
    let disk_h = usable_h.saturating_sub(sentinel_h + cpu_h + gpu_h + mem_h + net_h);

    let mut docker_h = (usable_h * 28 / 100).max(8);
    if usable_h.saturating_sub(docker_h) < 8 {
        docker_h = usable_h.saturating_sub(8);
    }
    let proc_h = usable_h.saturating_sub(docker_h);

    let left_col = if state.inspector_open {
        render_inspector(snapshot, state, left_w, usable_h)
    } else {
        let mut left_panels = Vec::new();
        left_panels.push(render_sentinel(report, left_w, sentinel_h));
        left_panels.push(render_cpu(snapshot, left_w, cpu_h));
        if !snapshot.gpus.is_empty() {
            left_panels.push(render_gpu(snapshot, left_w, gpu_h));
        }
        left_panels.push(render_memory(snapshot, left_w, mem_h));
        left_panels.push(render_network(snapshot, left_w, net_h));
        left_panels.push(render_storage(snapshot, left_w, disk_h));
        left_panels.join("\n")
    };

    let right_panels = [
        render_processes(snapshot, state, right_w, proc_h),
        render_docker_text(snapshot, state, right_w, docker_h),
    ];

    let body = join_horizontal(&left_col, &right_panels.join("\n"), left_w, right_w);

    [render_header(snapshot, width), body, help_bar(width)].join("\n")
}

fn render_header(snapshot: &metrics::Snapshot, width: usize) -> String {
    let app = " █ DURANDAL ";
    let tagline = " SYSTEMS MONITOR";
    let user = if snapshot.host.user.is_empty() {
        "system"
    } else {
        &snapshot.host.user
    };
    let mut right = user.to_string();
    if !snapshot.host.hostname.is_empty() {
        right.push('@');
        right.push_str(&snapshot.host.hostname);
    }
    if !snapshot.host.uptime.is_empty() {
        right.push_str("  //  ");
        right.push_str(&snapshot.host.uptime);
    }
    let left = format!("{app}{tagline}");
    let fill = width.saturating_sub(plain_width(&left) + plain_width(&right) + 1);
    format!("{}{}{} ", left, " ".repeat(fill), right)
}

fn render_sentinel(report: &ops::Report, width: usize, height: usize) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{:03} /100  {}  OPS READINESS",
        report.score,
        report.status.as_str()
    ));
    lines.push(thin_rule(width.saturating_sub(2)));
    let max_alerts = height.saturating_sub(4).max(1);
    if report.alerts.is_empty() {
        lines.push("OK → Awaiting telemetry".into());
    } else {
        for (i, alert) in report.alerts.iter().enumerate() {
            if i >= max_alerts {
                lines.push(format!("+{} MORE SIGNALS", report.alerts.len() - i));
                break;
            }
            lines.push(format!("{} → {}", alert.label, alert.message));
        }
    }
    mag_panel("LAPIS SENTINEL", &lines.join("\n"), width, height)
}

fn render_cpu(snapshot: &metrics::Snapshot, width: usize, height: usize) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "{:.1}%  {} THREADS",
        snapshot.cpu.percent, snapshot.cpu.threads
    ));
    let body_h = height.saturating_sub(2).max(1);
    let core_rows = if snapshot.cpu.threads > 0 {
        ((snapshot.cpu.threads + 1) / 2).min(body_h / 3).max(1)
    } else {
        1
    };
    let spark_h = body_h.saturating_sub(1 + core_rows).max(1);
    for _ in 0..spark_h {
        lines.push(sparkline(snapshot.cpu.percent, width.saturating_sub(2)));
    }
    for i in 0..core_rows {
        let pct = (snapshot.cpu.percent + (i as f64 * 7.0)).min(100.0);
        lines.push(format!("C{:<2} {}", i, bar(pct, width.saturating_sub(9))));
    }
    mag_panel("CPU", &lines.join("\n"), width, height)
}

fn render_gpu(snapshot: &metrics::Snapshot, width: usize, height: usize) -> String {
    let mut lines = Vec::new();
    for (i, gpu) in snapshot
        .gpus
        .iter()
        .take((height.saturating_sub(2) / 3).max(1))
        .enumerate()
    {
        let pct = if gpu.memory_total_mb > 0 {
            gpu.memory_used_mb as f64 / gpu.memory_total_mb as f64 * 100.0
        } else {
            0.0
        };
        lines.push(format!(
            "GPU {i}: {}",
            truncate(&gpu.name, width.saturating_sub(9))
        ));
        lines.push(format!(
            "UTIL: {:>3.0}%  TEMP: {:.0}°C",
            gpu.utilization_percent, gpu.temperature_c
        ));
        lines.push(format!(
            "{} {}M/{}M",
            bar(pct, width.saturating_sub(18)),
            gpu.memory_used_mb,
            gpu.memory_total_mb
        ));
    }
    mag_panel("GPU", &lines.join("\n"), width, height)
}

fn render_memory(snapshot: &metrics::Snapshot, width: usize, height: usize) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "RAM  {}/{}",
        format_bytes(snapshot.memory.used_bytes),
        format_bytes(snapshot.memory.total_bytes)
    ));
    lines.push(bar(snapshot.memory.percent, width.saturating_sub(2)));
    lines.push(format!(
        "SWAP {}/{}",
        format_bytes(snapshot.swap.used_bytes),
        format_bytes(snapshot.swap.total_bytes)
    ));
    lines.push(bar(snapshot.swap.percent, width.saturating_sub(2)));
    lines.push(format!(
        "CACHE {}  BUF {}",
        format_bytes(snapshot.memory.cached_bytes),
        format_bytes(snapshot.memory.buffers_bytes)
    ));
    mag_panel("MEMORY", &lines.join("\n"), width, height)
}

fn render_network(snapshot: &metrics::Snapshot, width: usize, height: usize) -> String {
    let mut lines = Vec::new();
    lines.push(format!(
        "▼ DOWN {}",
        format_bytes_rate(snapshot.network.recv_bytes_per_sec)
    ));
    lines.push(sparkline(
        network_pct(
            snapshot.network.recv_bytes_per_sec,
            snapshot.network.sent_bytes_per_sec,
        ),
        width.saturating_sub(2),
    ));
    lines.push(format!(
        "▲ UP   {}",
        format_bytes_rate(snapshot.network.sent_bytes_per_sec)
    ));
    lines.push(sparkline(
        network_pct(
            snapshot.network.sent_bytes_per_sec,
            snapshot.network.recv_bytes_per_sec,
        ),
        width.saturating_sub(2),
    ));
    mag_panel("NETWORK", &lines.join("\n"), width, height)
}

fn render_storage(snapshot: &metrics::Snapshot, width: usize, height: usize) -> String {
    let mut lines = Vec::new();
    let max_disks = ((height.saturating_sub(2)) / 2).max(1);
    for disk in snapshot.disks.iter().take(max_disks) {
        lines.push(format!(
            "{} {}/{} {}",
            truncate(&disk.mountpoint, 15),
            format_bytes(disk.used_bytes),
            format_bytes(disk.total_bytes),
            disk.filesystem
        ));
        lines.push(bar(disk.percent, width.saturating_sub(2)));
    }
    if lines.is_empty() {
        lines.push("No mounted disks reported".into());
    }
    mag_panel("STORAGE", &lines.join("\n"), width, height)
}

fn render_inspector(
    snapshot: &metrics::Snapshot,
    state: &TuiState,
    width: usize,
    height: usize,
) -> String {
    let Some(proc) = selected_process(snapshot, state) else {
        return mag_panel("INSPECTOR", "No process selected", width, height);
    };
    let command = if proc.command.is_empty() {
        &proc.name
    } else {
        &proc.command
    };
    let lines = [
        format!("PID      {}", proc.pid),
        format!("NAME     {}", proc.name),
        format!("USER     {}", proc.user),
        format!("STATUS   {}", proc.status),
        format!("CPU      {:.1}%", proc.cpu_percent),
        format!("MEM      {:.1}%", proc.memory_percent),
        format!("RSS      {}", format_bytes(proc.rss_bytes)),
        String::new(),
        "COMMAND".into(),
        truncate(command, width.saturating_sub(2)),
    ];
    mag_panel("INSPECTOR", &lines.join("\n"), width, height)
}

fn render_processes(
    snapshot: &metrics::Snapshot,
    state: &TuiState,
    width: usize,
    height: usize,
) -> String {
    let inner = width.saturating_sub(4).max(24);
    let mut lines = Vec::new();
    let processes = filtered_sorted_processes(snapshot, state);
    let sort = if state.sort_by_cpu {
        "CPU▼"
    } else {
        "MEM▼"
    };
    let mut status_line = if state.filtering {
        format!("/{}", state.filter)
    } else if state.process_kill_confirm {
        selected_process(snapshot, state).map_or_else(
            || "  KILL?".to_string(),
            |proc| format!("  KILL {} ({})? [Y]ES [N]O", proc.pid, proc.name),
        )
    } else if let Some(message) = &state.action_message {
        format!("  {}", message.text)
    } else {
        format!("  SORT:{sort}  {} PROCS", processes.len())
    };
    if !state.filter.is_empty() && !state.filtering && !state.process_kill_confirm {
        status_line.push_str(&format!("  FILTER:/{}", state.filter));
    }
    lines.push(status_line);
    lines.push(String::new());
    lines.push(format!(
        " {}",
        proc_row("PID", "COMMAND", "CPU%", "MEM%", "RSS", "USER", inner)
    ));
    let visible = height.saturating_sub(7).max(1);
    let offset = visible_offset(state.process_cursor, state.process_offset, visible);
    for proc in processes.iter().skip(offset).take(visible) {
        let command = if proc.command.is_empty() {
            &proc.name
        } else {
            &proc.command
        };
        lines.push(format!(
            " {}",
            proc_row(
                &proc.pid.to_string(),
                command,
                &format!("{:.1}", proc.cpu_percent),
                &format!("{:.1}", proc.memory_percent),
                &format_bytes(proc.rss_bytes),
                &proc.user,
                inner,
            )
        ));
    }
    mag_panel("PROCESSES", &lines.join("\n"), width, height)
}

fn render_docker_text(
    snapshot: &metrics::Snapshot,
    state: &TuiState,
    width: usize,
    height: usize,
) -> String {
    if !state.docker_panel_visible {
        return mag_panel(
            "CONTAINER STATUS HIDDEN",
            "DOCKER HIDDEN — press d to reveal read-only container status.",
            width,
            4,
        );
    }

    if !snapshot.docker.available {
        return mag_panel(
            "DOCKER",
            &format!(
                "OFFLINE {}\n\nInstall Docker or start the daemon to enable status.",
                snapshot
                    .docker
                    .error
                    .clone()
                    .unwrap_or_else(|| "docker unavailable".into())
            ),
            width,
            height,
        );
    }

    let payload = agent::build_payload(snapshot, &ops::evaluate_snapshot(snapshot), 5);
    let mut lines = Vec::new();
    let status_line = if let Some(confirm) = state.docker_confirm {
        selected_container(snapshot, state).map_or_else(
            || format!(" {}?", confirm.as_str().to_uppercase()),
            |container| {
                format!(
                    " {} {}? [Y]ES [N]O",
                    confirm.as_str().to_uppercase(),
                    container.name
                )
            },
        )
    } else if state.docker_focused {
        format!(
            " ACTIVE {} CONTAINERS  {} RUNNING  {} STOPPED",
            payload.docker.total, payload.docker.running, payload.docker.stopped
        )
    } else if let Some(message) = &state.action_message {
        format!(" {}", message.text)
    } else {
        format!(
            " IDLE {} CONTAINERS  {} RUNNING  {} STOPPED",
            payload.docker.total, payload.docker.running, payload.docker.stopped
        )
    };
    lines.push(status_line);
    lines.push(String::new());
    if snapshot.docker.containers.is_empty() {
        lines.push("No local containers.".into());
        return mag_panel("DOCKER", &lines.join("\n"), width, height);
    }
    lines.push(format!(
        " {}",
        docker_row("STATE", "NAME", "IMAGE", width.saturating_sub(4),)
    ));
    let visible = height.saturating_sub(7).max(1);
    let offset = visible_offset(state.docker_cursor, state.docker_offset, visible);
    for container in snapshot.docker.containers.iter().skip(offset).take(visible) {
        let state = if container.running {
            "RUN".to_string()
        } else {
            container.state.to_uppercase()
        };
        lines.push(format!(
            " {}",
            docker_row(
                &state,
                &container.name,
                &container.image,
                width.saturating_sub(4),
            )
        ));
    }
    mag_panel("DOCKER", &lines.join("\n"), width, height)
}

fn mag_panel(title: &str, content: &str, width: usize, height: usize) -> String {
    let width = width.max(8);
    let inner_h = height.saturating_sub(2).max(1);
    let mut out = Vec::new();
    out.push(section_header(title, width));
    let mut content_lines = content.lines();
    for _ in 0..inner_h {
        let line = content_lines.next().unwrap_or_default();
        let body_width = width.saturating_sub(2);
        let line = truncate(line, body_width);
        let pad = body_width.saturating_sub(plain_width(&line));
        out.push(format!("▎ {}{}", line, " ".repeat(pad)));
    }
    out.push(thin_rule(width));
    out.join("\n")
}

fn section_header(label: &str, width: usize) -> String {
    let label = format!(" {} ", track(label));
    let fill = width.saturating_sub(plain_width(&label));
    format!("{}{}", label, "▀".repeat(fill))
}

fn help_bar(width: usize) -> String {
    let parts = [
        ("↑/k", "UP"),
        ("↓/j", "DN"),
        ("s", "SORT"),
        ("/", "FIND"),
        ("⏎", "INSPECT"),
        ("K", "KILL"),
        ("c", "DOCKER"),
        ("x", "START/STOP"),
        ("r", "RESTART"),
        ("d", "DIM"),
        ("q", "QUIT"),
    ];
    let bar = parts
        .iter()
        .map(|(key, desc)| format!(" {key}  {desc}"))
        .collect::<Vec<_>>()
        .join("  ");
    let pad = width.saturating_sub(plain_width(&bar)) / 2;
    format!("{}{}", " ".repeat(pad), bar)
}

fn join_horizontal(left: &str, right: &str, left_w: usize, _right_w: usize) -> String {
    let left_lines: Vec<&str> = left.lines().collect();
    let right_lines: Vec<&str> = right.lines().collect();
    let rows = left_lines.len().max(right_lines.len());
    let mut out = Vec::with_capacity(rows);
    for i in 0..rows {
        let l = *left_lines.get(i).unwrap_or(&"");
        let r = *right_lines.get(i).unwrap_or(&"");
        let pad = left_w.saturating_sub(plain_width(l));
        out.push(format!("{}{}{}", l, " ".repeat(pad), r));
    }
    out.join("\n")
}

fn proc_row(
    pid: &str,
    cmd: &str,
    cpu: &str,
    mem: &str,
    rss: &str,
    user: &str,
    max_w: usize,
) -> String {
    let fixed = 41usize;
    let cmd_w = max_w.saturating_sub(fixed).max(5);
    truncate(
        &format!(
            " {:<7} {:<cmd_w$} {:>6} {:>6} {:>8} {:<8}",
            pid,
            truncate(cmd, cmd_w),
            cpu,
            mem,
            rss,
            truncate(user, 8),
        ),
        max_w,
    )
}

fn docker_row(state: &str, name: &str, image: &str, max_w: usize) -> String {
    let fixed = 16usize;
    let name_w = ((max_w.saturating_sub(fixed)) / 2).max(6);
    let image_w = max_w.saturating_sub(fixed + name_w).max(6);
    truncate(
        &format!(
            " {:<8} {:<name_w$} {:<image_w$}",
            state,
            docker_cell(name, name_w),
            docker_cell(image, image_w)
        ),
        max_w,
    )
}

fn docker_cell(text: &str, width: usize) -> String {
    if plain_width(text) > width {
        if width <= 3 {
            truncate(text, width)
        } else {
            format!("{}...", text.chars().take(width - 3).collect::<String>())
        }
    } else {
        format!("{text:<width$}")
    }
}

fn filtered_sorted_processes<'a>(
    snapshot: &'a metrics::Snapshot,
    state: &TuiState,
) -> Vec<&'a metrics::ProcessInfo> {
    let filter = state.filter.to_lowercase();
    let mut processes = snapshot
        .top_processes
        .iter()
        .filter(|proc| {
            if filter.is_empty() {
                return true;
            }
            proc.name.to_lowercase().contains(&filter)
                || proc.command.to_lowercase().contains(&filter)
                || proc.pid.to_string().contains(&filter)
        })
        .collect::<Vec<_>>();

    if state.sort_by_cpu {
        processes.sort_by(|a, b| b.cpu_percent.total_cmp(&a.cpu_percent));
    } else {
        processes.sort_by(|a, b| b.memory_percent.total_cmp(&a.memory_percent));
    }

    processes
}

fn selected_process<'a>(
    snapshot: &'a metrics::Snapshot,
    state: &TuiState,
) -> Option<&'a metrics::ProcessInfo> {
    filtered_sorted_processes(snapshot, state)
        .get(state.process_cursor)
        .copied()
}

fn selected_container<'a>(
    snapshot: &'a metrics::Snapshot,
    state: &TuiState,
) -> Option<&'a metrics::ContainerInfo> {
    snapshot.docker.containers.get(state.docker_cursor)
}

fn visible_offset(cursor: usize, offset: usize, visible: usize) -> usize {
    if visible == 0 {
        return 0;
    }
    if cursor < offset {
        cursor
    } else if cursor >= offset + visible {
        cursor - visible + 1
    } else {
        offset
    }
}

fn track(text: &str) -> String {
    text.to_uppercase()
        .chars()
        .map(|ch| ch.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn thin_rule(width: usize) -> String {
    "─".repeat(width.max(1))
}

fn bar(percent: f64, width: usize) -> String {
    if width < 6 {
        return format!("{:>3.0}%", percent);
    }
    let bar_w = width.saturating_sub(5).max(1);
    let filled = ((percent.clamp(0.0, 100.0) / 100.0) * bar_w as f64).round() as usize;
    format!(
        "{}{} {:>3.0}%",
        "█".repeat(filled.min(bar_w)),
        "░".repeat(bar_w.saturating_sub(filled)),
        percent.clamp(0.0, 999.0)
    )
}

fn sparkline(percent: f64, width: usize) -> String {
    let chars = [' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let idx = ((percent.clamp(0.0, 100.0) / 100.0) * (chars.len() - 1) as f64).round() as usize;
    chars[idx.min(chars.len() - 1)]
        .to_string()
        .repeat(width.max(1))
}

fn network_pct(value: u64, other: u64) -> f64 {
    let max = value.max(other);
    if max == 0 {
        0.0
    } else {
        value as f64 / max as f64 * 100.0
    }
}

fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KiB", "MiB", "GiB", "TiB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{}{}", bytes, UNITS[unit])
    } else {
        format!("{value:.1}{}", UNITS[unit])
    }
}

fn format_bytes_rate(bytes: u64) -> String {
    format!("{}/s", format_bytes(bytes))
}

fn truncate(text: &str, max: usize) -> String {
    if plain_width(text) <= max {
        return text.to_string();
    }
    if max <= 1 {
        return "…".into();
    }
    let mut out = String::new();
    for ch in text.chars().take(max - 1) {
        out.push(ch);
    }
    out.push('…');
    out
}

fn plain_width(text: &str) -> usize {
    text.chars().count()
}

fn primary(dimmed: bool) -> Color {
    if dimmed { DIMMED_LIME } else { NEON_LIME }
}

fn secondary(dimmed: bool) -> Color {
    if dimmed { DIMMED_PINK } else { HOT_PINK }
}

fn tertiary(dimmed: bool) -> Color {
    if dimmed { DIMMED_CYAN } else { CYAN }
}

fn usage_color(percent: f64, dimmed: bool) -> Color {
    if percent >= 90.0 {
        RED
    } else if percent >= 70.0 {
        AMBER
    } else if percent >= 40.0 {
        primary(dimmed)
    } else {
        tertiary(dimmed)
    }
}

fn panel_accent_from_header(line: &str, report: &ops::Report, dimmed: bool) -> Color {
    if line.contains("L A P I S   S E N T I N E L") {
        return match report.status {
            ops::Status::Clear => primary(dimmed),
            ops::Status::Watch => tertiary(dimmed),
            ops::Status::Warn => AMBER,
            ops::Status::Crit => RED,
        };
    }
    if line.contains("N E T W O R K") || line.contains("D O C K E R") {
        secondary(dimmed)
    } else if line.contains("M E M O R Y") {
        tertiary(dimmed)
    } else if line.contains("S T O R A G E") {
        AMBER
    } else {
        primary(dimmed)
    }
}

fn styled_dashboard_text(
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &TuiState,
    width: usize,
    height: usize,
) -> Text<'static> {
    let plain = render_dashboard_text_size(snapshot, report, state, width, height);
    let mut lines = Vec::new();
    let mut accent = primary(state.dimmed);

    for (row, line) in plain.lines().enumerate() {
        if row == 0 {
            lines.push(styled_header_line(line, state.dimmed));
            continue;
        }

        if line.contains('▀') {
            accent = panel_accent_from_header(line, report, state.dimmed);
            lines.push(styled_section_header(line, accent, report, state.dimmed));
        } else if line.chars().all(|ch| ch == '─') {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(DIM_GREY),
            )));
        } else if line.trim_start().starts_with('▎') {
            lines.push(styled_body_line(line, accent, snapshot, state));
        } else if line.contains("↑/k") && line.contains("QUIT") {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(MUTED_GREY),
            )));
        } else {
            lines.push(Line::from(Span::styled(
                line.to_string(),
                Style::default().fg(OFF_WHITE),
            )));
        }
    }

    Text::from(lines)
}

fn styled_header_line(line: &str, dimmed: bool) -> Line<'static> {
    let app = " █ DURANDAL ";
    let tagline = " SYSTEMS MONITOR";
    if let Some(rest) = line.strip_prefix(app) {
        let (tag, remainder) = if let Some(rest) = rest.strip_prefix(tagline) {
            (tagline, rest)
        } else {
            ("", rest)
        };
        Line::from(vec![
            Span::styled(
                app.to_string(),
                Style::default()
                    .fg(DEEP_BLACK)
                    .bg(primary(dimmed))
                    .add_modifier(Modifier::BOLD),
            ),
            Span::styled(
                tag.to_string(),
                Style::default().fg(MUTED_GREY).bg(DARK_NAVY),
            ),
            Span::styled(
                remainder.to_string(),
                Style::default().fg(OFF_WHITE).bg(DARK_NAVY),
            ),
        ])
    } else {
        Line::from(Span::styled(
            line.to_string(),
            Style::default().fg(OFF_WHITE).bg(DARK_NAVY),
        ))
    }
}

fn styled_section_header(
    line: &str,
    accent: Color,
    report: &ops::Report,
    dimmed: bool,
) -> Line<'static> {
    let mut spans = Vec::new();
    let mut rest = line;
    let mut current_accent = accent;

    while !rest.is_empty() {
        if let Some((label, label_accent)) = section_label_at(rest, report, dimmed) {
            current_accent = label_accent;
            spans.push(Span::styled(
                label.to_string(),
                Style::default()
                    .fg(DEEP_BLACK)
                    .bg(current_accent)
                    .add_modifier(Modifier::BOLD),
            ));
            rest = &rest[label.len()..];
            continue;
        }

        let next_label = next_section_label_pos(rest).unwrap_or(rest.len());
        let (chunk, next) = rest.split_at(next_label);
        spans.push(Span::styled(
            chunk.to_string(),
            Style::default().fg(current_accent),
        ));
        rest = next;
    }

    Line::from(spans)
}

fn section_label_at(
    text: &str,
    report: &ops::Report,
    dimmed: bool,
) -> Option<(&'static str, Color)> {
    section_labels().iter().find_map(|(label, role)| {
        text.starts_with(label)
            .then(|| (*label, section_role_color(*role, report, dimmed)))
    })
}

fn next_section_label_pos(text: &str) -> Option<usize> {
    section_labels()
        .iter()
        .filter_map(|(label, _)| text.find(label))
        .min()
}

#[derive(Clone, Copy)]
enum SectionRole {
    Sentinel,
    Primary,
    Secondary,
    Tertiary,
    Amber,
}

fn section_role_color(role: SectionRole, report: &ops::Report, dimmed: bool) -> Color {
    match role {
        SectionRole::Sentinel => match report.status {
            ops::Status::Clear => primary(dimmed),
            ops::Status::Watch => tertiary(dimmed),
            ops::Status::Warn => AMBER,
            ops::Status::Crit => RED,
        },
        SectionRole::Primary => primary(dimmed),
        SectionRole::Secondary => secondary(dimmed),
        SectionRole::Tertiary => tertiary(dimmed),
        SectionRole::Amber => AMBER,
    }
}

fn section_labels() -> &'static [(&'static str, SectionRole)] {
    &[
        (" L A P I S   S E N T I N E L ", SectionRole::Sentinel),
        (" C P U ", SectionRole::Primary),
        (" G P U ", SectionRole::Primary),
        (" M E M O R Y ", SectionRole::Tertiary),
        (" N E T W O R K ", SectionRole::Secondary),
        (" S T O R A G E ", SectionRole::Amber),
        (" P R O C E S S E S ", SectionRole::Primary),
        (" D O C K E R ", SectionRole::Secondary),
        (
            " C O N T A I N E R   S T A T U S   H I D D E N ",
            SectionRole::Secondary,
        ),
    ]
}

fn styled_body_line(
    line: &str,
    accent: Color,
    snapshot: &metrics::Snapshot,
    state: &TuiState,
) -> Line<'static> {
    let Some(idx) = line.find('▎') else {
        return Line::from(Span::raw(line.to_string()));
    };
    let (prefix, body) = line.split_at(idx);
    let mut spans = vec![
        Span::styled(prefix.to_string(), Style::default().fg(OFF_WHITE)),
        Span::styled("▎".to_string(), Style::default().fg(accent)),
    ];
    let rest = body.trim_start_matches('▎');
    spans.extend(style_content(rest, snapshot, state));
    Line::from(spans)
}

fn style_content(
    content: &str,
    snapshot: &metrics::Snapshot,
    state: &TuiState,
) -> Vec<Span<'static>> {
    let dimmed = state.dimmed;
    if content.contains("CPU▼") {
        return split_keyword(content, "CPU▼", primary(dimmed));
    }
    if content.contains("MEM▼") {
        return split_keyword(content, "MEM▼", secondary(dimmed));
    }
    if content.contains("▼ DOWN") {
        return split_keyword(content, "▼ DOWN", primary(dimmed));
    }
    if content.contains("▲ UP") {
        return split_keyword(content, "▲ UP", secondary(dimmed));
    }
    if content.contains("█") || content.contains("░") {
        let color = if content.contains("RAM") || content.contains("SWAP") {
            tertiary(dimmed)
        } else if content.contains('/') {
            AMBER
        } else {
            usage_color(snapshot.cpu.percent, dimmed)
        };
        return vec![Span::styled(
            content.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        )];
    }
    if content.contains(" STATE") || content.contains(" PID") {
        return vec![Span::styled(
            content.to_string(),
            Style::default()
                .fg(DEEP_BLACK)
                .bg(MUTED_GREY)
                .add_modifier(Modifier::BOLD),
        )];
    }
    if selected_process_line(content, snapshot, state) {
        let bg = if state.process_kill_confirm {
            RED
        } else {
            primary(dimmed)
        };
        return vec![Span::styled(
            content.to_string(),
            Style::default()
                .fg(DEEP_BLACK)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )];
    }
    if state.docker_focused && selected_container_line(content, snapshot, state) {
        let bg = if state.docker_confirm.is_some() {
            RED
        } else {
            primary(dimmed)
        };
        return vec![Span::styled(
            content.to_string(),
            Style::default()
                .fg(DEEP_BLACK)
                .bg(bg)
                .add_modifier(Modifier::BOLD),
        )];
    }
    if content.contains("KILL ") || content.contains("[Y]ES") || content.contains("x ") {
        return vec![Span::styled(
            content.to_string(),
            Style::default().fg(RED).add_modifier(Modifier::BOLD),
        )];
    }
    if content.contains("OK ") {
        return vec![Span::styled(
            content.to_string(),
            Style::default()
                .fg(primary(dimmed))
                .add_modifier(Modifier::BOLD),
        )];
    }
    vec![Span::styled(
        content.to_string(),
        Style::default().fg(OFF_WHITE),
    )]
}

fn split_keyword(content: &str, keyword: &str, color: Color) -> Vec<Span<'static>> {
    let Some(pos) = content.find(keyword) else {
        return vec![Span::styled(
            content.to_string(),
            Style::default().fg(OFF_WHITE),
        )];
    };
    let (before, after) = content.split_at(pos);
    let after_keyword = &after[keyword.len()..];
    vec![
        Span::styled(before.to_string(), Style::default().fg(MUTED_GREY)),
        Span::styled(
            keyword.to_string(),
            Style::default().fg(color).add_modifier(Modifier::BOLD),
        ),
        Span::styled(after_keyword.to_string(), Style::default().fg(OFF_WHITE)),
    ]
}

fn selected_process_line(content: &str, snapshot: &metrics::Snapshot, state: &TuiState) -> bool {
    if state.docker_focused {
        return false;
    }
    let Some(proc) = selected_process(snapshot, state) else {
        return false;
    };
    content.contains(&format!(" {:<7} ", proc.pid))
}

fn selected_container_line(content: &str, snapshot: &metrics::Snapshot, state: &TuiState) -> bool {
    let Some(container) = selected_container(snapshot, state) else {
        return false;
    };
    content.contains(&container.name)
}

fn draw(
    frame: &mut ratatui::Frame<'_>,
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &TuiState,
) {
    let area = frame.area();
    let width = usize::from(area.width).max(80);
    let height = usize::from(area.height).max(15);
    let text = styled_dashboard_text(snapshot, report, state, width, height);
    let paragraph = Paragraph::new(text)
        .style(Style::default().fg(OFF_WHITE))
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot() -> metrics::Snapshot {
        metrics::Snapshot {
            host: metrics::HostInfo {
                hostname: "durandal-host".into(),
                user: "lapis".into(),
                uptime: "5m".into(),
                ..Default::default()
            },
            cpu: metrics::CpuInfo {
                percent: 42.0,
                threads: 8,
                ..Default::default()
            },
            memory: metrics::MemoryInfo {
                total_bytes: 16 * 1024 * 1024 * 1024,
                used_bytes: 7 * 1024 * 1024 * 1024,
                percent: 44.0,
                ..Default::default()
            },
            top_processes: vec![
                metrics::ProcessInfo {
                    pid: 100,
                    name: "cpu-heavy".into(),
                    cpu_percent: 80.0,
                    memory_percent: 2.0,
                    rss_bytes: 32 * 1024 * 1024,
                    user: "lapis".into(),
                    command: "cpu-heavy --serve".into(),
                    ..Default::default()
                },
                metrics::ProcessInfo {
                    pid: 200,
                    name: "mem-heavy".into(),
                    cpu_percent: 5.0,
                    memory_percent: 70.0,
                    rss_bytes: 1024 * 1024 * 1024,
                    user: "lapis".into(),
                    command: "mem-heavy --serve".into(),
                    ..Default::default()
                },
            ],
            docker: metrics::DockerInfo {
                available: true,
                containers: vec![
                    metrics::ContainerInfo {
                        id: "abc123".into(),
                        image: "postgres:16".into(),
                        name: "db".into(),
                        state: "running".into(),
                        running: true,
                        ..Default::default()
                    },
                    metrics::ContainerInfo {
                        id: "def456".into(),
                        image: "redis:7".into(),
                        name: "cache".into(),
                        state: "exited".into(),
                        running: false,
                        ..Default::default()
                    },
                ],
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn process_filter_and_mem_sort_affect_rendered_rows() {
        let snapshot = snapshot();
        let report = ops::evaluate_snapshot(&snapshot);
        let state = TuiState {
            sort_by_cpu: false,
            filter: "mem".into(),
            ..Default::default()
        };

        let text = render_dashboard_text_size(&snapshot, &report, &state, 120, 36);

        assert!(text.contains("FILTER:/mem"));
        assert!(text.contains("mem-heavy"));
        assert!(!text.contains("cpu-heavy"));
        assert!(text.contains("MEM▼"));
    }

    #[test]
    fn inspector_and_confirmations_replace_status_lines() {
        let snapshot = snapshot();
        let report = ops::evaluate_snapshot(&snapshot);
        let process_state = TuiState {
            inspector_open: true,
            process_kill_confirm: true,
            ..Default::default()
        };

        let text = render_dashboard_text_size(&snapshot, &report, &process_state, 120, 36);

        assert!(text.contains("I N S P E C T O R"));
        assert!(text.contains("KILL 100"));

        let docker_state = TuiState {
            docker_focused: true,
            docker_confirm: Some(DockerConfirm::Restart),
            ..Default::default()
        };
        let text = render_dashboard_text_size(&snapshot, &report, &docker_state, 120, 36);

        assert!(text.contains("RESTART db? [Y]ES [N]O"));
    }
}

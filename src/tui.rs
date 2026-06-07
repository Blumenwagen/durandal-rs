use crate::{agent, config::Config, metrics, ops};
use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    style::{Color, Modifier, Style},
    widgets::{Paragraph, Wrap},
};
use std::{
    io,
    time::{Duration, Instant},
};

const DEFAULT_WIDTH: usize = 120;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TuiState {
    pub docker_panel_visible: bool,
    pub docker_controls_enabled: bool,
    pub docker_focused: bool,
    pub sort_by_cpu: bool,
    pub dimmed: bool,
}

impl Default for TuiState {
    fn default() -> Self {
        Self {
            docker_panel_visible: true,
            docker_controls_enabled: true,
            docker_focused: false,
            sort_by_cpu: true,
            dimmed: false,
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
            match key.code {
                KeyCode::Char('q') | KeyCode::Esc => break,
                KeyCode::Char('c') => state.toggle_docker_focus(),
                KeyCode::Char('s') | KeyCode::Tab => state.toggle_sort(),
                KeyCode::Char('d') => state.toggle_dimmed(),
                _ => {}
            }
        }
        if last_tick.elapsed() > Duration::from_secs(1) {
            snapshot = metrics::collect_snapshot(50).unwrap_or_default();
            last_tick = Instant::now();
        }
    }
    Ok(())
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
    let width = width.max(80);
    let left_w = (width * 35 / 100).clamp(32, 48);
    let right_w = width.saturating_sub(left_w).max(42);

    let mut left_panels = Vec::new();
    left_panels.push(render_sentinel(report, left_w, 8));
    left_panels.push(render_cpu(snapshot, left_w, 9));
    if !snapshot.gpus.is_empty() {
        left_panels.push(render_gpu(snapshot, left_w, 5));
    }
    left_panels.push(render_memory(snapshot, left_w, 8));
    left_panels.push(render_network(snapshot, left_w, 7));
    left_panels.push(render_storage(snapshot, left_w, 8));

    let right_panels = [
        render_processes(snapshot, state, right_w, 24),
        render_docker_text(snapshot, state, right_w, 10),
    ];

    let body = join_horizontal(
        &left_panels.join("\n"),
        &right_panels.join("\n"),
        left_w,
        right_w,
    );

    [render_header(snapshot, width), body, help_bar(width)].join("\n")
}

fn render_header(snapshot: &metrics::Snapshot, width: usize) -> String {
    let app = "█ DURANDAL";
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
    let left = format!(" {app} {tagline}");
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
    lines.push(sparkline(snapshot.cpu.percent, width.saturating_sub(2)));
    let core_rows = (height.saturating_sub(5)).max(2);
    for i in 0..core_rows {
        let pct = (snapshot.cpu.percent + (i as f64 * 7.0)).min(100.0);
        lines.push(format!("C{:<2} {}", i, bar(pct, width.saturating_sub(9))));
    }
    mag_panel("CPU", &lines.join("\n"), width, height)
}

fn render_gpu(snapshot: &metrics::Snapshot, width: usize, height: usize) -> String {
    let mut lines = Vec::new();
    for gpu in snapshot.gpus.iter().take(height.saturating_sub(2).max(1)) {
        lines.push(format!(
            "{} {} VRAM {}/{} MiB",
            truncate(&gpu.name, 14),
            bar(gpu.utilization_percent, width.saturating_sub(24)),
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

fn render_processes(
    snapshot: &metrics::Snapshot,
    state: &TuiState,
    width: usize,
    height: usize,
) -> String {
    let inner = width.saturating_sub(4).max(24);
    let mut lines = Vec::new();
    let sort = if state.sort_by_cpu {
        "CPU▼"
    } else {
        "MEM▼"
    };
    lines.push(format!(
        " SORT:{sort}  {} PROCS",
        snapshot.top_processes.len()
    ));
    lines.push(String::new());
    lines.push(proc_row(
        "PID", "COMMAND", "CPU%", "MEM%", "RSS", "USER", inner,
    ));
    let visible = height.saturating_sub(7).max(1);
    for proc in snapshot.top_processes.iter().take(visible) {
        let command = if proc.command.is_empty() {
            &proc.name
        } else {
            &proc.command
        };
        lines.push(proc_row(
            &proc.pid.to_string(),
            command,
            &format!("{:.1}", proc.cpu_percent),
            &format!("{:.1}", proc.memory_percent),
            &format_bytes(proc.rss_bytes),
            &proc.user,
            inner,
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
    let focus = if state.docker_focused {
        "ACTIVE"
    } else {
        "IDLE"
    };
    lines.push(format!(
        " {focus} {} CONTAINERS  {} RUNNING  {} STOPPED",
        payload.docker.total, payload.docker.running, payload.docker.stopped
    ));
    lines.push(if state.docker_controls_enabled {
        " ACTIVE — x START/STOP  r RESTART  y/n CONFIRM".into()
    } else {
        " IDLE — controls unavailable".into()
    });
    lines.push(String::new());
    lines.push(docker_row(
        "STATE",
        "NAME",
        "IMAGE",
        width.saturating_sub(4),
    ));
    for container in snapshot
        .docker
        .containers
        .iter()
        .take(height.saturating_sub(7).max(1))
    {
        let state = if container.running {
            "RUN".to_string()
        } else {
            container.state.to_uppercase()
        };
        lines.push(docker_row(
            &state,
            &container.name,
            &container.image,
            width.saturating_sub(4),
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
    let fixed = 34usize;
    let cmd_w = max_w.saturating_sub(fixed).max(12);
    truncate(
        &format!(
            " {:>6} {:<cmd_w$} {:>5} {:>5} {:>7} {:<8}",
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
            truncate(name, name_w),
            truncate(image, image_w)
        ),
        max_w,
    )
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

fn draw(
    frame: &mut ratatui::Frame<'_>,
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &TuiState,
) {
    let area = frame.area();
    let width = usize::from(area.width).max(DEFAULT_WIDTH);
    let text = render_dashboard_text_width(snapshot, report, state, width);
    let status_color = match report.status {
        ops::Status::Clear => Color::LightGreen,
        ops::Status::Watch => Color::Cyan,
        ops::Status::Warn => Color::Yellow,
        ops::Status::Crit => Color::Red,
    };
    let paragraph = Paragraph::new(text)
        .style(
            Style::default()
                .fg(status_color)
                .add_modifier(Modifier::BOLD),
        )
        .wrap(Wrap { trim: false });
    frame.render_widget(paragraph, area);
}

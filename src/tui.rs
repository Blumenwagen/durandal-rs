mod view;
pub use view::draw as draw_dashboard;

use crate::{config::Config, docker, metrics, ops};
use anyhow::Context;
use crossterm::{
    event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::{
    collections::VecDeque,
    io,
    process::Command,
    sync::mpsc,
    time::{Duration, Instant},
};

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
    pub inspector_scroll: u16,
    pub docker_confirm: Option<DockerConfirm>,
    pub process_kill_confirm: bool,
    pub action_message: Option<ActionMessage>,
    pub paused: bool,
    pub help_open: bool,
    pub alerts_open: bool,
    pub alerts_scroll: u16,
    pub last_updated: Option<Instant>,
    pub refresh_error: Option<String>,
    pub process_cpu_available: bool,
    pub network_rates_available: bool,
    pub help_scroll: u16,
    pub process_page_size: usize,
    pub docker_page_size: usize,
    pub filter_before_edit: String,
    pub pending_process: Option<ProcessTarget>,
    pub pending_container: Option<ContainerTarget>,
    pub history: VecDeque<TelemetryPoint>,
}

/// One collected sample, in basis points and bytes/s; never populated by redraws.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TelemetryPoint {
    pub cpu_basis_points: u16,
    pub network: Option<(u64, u64)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessTarget {
    pub pid: i32,
    pub name: String,
    pub command: String,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerTarget {
    pub id: String,
    pub name: String,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DockerConfirm {
    Start,
    Stop,
    Restart,
}
impl DockerConfirm {
    fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
        }
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
            inspector_scroll: 0,
            docker_confirm: None,
            process_kill_confirm: false,
            action_message: None,
            paused: false,
            help_open: false,
            alerts_open: false,
            alerts_scroll: 0,
            last_updated: None,
            refresh_error: None,
            process_cpu_available: false,
            network_rates_available: false,
            help_scroll: 0,
            process_page_size: 1,
            docker_page_size: 1,
            filter_before_edit: String::new(),
            pending_process: None,
            pending_container: None,
            history: VecDeque::new(),
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
    pub fn record_sample(&mut self, snapshot: &metrics::Snapshot) {
        if self.paused {
            return;
        }
        let cpu = if snapshot.cpu.percent.is_finite() {
            snapshot.cpu.percent.clamp(0.0, 100.0)
        } else {
            0.0
        };
        self.history.push_back(TelemetryPoint {
            cpu_basis_points: (cpu * 100.0).round() as u16,
            network: self.network_rates_available.then_some((
                snapshot.network.recv_bytes_per_sec,
                snapshot.network.sent_bytes_per_sec,
            )),
        });
        while self.history.len() > 120 {
            self.history.pop_front();
        }
    }

    pub fn toggle_docker_focus(&mut self) {
        if self.docker_panel_visible {
            self.docker_focused = !self.docker_focused;
            self.inspector_open = false;
        }
    }
    pub fn toggle_sort(&mut self) {
        self.sort_by_cpu = !self.sort_by_cpu;
        self.process_cursor = 0;
        self.process_offset = 0;
    }
    pub fn toggle_dimmed(&mut self) {
        self.dimmed = !self.dimmed;
    }
    fn confirming(&self) -> bool {
        self.process_kill_confirm || self.docker_confirm.is_some()
    }
    fn message(&mut self, text: impl Into<String>, ok: bool) {
        self.action_message = Some(ActionMessage {
            text: text.into(),
            ok,
        });
    }
    fn cancel_confirmation(&mut self) {
        self.process_kill_confirm = false;
        self.docker_confirm = None;
        self.pending_process = None;
        self.pending_container = None;
    }
}
struct TerminalSession;
impl TerminalSession {
    fn enter() -> anyhow::Result<Self> {
        enable_raw_mode().context("enable raw mode")?;
        let session = Self;
        execute!(io::stdout(), EnterAlternateScreen).context("enter alternate screen")?;
        Ok(session)
    }
}
impl Drop for TerminalSession {
    fn drop(&mut self) {
        disable_raw_mode().ok();
        execute!(io::stdout(), LeaveAlternateScreen, crossterm::cursor::Show).ok();
    }
}
pub fn run_tui() -> anyhow::Result<()> {
    run_tui_with_config(Config::default())
}
pub fn run_tui_with_config(config: Config) -> anyhow::Result<()> {
    let _session = TerminalSession::enter()?;
    let mut terminal = Terminal::new(CrosstermBackend::new(io::stdout()))?;
    run_loop(&mut terminal, TuiState::from_config(&config))
}
fn run_loop<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    mut state: TuiState,
) -> anyhow::Result<()> {
    // A single worker keeps slow telemetry/Docker queries off the input/render thread.
    let (request_tx, request_rx) = mpsc::sync_channel::<()>(1);
    let (result_tx, result_rx) = mpsc::sync_channel(1);
    std::thread::spawn(move || {
        let mut sampler = metrics::LiveSampler::new();
        while request_rx.recv().is_ok() {
            if result_tx
                .send(sampler.collect().map_err(|e| e.to_string()))
                .is_err()
            {
                break;
            }
        }
    });
    let mut snapshot = metrics::Snapshot::default();
    let mut in_flight = false;
    let mut last_request = Instant::now() - Duration::from_secs(1);
    loop {
        if !state.paused && !state.confirming() {
            if let Ok(result) = result_rx.try_recv() {
                in_flight = false;
                match result {
                    Ok(next) => {
                        reconcile_selection(&mut state, &snapshot, &next.snapshot);
                        state.process_cpu_available = next.process_cpu_available;
                        state.network_rates_available = next.network_rates_available;
                        state.record_sample(&next.snapshot);
                        snapshot = next.snapshot;
                        state.last_updated = Some(next.collected_at);
                        state.refresh_error = None;
                    }
                    Err(error) => state.refresh_error = Some(error),
                }
            }
            if !in_flight && last_request.elapsed() >= Duration::from_secs(1) {
                request_tx.try_send(()).ok();
                in_flight = true;
                last_request = Instant::now();
            }
        }
        let report = ops::evaluate_snapshot(&snapshot);
        terminal.draw(|frame| draw_dashboard(frame, &snapshot, &report, &mut state))?;
        if event::poll(Duration::from_millis(80))?
            && let Event::Key(key) = event::read()?
            && handle_key(key, &mut state, &snapshot)?
        {
            break;
        }
    }
    Ok(())
}
fn handle_key(
    key: KeyEvent,
    state: &mut TuiState,
    snapshot: &metrics::Snapshot,
) -> anyhow::Result<bool> {
    if key.kind == KeyEventKind::Release {
        return Ok(false);
    }
    if key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL) {
        return Ok(true);
    }
    if state.filtering {
        match key.code {
            KeyCode::Esc => {
                state.filter = state.filter_before_edit.clone();
                state.filtering = false;
            }
            KeyCode::Enter => state.filtering = false,
            KeyCode::Backspace => {
                state.filter.pop();
            }
            KeyCode::Char(ch)
                if key.modifiers.is_empty() || key.modifiers == KeyModifiers::SHIFT =>
            {
                state.filter.push(ch);
            }
            _ => {}
        }
        state.process_cursor = 0;
        state.process_offset = 0;
        return Ok(false);
    }
    if state.confirming() {
        match key.code {
            KeyCode::Char('y' | 'Y') => {
                if state.process_kill_confirm {
                    confirm_process_kill(state, snapshot);
                } else {
                    confirm_docker_action(state, snapshot);
                }
            }
            KeyCode::Char('n' | 'N' | 'q') | KeyCode::Esc => state.cancel_confirmation(),
            _ => {}
        }
        return Ok(false);
    }
    if state.alerts_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('a' | 'q') => state.alerts_open = false,
            KeyCode::Down | KeyCode::Char('j') => {
                state.alerts_scroll = state.alerts_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.alerts_scroll = state.alerts_scroll.saturating_sub(1)
            }
            _ => {}
        }
        return Ok(false);
    }
    if state.help_open {
        match key.code {
            KeyCode::Esc | KeyCode::Char('?' | 'q') | KeyCode::F(1) => state.help_open = false,
            KeyCode::Down | KeyCode::Char('j') => {
                state.help_scroll = state.help_scroll.saturating_add(1)
            }
            KeyCode::Up | KeyCode::Char('k') => {
                state.help_scroll = state.help_scroll.saturating_sub(1)
            }
            KeyCode::PageDown => state.help_scroll = state.help_scroll.saturating_add(5),
            KeyCode::PageUp => state.help_scroll = state.help_scroll.saturating_sub(5),
            _ => {}
        }
        return Ok(false);
    }
    if state.inspector_open {
        match key.code {
            KeyCode::PageDown => {
                state.inspector_scroll = state.inspector_scroll.saturating_add(5);
                return Ok(false);
            }
            KeyCode::PageUp => {
                state.inspector_scroll = state.inspector_scroll.saturating_sub(5);
                return Ok(false);
            }
            _ => {}
        }
    }
    match key.code {
        KeyCode::Char('q') => return Ok(true),
        KeyCode::Esc => {
            if state.inspector_open {
                state.inspector_open = false;
            } else if !state.filter.is_empty() {
                state.filter.clear();
                state.process_cursor = 0;
                state.process_offset = 0;
            } else {
                state.docker_focused = false;
                state.action_message = None;
            }
        }
        KeyCode::Char('?') | KeyCode::F(1) => {
            state.help_open = true;
            state.help_scroll = 0;
        }
        KeyCode::Char('c') | KeyCode::Tab | KeyCode::BackTab => state.toggle_docker_focus(),
        KeyCode::Char('a') => {
            state.alerts_open = true;
            state.alerts_scroll = 0;
        }
        KeyCode::Char('s') if !state.docker_focused => state.toggle_sort(),
        KeyCode::Char('d') => state.toggle_dimmed(),
        KeyCode::Char(' ') => state.paused = !state.paused,
        KeyCode::Char('/') if !state.docker_focused => {
            state.filter_before_edit = state.filter.clone();
            state.filtering = true;
            state.inspector_open = false;
            state.action_message = None;
        }
        KeyCode::Enter if !state.docker_focused => {
            state.inspector_open = !state.inspector_open;
            state.inspector_scroll = 0;
        }
        KeyCode::Char('K') if !state.docker_focused => {
            if state.paused {
                state.message("Resume updates before sending a signal", false);
            } else if let Some(proc) = selected_process(snapshot, state) {
                state.pending_process = Some(ProcessTarget {
                    pid: proc.pid,
                    name: proc.name.clone(),
                    command: proc.command.clone(),
                });
                state.process_kill_confirm = true;
                state.action_message = None;
            }
        }
        KeyCode::Char('x') if state.docker_focused => request_docker_action(state, snapshot, false),
        KeyCode::Char('r') if state.docker_focused => request_docker_action(state, snapshot, true),
        KeyCode::Down | KeyCode::Char('j') => move_cursor(state, snapshot, 1),
        KeyCode::Up | KeyCode::Char('k') => move_cursor(state, snapshot, -1),
        KeyCode::PageDown | KeyCode::PageUp => {
            let page = if state.docker_focused {
                state.docker_page_size
            } else {
                state.process_page_size
            } as isize;
            move_cursor(
                state,
                snapshot,
                if key.code == KeyCode::PageUp {
                    -page
                } else {
                    page
                },
            );
        }
        KeyCode::Home => move_cursor(state, snapshot, -isize::MAX),
        KeyCode::End => move_cursor(state, snapshot, isize::MAX),
        _ => {}
    }
    Ok(false)
}
fn move_cursor(state: &mut TuiState, snapshot: &metrics::Snapshot, delta: isize) {
    state.inspector_scroll = 0;
    let (cursor, len) = if state.docker_focused {
        (&mut state.docker_cursor, snapshot.docker.containers.len())
    } else {
        let len = filtered_sorted_processes(snapshot, state).len();
        (&mut state.process_cursor, len)
    };
    *cursor = cursor
        .saturating_add_signed(delta)
        .min(len.saturating_sub(1));
}
fn reconcile_selection(
    state: &mut TuiState,
    before: &metrics::Snapshot,
    after: &metrics::Snapshot,
) {
    let pid = selected_process(before, state).map(|p| p.pid);
    let id = selected_container(before, state).map(|c| c.id.as_str());
    let processes = filtered_sorted_processes(after, state);
    state.process_cursor = processes
        .iter()
        .position(|p| Some(p.pid) == pid)
        .unwrap_or(state.process_cursor.min(processes.len().saturating_sub(1)));
    state.docker_cursor = after
        .docker
        .containers
        .iter()
        .position(|c| Some(c.id.as_str()) == id)
        .unwrap_or(
            state
                .docker_cursor
                .min(after.docker.containers.len().saturating_sub(1)),
        );
}
fn request_docker_action(state: &mut TuiState, snapshot: &metrics::Snapshot, restart: bool) {
    if state.paused {
        state.message("Resume updates before changing a container", false);
        return;
    }
    if !state.docker_controls_enabled {
        state.message("Docker controls are read-only", false);
        return;
    }
    if !snapshot.docker.available {
        state.message("Docker is unavailable", false);
        return;
    }
    let Some(container) = selected_container(snapshot, state) else {
        return;
    };
    if restart && !container.running {
        state.message("Only running containers can be restarted", false);
        return;
    }
    state.pending_container = Some(ContainerTarget {
        id: container.id.clone(),
        name: container.name.clone(),
    });
    state.docker_confirm = Some(if restart {
        DockerConfirm::Restart
    } else if container.running {
        DockerConfirm::Stop
    } else {
        DockerConfirm::Start
    });
    state.action_message = None;
}
fn confirm_docker_action(state: &mut TuiState, snapshot: &metrics::Snapshot) {
    if let (Some(action), Some(target)) = (state.docker_confirm, state.pending_container.clone()) {
        if snapshot.docker.containers.iter().any(|c| c.id == target.id)
            && state.docker_controls_enabled
        {
            match docker::docker_action(action.as_str(), &target.id) {
                Ok(_) => state.message(format!("{} {}: done", action.as_str(), target.name), true),
                Err(error) => state.message(
                    format!("{} {}: {error}", action.as_str(), target.name),
                    false,
                ),
            }
        } else {
            state.message("Container is no longer available; action canceled", false);
        }
    }
    state.cancel_confirmation();
}
fn confirm_process_kill(state: &mut TuiState, snapshot: &metrics::Snapshot) {
    if let Some(target) = state.pending_process.clone() {
        if snapshot
            .top_processes
            .iter()
            .any(|p| p.pid == target.pid && p.command == target.command)
        {
            match Command::new("kill")
                .arg("-TERM")
                .arg(target.pid.to_string())
                .output()
            {
                Ok(out) if out.status.success() => state.message(
                    format!("SIGTERM sent to {} ({})", target.name, target.pid),
                    true,
                ),
                Ok(out) => state.message(String::from_utf8_lossy(&out.stderr).trim(), false),
                Err(error) => state.message(error.to_string(), false),
            }
        } else {
            state.message("Process is no longer available; signal canceled", false);
        }
    }
    state.cancel_confirmation();
}
/// Render the real dashboard widgets without collecting host data.
pub fn render_dashboard_text(
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &TuiState,
) -> String {
    render_dashboard_text_size(snapshot, report, state, 120, 36)
}
pub fn render_dashboard_text_size(
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &TuiState,
    width: u16,
    height: u16,
) -> String {
    let mut terminal = Terminal::new(ratatui::backend::TestBackend::new(width, height))
        .expect("in-memory terminal");
    let mut state = state.clone();
    terminal
        .draw(|frame| draw_dashboard(frame, snapshot, report, &mut state))
        .expect("in-memory draw");
    let buffer = terminal.backend().buffer();
    (0..height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < width {
                let symbol = buffer[(x, y)].symbol();
                line.push_str(symbol);
                x += (ratatui::text::Span::raw(symbol).width().max(1) as u16).min(width - x);
            }
            line
        })
        .collect::<Vec<_>>()
        .join("\n")
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
        processes.sort_by(|a, b| {
            b.cpu_percent
                .total_cmp(&a.cpu_percent)
                .then_with(|| a.pid.cmp(&b.pid))
        });
    } else {
        processes.sort_by(|a, b| {
            b.memory_percent
                .total_cmp(&a.memory_percent)
                .then_with(|| a.pid.cmp(&b.pid))
        });
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

#[cfg(test)]
mod tests;

use super::*;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Table, TableState, Wrap},
};

const BACKGROUND: Color = Color::Rgb(12, 13, 20);
const SURFACE: Color = Color::Rgb(23, 25, 37);
const TEXT: Color = Color::Rgb(224, 226, 238);
const MUTED: Color = Color::Rgb(145, 151, 174);
const BORDER: Color = Color::Rgb(64, 69, 91);
const PINK: Color = Color::Rgb(244, 109, 179);
const LIME: Color = Color::Rgb(197, 233, 113);
const CYAN: Color = Color::Rgb(113, 207, 220);
const AMBER: Color = Color::Rgb(244, 192, 106);
const RED: Color = Color::Rgb(255, 120, 136);

fn label(text: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}
fn muted(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(MUTED))
}
fn panel(title: impl Into<Line<'static>>, focused: bool) -> Block<'static> {
    Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(if focused { PINK } else { BORDER }))
        .title(title)
        .title_style(
            Style::default()
                .fg(if focused { PINK } else { MUTED })
                .add_modifier(Modifier::BOLD),
        )
}
fn severity(report: &ops::Report) -> Color {
    match report.status {
        ops::Status::Clear => LIME,
        ops::Status::Watch => CYAN,
        ops::Status::Warn => AMBER,
        ops::Status::Crit => RED,
    }
}
fn track(text: &str) -> String {
    text.chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
fn title(name: &str, width: u16) -> String {
    format!(
        " {} ",
        if width >= 68 {
            track(name)
        } else {
            name.to_string()
        }
    )
}

pub fn draw(
    frame: &mut ratatui::Frame<'_>,
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &mut TuiState,
) {
    let area = frame.area();
    let base = Style::default().bg(BACKGROUND).fg(TEXT);
    frame.render_widget(
        Block::default().style(if state.dimmed {
            base.add_modifier(Modifier::DIM)
        } else {
            base
        }),
        area,
    );
    if area.width < 50 || area.height < 12 {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(label("DURANDAL", PINK)),
                Line::from("Resize to at least 50 × 12"),
                Line::from("q quit · ? help"),
            ]),
            area,
        );
        return;
    }
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(if area.height < 18 { 3 } else { 4 }),
        Constraint::Min(4),
        Constraint::Length(2),
    ])
    .split(area);
    draw_header(frame, sections[0], snapshot, state);
    draw_health(frame, sections[1], report, state);
    let body = sections[2];
    let wide = area.width >= 100 && area.height >= 24;
    if wide {
        let columns = Layout::horizontal([Constraint::Percentage(34), Constraint::Percentage(66)])
            .split(body);
        draw_resources(frame, columns[0], snapshot, state);
        draw_workload(frame, columns[1], snapshot, state);
    } else {
        let parts = Layout::vertical([
            Constraint::Length(if area.height < 18 { 1 } else { 2 }),
            Constraint::Min(3),
        ])
        .split(body);
        draw_summary(frame, parts[0], snapshot);
        if state.docker_focused && state.docker_panel_visible {
            draw_docker(frame, parts[1], snapshot, state);
        } else {
            draw_processes(frame, parts[1], snapshot, state);
        }
    }
    draw_footer(frame, sections[3], snapshot, state);
    if state.inspector_open {
        draw_inspector(frame, area, snapshot, state);
    }
    if state.alerts_open {
        draw_alerts(frame, area, report, state);
    }
    if state.help_open {
        draw_help(frame, area, state);
    }
    if state.confirming() {
        draw_confirmation(frame, area, snapshot, state);
    }
}

fn draw_header(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &TuiState,
) {
    let split = Layout::horizontal([
        Constraint::Length(if area.width >= 90 { 34 } else { 15 }),
        Constraint::Min(0),
    ])
    .split(area);
    let mut brand = vec![label(" █ DURANDAL ", PINK)];
    if area.width >= 90 {
        brand.push(muted(" SYSTEMS MONITOR"));
    }
    frame.render_widget(Paragraph::new(Line::from(brand)), split[0]);
    let host = if snapshot.host.hostname.is_empty() {
        "connecting"
    } else {
        &snapshot.host.hostname
    };
    let age = if state.paused {
        "PAUSED".to_string()
    } else if let Some(time) = state.last_updated {
        format!("LIVE · {}s ago", time.elapsed().as_secs())
    } else {
        "LOADING".to_string()
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            muted(format!("{host} · ")),
            label(age, if state.paused { AMBER } else { CYAN }),
        ]))
        .right_aligned(),
        split[1],
    );
}
fn draw_health(frame: &mut ratatui::Frame<'_>, area: Rect, report: &ops::Report, state: &TuiState) {
    let block = panel(title("LAPIS SENTINEL", area.width), false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let loading = state.last_updated.is_none();
    let score = if loading {
        "--- /100  COLLECTING".into()
    } else {
        format!("{:03} /100  {}", report.score, report.status.as_str())
    };
    let first = Line::from(vec![
        label(score, if loading { MUTED } else { severity(report) }),
        muted("   OPS READINESS"),
    ]);
    let message = if let Some(error) = &state.refresh_error {
        if loading {
            format!("Telemetry unavailable: {error}")
        } else {
            format!("Refresh failed; retaining last sample: {error}")
        }
    } else if loading {
        "Waiting for the first system sample. Navigation remains available.".into()
    } else {
        report
            .alerts
            .first()
            .map(|a| a.message.clone())
            .unwrap_or_else(|| "No pressure signals".into())
    };
    let extra = report.alerts.len().saturating_sub(1);
    let message = if extra > 0 && !loading {
        format!("{message} · +{extra} signals · a details")
    } else {
        message
    };
    frame.render_widget(
        Paragraph::new(vec![first, Line::from(muted(message))]),
        inner,
    );
}

fn meter(percent: f64, width: u16, color: Color) -> Line<'static> {
    let size = usize::from(width.saturating_sub(8));
    let percent = if percent.is_finite() {
        percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    let filled = (percent / 100.0 * size as f64).round() as usize;
    Line::from(vec![
        Span::styled(
            "█".repeat(filled),
            Style::default().fg(if percent >= 90.0 {
                RED
            } else if percent >= 75.0 {
                AMBER
            } else {
                color
            }),
        ),
        Span::styled(
            "░".repeat(size.saturating_sub(filled)),
            Style::default().fg(BORDER),
        ),
        label(format!(" {percent:>5.1}%"), TEXT),
    ])
}
fn draw_resources(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &TuiState,
) {
    let block = panel(" RESOURCE OVERVIEW ", false);
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let roomy = inner.height >= 24;
    let mut lines = vec![Line::from(label("C P U", LIME))];
    if roomy {
        lines.push(Line::from(muted(snapshot.cpu.model.clone())));
    }
    lines.push(meter(snapshot.cpu.percent, inner.width, LIME));
    lines.push(Line::from(muted(format!(
        "{} cores · {} threads",
        snapshot.cpu.cores, snapshot.cpu.threads
    ))));
    lines.push(Line::default());
    lines.push(Line::from(vec![
        label("M E M O R Y", CYAN),
        muted(format!(
            "  {} / {}",
            format_bytes(snapshot.memory.used_bytes),
            format_bytes(snapshot.memory.total_bytes)
        )),
    ]));
    lines.push(meter(snapshot.memory.percent, inner.width, CYAN));
    if snapshot.swap.total_bytes == 0 {
        lines.push(Line::from(muted("Swap not configured")));
    } else {
        lines.push(Line::from(muted(format!(
            "Swap {} / {}",
            format_bytes(snapshot.swap.used_bytes),
            format_bytes(snapshot.swap.total_bytes)
        ))));
        if roomy {
            lines.push(meter(snapshot.swap.percent, inner.width, CYAN));
        }
    }
    if roomy {
        lines.push(Line::from(muted(format!(
            "Cache {} · Buffers {}",
            format_bytes(snapshot.memory.cached_bytes),
            format_bytes(snapshot.memory.buffers_bytes)
        ))));
    }
    lines.push(Line::default());
    lines.push(Line::from(label("S T O R A G E", AMBER)));
    if snapshot.disks.is_empty() {
        lines.push(Line::from(muted("No mounted disks reported")));
    }
    for disk in snapshot.disks.iter().take(if roomy { 3 } else { 1 }) {
        lines.push(Line::from(vec![
            label(format!("{} ", disk.mountpoint), TEXT),
            muted(format!(
                "{} / {}",
                format_bytes(disk.used_bytes),
                format_bytes(disk.total_bytes)
            )),
        ]));
        lines.push(meter(disk.percent, inner.width, AMBER));
    }
    lines.push(Line::default());
    lines.push(Line::from(label("N E T W O R K", PINK)));
    if state.network_rates_available {
        lines.push(Line::from(vec![
            label("▼ DOWN ", LIME),
            muted(format!(
                "{}/s",
                format_bytes(snapshot.network.recv_bytes_per_sec)
            )),
        ]));
        lines.push(Line::from(vec![
            label("▲ UP   ", PINK),
            muted(format!(
                "{}/s",
                format_bytes(snapshot.network.sent_bytes_per_sec)
            )),
        ]));
    } else {
        lines.push(Line::from(muted("▼ DOWN / ▲ UP · waiting for sample")));
    }
    if !snapshot.gpus.is_empty() && roomy {
        lines.push(Line::default());
        lines.push(Line::from(label("G P U", LIME)));
        for gpu in snapshot.gpus.iter().take(1) {
            lines.push(Line::from(muted(gpu.name.clone())));
            lines.push(Line::from(muted("Device detected · sensors unavailable")));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}
fn draw_summary(frame: &mut ratatui::Frame<'_>, area: Rect, snapshot: &metrics::Snapshot) {
    let storage = snapshot
        .disks
        .first()
        .map(|d| format!(" · Disk {:.0}%", d.percent))
        .unwrap_or_default();
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(vec![
                label(format!(" CPU {:.1}%", snapshot.cpu.percent), LIME),
                label(format!(" · RAM {:.1}%", snapshot.memory.percent), CYAN),
                muted(storage),
            ]),
            Line::from(muted(" Tab switch panels · Space pause · ? all shortcuts")),
        ]),
        area,
    );
}
fn draw_workload(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &mut TuiState,
) {
    if !state.docker_panel_visible {
        draw_processes(frame, area, snapshot, state);
        return;
    }
    let docker_height = if !snapshot.docker.available {
        5
    } else {
        (area.height / 3).max(7)
    };
    let sections =
        Layout::vertical([Constraint::Min(5), Constraint::Length(docker_height)]).split(area);
    draw_processes(frame, sections[0], snapshot, state);
    draw_docker(frame, sections[1], snapshot, state);
}
fn selection_style(focused: bool) -> Style {
    Style::default()
        .bg(SURFACE)
        .fg(if focused { LIME } else { TEXT })
        .add_modifier(if focused {
            Modifier::BOLD
        } else {
            Modifier::empty()
        })
}
fn draw_processes(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &mut TuiState,
) {
    let processes = filtered_sorted_processes(snapshot, state);
    let count = processes.len();
    let focused = !state.docker_focused;
    let bottom = if state.filter.is_empty() {
        format!(
            " {} PROCS · {} {} ",
            count,
            if state.sort_by_cpu {
                "CPU▼"
            } else {
                "MEM▼"
            },
            if focused { "ACTIVE" } else { "IDLE" }
        )
    } else {
        format!(
            " {count}/{} PROCS · FILTER:/{} ",
            snapshot.top_processes.len(),
            state.filter
        )
    };
    let block = panel(title("PROCESSES", area.width), focused)
        .title_bottom(Line::from(bottom).style(Style::default().fg(MUTED)));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let rows = if state.filtering {
        let parts = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).split(inner);
        let search = Line::from(vec![
            label(" / ", PINK),
            Span::raw(state.filter.clone()),
            muted("  · Enter apply · Esc cancel"),
        ]);
        let cursor = (3 + Line::from(state.filter.as_str()).width())
            .min(usize::from(parts[0].width.saturating_sub(1))) as u16;
        frame.render_widget(Paragraph::new(search), parts[0]);
        if parts[0].width > 0 {
            frame.set_cursor_position((parts[0].x + cursor, parts[0].y));
        }
        parts[1]
    } else {
        inner
    };
    if count == 0 {
        let message = if !state.filter.is_empty() {
            "No matching processes. Esc clears the filter; / edits it."
        } else if state.last_updated.is_none() {
            "Collecting processes…"
        } else {
            "No processes reported."
        };
        frame.render_widget(Paragraph::new(message).wrap(Wrap { trim: true }), rows);
        return;
    }
    let rss = rows.width >= 65;
    let user = rows.width >= 85;
    let headers = [
        "PID",
        "COMMAND",
        if state.sort_by_cpu { "CPU▼" } else { "CPU%" },
        if state.sort_by_cpu { "MEM%" } else { "MEM▼" },
    ];
    let mut names: Vec<Cell> = headers.into_iter().map(Cell::from).collect();
    let mut widths = vec![
        Constraint::Length(7),
        Constraint::Min(10),
        Constraint::Length(6),
        Constraint::Length(6),
    ];
    if rss {
        names.push(Cell::from("RSS"));
        widths.push(Constraint::Length(9));
    }
    if user {
        names.push(Cell::from("USER"));
        widths.push(Constraint::Length(9));
    }
    let items = processes.iter().map(|p| {
        let mut cells = vec![
            Cell::from(p.pid.to_string()),
            Cell::from(if p.command.is_empty() {
                p.name.clone()
            } else {
                p.command.clone()
            }),
            Cell::from(if state.process_cpu_available {
                format!("{:>5.1}", p.cpu_percent)
            } else {
                "    —".into()
            }),
            Cell::from(format!("{:>5.1}", p.memory_percent)),
        ];
        if rss {
            cells.push(Cell::from(format_bytes(p.rss_bytes)));
        }
        if user {
            cells.push(Cell::from(p.user.clone()));
        }
        Row::new(cells)
    });
    state.process_cursor = state.process_cursor.min(count.saturating_sub(1));
    state.process_page_size = usize::from(rows.height.saturating_sub(1)).max(1);
    state.process_offset = visible_offset(
        state.process_cursor,
        state.process_offset,
        state.process_page_size,
    );
    let mut table_state = TableState::default()
        .with_selected(state.process_cursor)
        .with_offset(state.process_offset);
    let table = Table::new(items, widths)
        .column_spacing(1)
        .header(Row::new(names).style(Style::default().fg(MUTED)))
        .row_highlight_style(selection_style(focused))
        .highlight_symbol(if focused { "› " } else { "  " });
    frame.render_stateful_widget(table, rows, &mut table_state);
    state.process_offset = table_state.offset();
}
fn draw_docker(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &mut TuiState,
) {
    let running = snapshot
        .docker
        .containers
        .iter()
        .filter(|c| c.running)
        .count();
    let total = snapshot.docker.containers.len();
    let block = panel(title("DOCKER", area.width), state.docker_focused).title_bottom(
        Line::from(format!(
            " {total} CONTAINERS · {running} RUNNING · {} ",
            if state.docker_focused {
                "ACTIVE"
            } else {
                "IDLE"
            }
        ))
        .style(Style::default().fg(MUTED)),
    );
    let inner = block.inner(area);
    frame.render_widget(block, area);
    if !snapshot.docker.available {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(label("OFFLINE", AMBER)),
                Line::from(muted(
                    snapshot
                        .docker
                        .error
                        .clone()
                        .unwrap_or_else(|| "Docker is not available on this host".into()),
                )),
            ])
            .wrap(Wrap { trim: true }),
            inner,
        );
        return;
    }
    if total == 0 {
        frame.render_widget(
            Paragraph::new("No local containers. Docker is connected."),
            inner,
        );
        return;
    }
    let image = inner.width >= 65;
    let mut widths = vec![Constraint::Length(9), Constraint::Min(12)];
    let mut headers = vec!["STATE", "NAME"];
    if image {
        widths.push(Constraint::Percentage(35));
        headers.push("IMAGE");
    }
    let items = snapshot.docker.containers.iter().map(|c| {
        let mut cells = vec![
            Cell::from(if c.running {
                "RUN".into()
            } else {
                c.state.to_uppercase()
            })
            .style(Style::default().fg(if c.running { LIME } else { MUTED })),
            Cell::from(c.name.clone()),
        ];
        if image {
            cells.push(Cell::from(c.image.clone()));
        }
        Row::new(cells)
    });
    state.docker_cursor = state.docker_cursor.min(total.saturating_sub(1));
    state.docker_page_size = usize::from(inner.height.saturating_sub(1)).max(1);
    state.docker_offset = visible_offset(
        state.docker_cursor,
        state.docker_offset,
        state.docker_page_size,
    );
    let mut table_state = TableState::default()
        .with_selected(state.docker_cursor)
        .with_offset(state.docker_offset);
    frame.render_stateful_widget(
        Table::new(items, widths)
            .column_spacing(1)
            .header(Row::new(headers).style(Style::default().fg(MUTED)))
            .row_highlight_style(selection_style(state.docker_focused))
            .highlight_symbol(if state.docker_focused { "› " } else { "  " }),
        inner,
        &mut table_state,
    );
    state.docker_offset = table_state.offset();
}
fn draw_footer(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &TuiState,
) {
    let status = if let Some(message) = &state.action_message {
        Line::from(label(
            message.text.clone(),
            if message.ok { LIME } else { AMBER },
        ))
    } else if state.paused {
        Line::from(label(" PAUSED · Space resumes updates", AMBER))
    } else if state.docker_focused {
        let detail = selected_container(snapshot, state)
            .map(|c| format!("{} · {}", c.name, c.status))
            .unwrap_or_else(|| "Container station".into());
        Line::from(muted(format!(
            " {detail}{}",
            if state.docker_controls_enabled {
                ""
            } else {
                " · read-only"
            }
        )))
    } else {
        Line::from(muted(format!(
            " {} · {}",
            if state.sort_by_cpu {
                "Sorted by CPU"
            } else {
                "Sorted by memory"
            },
            if state.last_updated.is_some() {
                "refresh 1s"
            } else {
                "waiting for telemetry"
            }
        )))
    };
    let hints = if state.filtering {
        " Enter apply  Esc cancel  Ctrl+C quit"
    } else if state.confirming() {
        " Y confirm  N / Esc cancel"
    } else if area.width < 75 {
        " q quit  ? help  Tab panel  ↑/k ↓/j move"
    } else if state.docker_focused
        && state.docker_controls_enabled
        && snapshot.docker.available
        && !snapshot.docker.containers.is_empty()
    {
        " q quit  ? help  Tab processes  ↑/k ↓/j move  x start/stop  r restart  Space pause"
    } else if state.docker_focused {
        " q quit  ? help  Tab processes  ↑/k ↓/j move  Space pause"
    } else {
        " q quit  ? help  a alerts  Tab Docker  ↑/k ↓/j move  s SORT  / find  Enter inspect  Space pause"
    };
    frame.render_widget(Paragraph::new(vec![status, Line::from(muted(hints))]), area);
}
fn modal(area: Rect, width: u16, height: u16) -> Rect {
    let width = width.min(area.width.saturating_sub(2));
    let height = height.min(area.height.saturating_sub(2));
    Rect::new(
        area.x + (area.width - width) / 2,
        area.y + (area.height - height) / 2,
        width,
        height,
    )
}
fn draw_inspector(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &mut TuiState,
) {
    let area = modal(area, 78, 18);
    frame.render_widget(Clear, area);
    let block = panel(" I N S P E C T O R · Esc close ", true)
        .title_bottom(" PgUp/PgDn scroll · ↑/↓ select ")
        .style(Style::default().bg(SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let Some(p) = selected_process(snapshot, state) else {
        frame.render_widget(Paragraph::new("No process selected"), inner);
        return;
    };
    let lines = vec![
        Line::from(label(format!("{} · PID {}", p.name, p.pid), LIME)),
        Line::from(muted(format!("User {} · State {}", p.user, p.status))),
        Line::default(),
        Line::from(format!(
            "CPU {}   Memory {:.1}%   RSS {}",
            if state.process_cpu_available {
                format!("{:.1}%", p.cpu_percent)
            } else {
                "awaiting sample".into()
            },
            p.memory_percent,
            format_bytes(p.rss_bytes)
        )),
        Line::default(),
        Line::from(label("COMMAND", PINK)),
        Line::from(p.command.clone()),
        Line::default(),
        Line::from(muted(
            "↑/↓ browse processes · K request SIGTERM · Esc close",
        )),
    ];
    let lines: Vec<_> = lines
        .into_iter()
        .flat_map(|line| {
            if line.spans.len() == 1 && line.spans[0].style == Style::default() {
                wrap_message(&line.spans[0].content, inner.width)
                    .into_iter()
                    .map(Line::from)
                    .collect::<Vec<_>>()
            } else {
                vec![line]
            }
        })
        .collect();
    state.inspector_scroll = state
        .inspector_scroll
        .min((lines.len() as u16).saturating_sub(inner.height));
    frame.render_widget(
        Paragraph::new(lines).scroll((state.inspector_scroll, 0)),
        inner,
    );
}
fn draw_help(frame: &mut ratatui::Frame<'_>, area: Rect, state: &mut TuiState) {
    let area = modal(area, 76, 23);
    frame.render_widget(Clear, area);
    let block = panel(" KEYBOARD · ? / Esc close ", true)
        .title_bottom(" ↑/↓ scroll ")
        .style(Style::default().bg(SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let lines = [
        "NAVIGATION",
        "Tab / Shift+Tab / c  Switch panel",
        "a                   Health signals",
        "↑/k  ↓/j            Move selection",
        "PgUp / PgDn         Page / scroll details",
        "Home / End          First / last row",
        "",
        "PROCESSES",
        "s                   Sort CPU / memory",
        "/                   Find name, command, PID",
        "Enter               Inspect process",
        "Esc                 Close / clear search",
        "K                   Request SIGTERM",
        "",
        "DOCKER (CONFIRM BEFORE EXECUTION)",
        "x                   Start / stop container",
        "r                   Restart container",
        "",
        "Space               Pause / resume updates",
        "d                   Dim colors",
        "q / Ctrl+C          Quit; q closes dialogs",
    ];
    state.help_scroll = state
        .help_scroll
        .min((lines.len() as u16).saturating_sub(inner.height));
    frame.render_widget(
        Paragraph::new(
            lines
                .into_iter()
                .map(|s| {
                    if matches!(s, "NAVIGATION" | "PROCESSES" | "DOCKER") {
                        Line::from(label(s, PINK))
                    } else {
                        Line::from(s)
                    }
                })
                .collect::<Vec<_>>(),
        )
        .scroll((state.help_scroll, 0)),
        inner,
    );
}
fn draw_confirmation(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &TuiState,
) {
    let area = modal(area, 74, 10);
    frame.render_widget(Clear, area);
    let block = panel(" CONFIRM ACTION ", true)
        .border_style(Style::default().fg(AMBER))
        .style(Style::default().bg(SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let message = if state.process_kill_confirm {
        let target = state
            .pending_process
            .as_ref()
            .map(|p| format!("{} ({})", p.pid, p.name))
            .or_else(|| {
                selected_process(snapshot, state).map(|p| format!("{} ({})", p.pid, p.name))
            })
            .unwrap_or_default();
        format!("KILL {target}? Send SIGTERM to this process.")
    } else {
        let action = state
            .docker_confirm
            .map(|a| a.as_str().to_uppercase())
            .unwrap_or_default();
        let target = state
            .pending_container
            .as_ref()
            .map(|c| c.name.as_str())
            .or_else(|| selected_container(snapshot, state).map(|c| c.name.as_str()))
            .unwrap_or_default();
        format!("{action} {target}? [Y]ES [N]O")
    };
    frame.render_widget(
        Paragraph::new(vec![
            Line::from(label(message, AMBER)),
            Line::default(),
            Line::from("The selected target stays fixed while this dialog is open."),
            Line::default(),
            Line::from(label("Y confirm · N / Esc cancel", TEXT)),
        ])
        .wrap(Wrap { trim: true }),
        inner,
    );
}

fn draw_alerts(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    report: &ops::Report,
    state: &mut TuiState,
) {
    let area = modal(area, 82, 20);
    frame.render_widget(Clear, area);
    let block = panel(" SENTINEL SIGNALS · a / Esc close ", true)
        .title_bottom(" ↑/↓ scroll ")
        .style(Style::default().bg(SURFACE));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    let mut lines = Vec::new();
    for alert in &report.alerts {
        let color = match alert.severity {
            ops::Severity::Info => LIME,
            ops::Severity::Watch => CYAN,
            ops::Severity::Warning => AMBER,
            ops::Severity::Critical => RED,
        };
        lines.push(Line::from(label(
            format!(
                "{} · {}",
                alert.label,
                alert.severity.as_str().to_uppercase()
            ),
            color,
        )));
        lines.extend(
            wrap_message(&alert.message, inner.width)
                .into_iter()
                .map(Line::from),
        );
        lines.push(Line::default());
    }
    if state.last_updated.is_none() {
        lines = vec![Line::from("Waiting for the first system sample.")];
    }
    let count = lines.len() as u16;
    let paragraph = Paragraph::new(lines);
    state.alerts_scroll = state.alerts_scroll.min(count.saturating_sub(inner.height));
    frame.render_widget(paragraph.scroll((state.alerts_scroll, 0)), inner);
}

fn wrap_message(text: &str, width: u16) -> Vec<String> {
    let width = usize::from(width.max(1));
    let mut result = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        if !current.is_empty() {
            if Line::from(format!("{current} {word}")).width() <= width {
                current.push(' ');
            } else {
                result.push(std::mem::take(&mut current));
            }
        }
        for ch in word.chars() {
            let candidate = format!("{current}{ch}");
            if !current.is_empty() && Line::from(candidate).width() > width {
                result.push(std::mem::take(&mut current));
            }
            current.push(ch);
        }
    }
    if !current.is_empty() {
        result.push(current);
    }
    result
}

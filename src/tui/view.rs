use super::*;
use ratatui::{
    layout::{Constraint, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Cell, Clear, Paragraph, Row, Sparkline, Table, TableState, Wrap},
};

// Canonical palette: Blumenwagen/durandal, internal/styles/styles.go.
const BACKGROUND: Color = Color::Rgb(10, 10, 10);
const SURFACE: Color = Color::Rgb(18, 18, 31);
const TEXT: Color = Color::Rgb(216, 216, 232);
const MUTED: Color = Color::Rgb(138, 138, 158);
const BORDER: Color = Color::Rgb(74, 74, 90);
const PINK: Color = Color::Rgb(255, 0, 127);
const LIME: Color = Color::Rgb(191, 255, 0);
const CYAN: Color = Color::Rgb(0, 240, 255);
const AMBER: Color = Color::Rgb(255, 184, 0);
const RED: Color = Color::Rgb(255, 34, 68);

fn label(text: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default().fg(color).add_modifier(Modifier::BOLD),
    )
}
fn muted(text: impl Into<String>) -> Span<'static> {
    Span::styled(text.into(), Style::default().fg(MUTED))
}
fn reverse(text: impl Into<String>, color: Color) -> Span<'static> {
    Span::styled(
        text.into(),
        Style::default()
            .fg(BACKGROUND)
            .bg(color)
            .add_modifier(Modifier::BOLD),
    )
}
// Dialogs retain a closed frame; the dashboard uses Go's open magazine panels.
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
fn usage_color(percent: f64) -> Color {
    if percent >= 90.0 {
        RED
    } else if percent >= 70.0 {
        AMBER
    } else if percent >= 40.0 {
        LIME
    } else {
        CYAN
    }
}
fn track(text: &str) -> String {
    text.chars()
        .map(|c| c.to_string())
        .collect::<Vec<_>>()
        .join(" ")
}
fn title(name: &str, width: u16) -> String {
    let tracked = track(name);
    format!(
        " {} ",
        if Line::from(tracked.as_str()).width() + 2 <= usize::from(width) {
            tracked
        } else {
            name.to_string()
        }
    )
}
/// Native-widget version of Go's MagPanel: reversed label, upper-half rule,
/// left accent stripe and quiet bottom rule. No enclosing rectangle.
fn mag_panel(frame: &mut ratatui::Frame<'_>, area: Rect, name: &str, color: Color) -> Rect {
    if area.is_empty() {
        return area;
    }
    let heading = title(name, area.width);
    let fill = usize::from(area.width).saturating_sub(Line::from(heading.as_str()).width());
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            reverse(heading, color),
            Span::styled("▀".repeat(fill), Style::default().fg(color)),
        ])),
        Rect::new(area.x, area.y, area.width, 1),
    );
    if area.height < 2 {
        return Rect::new(area.x, area.y, 0, 0);
    }
    let height = area.height.saturating_sub(2);
    for y in area.y + 1..area.y + 1 + height {
        frame.render_widget(
            Paragraph::new(Span::styled("▎", Style::default().fg(color))),
            Rect::new(area.x, y, 1, 1),
        );
    }
    frame.render_widget(
        Paragraph::new(Span::styled(
            "─".repeat(usize::from(area.width)),
            Style::default().fg(BORDER),
        )),
        Rect::new(area.x, area.bottom() - 1, area.width, 1),
    );
    Rect::new(
        area.x.saturating_add(2),
        area.y + 1,
        area.width.saturating_sub(2),
        height,
    )
}

pub fn draw(
    frame: &mut ratatui::Frame<'_>,
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &mut TuiState,
) {
    draw_frame(frame, snapshot, report, state);
    if state.dimmed {
        // Go dims the three accents, retaining readable neutral text.
        let dim = |color| match color {
            LIME => Color::Rgb(111, 143, 0),
            PINK => Color::Rgb(143, 0, 63),
            CYAN => Color::Rgb(0, 127, 143),
            _ => color,
        };
        for cell in &mut frame.buffer_mut().content {
            cell.fg = dim(cell.fg);
            cell.bg = dim(cell.bg);
        }
    }
}
fn draw_frame(
    frame: &mut ratatui::Frame<'_>,
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &mut TuiState,
) {
    let area = frame.area();
    frame.render_widget(
        Block::default().style(Style::default().bg(BACKGROUND).fg(TEXT)),
        area,
    );
    if area.width < 50 || area.height < 12 {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(reverse(" DURANDAL ", LIME)),
                Line::from("Resize to at least 50 × 12"),
                Line::from("q quit · ? help"),
            ]),
            area,
        );
        return;
    }
    let sections = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(4),
        Constraint::Length(if state.action_message.is_some() { 2 } else { 1 }),
    ])
    .split(area);
    draw_header(frame, sections[0], snapshot, state);
    if area.width >= 100 && area.height >= 24 {
        let columns = Layout::horizontal([Constraint::Percentage(35), Constraint::Percentage(65)])
            .split(sections[1]);
        draw_resources(frame, columns[0], snapshot, report, state);
        draw_workload(frame, columns[1], snapshot, state);
    } else {
        let parts = Layout::vertical([
            Constraint::Length(if area.height < 18 { 3 } else { 4 }),
            Constraint::Length(if area.height < 18 { 1 } else { 2 }),
            Constraint::Min(3),
        ])
        .split(sections[1]);
        draw_health(frame, parts[0], report, state);
        draw_summary(frame, parts[1], snapshot);
        if state.docker_focused && state.docker_panel_visible {
            draw_docker(frame, parts[2], snapshot, state);
        } else {
            draw_processes(frame, parts[2], snapshot, state);
        }
    }
    draw_footer(frame, sections[2], snapshot, state);
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
    frame.render_widget(Block::default().style(Style::default().bg(SURFACE)), area);
    let brand_width = if area.width >= 90 { 29 } else { 13 };
    let parts =
        Layout::horizontal([Constraint::Length(brand_width), Constraint::Min(0)]).split(area);
    let mut brand = vec![reverse(" █ DURANDAL ", LIME)];
    if area.width >= 90 {
        brand.push(muted(" SYSTEMS MONITOR"));
    }
    frame.render_widget(Paragraph::new(Line::from(brand)), parts[0]);
    let user = if snapshot.host.user.is_empty() {
        "system"
    } else {
        &snapshot.host.user
    };
    let host = if snapshot.host.hostname.is_empty() {
        "connecting"
    } else {
        &snapshot.host.hostname
    };
    let mut info = vec![label(user, LIME), muted("@"), label(host, TEXT)];
    if state.paused {
        info.extend([muted("  //  "), label("PAUSED", AMBER)]);
    } else if !snapshot.host.uptime.is_empty() {
        info.extend([
            muted("  //  "),
            Span::styled(snapshot.host.uptime.clone(), Style::default().fg(CYAN)),
        ]);
    }
    frame.render_widget(Paragraph::new(Line::from(info)).right_aligned(), parts[1]);
}
fn draw_health(frame: &mut ratatui::Frame<'_>, area: Rect, report: &ops::Report, state: &TuiState) {
    let loading = state.last_updated.is_none();
    let color = if loading { MUTED } else { severity(report) };
    let inner = mag_panel(frame, area, "LAPIS SENTINEL", color);
    let mut lines = vec![Line::from(vec![
        label(
            if loading {
                "---".into()
            } else {
                format!("{:03}", report.score)
            },
            color,
        ),
        muted(" /100 "),
        reverse(
            format!(
                " {} ",
                if loading {
                    "WAIT"
                } else {
                    report.status.as_str()
                }
            ),
            color,
        ),
        muted(" OPS READINESS"),
    ])];
    if inner.height >= 3 {
        lines.push(Line::from(Span::styled(
            "─".repeat(usize::from(inner.width)),
            Style::default().fg(BORDER),
        )));
    }
    if let Some(error) = &state.refresh_error {
        lines.push(Line::from(label(
            format!("Telemetry unavailable: {error}"),
            AMBER,
        )));
    } else if loading {
        lines.push(Line::from(muted("Awaiting telemetry")));
    } else {
        let room = usize::from(inner.height).saturating_sub(lines.len());
        for alert in report.alerts.iter().take(room.max(1)) {
            lines.push(Line::from(vec![
                label(alert.label.clone(), color),
                muted(" → "),
                Span::raw(alert.message.clone()),
            ]));
        }
    }
    frame.render_widget(Paragraph::new(lines), inner);
}
fn meter(percent: f64, width: u16, color: Color) -> Line<'static> {
    let percent = if percent.is_finite() {
        percent.clamp(0.0, 100.0)
    } else {
        0.0
    };
    let size = usize::from(width.saturating_sub(5));
    let fill = (percent / 100.0 * size as f64) as usize;
    Line::from(vec![
        Span::styled("█".repeat(fill), Style::default().fg(color)),
        Span::styled(
            "░".repeat(size.saturating_sub(fill)),
            Style::default().fg(BORDER),
        ),
        label(format!(" {percent:>3.0}%"), TEXT),
    ])
}
fn compact_bytes(bytes: u64) -> String {
    let units = ["B", "K", "M", "G", "T"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < units.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes}B")
    } else {
        format!("{value:.1}{}", units[unit])
    }
}
fn draw_resources(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    report: &ops::Report,
    state: &TuiState,
) {
    let gpu = if !snapshot.gpus.is_empty() && area.height >= 30 {
        4
    } else {
        0
    };
    let sentinel = if area.height >= 28 { 5 } else { 4 };
    let cpu = (area.height * 22 / 100).max(4);
    let memory = if area.height >= 28 { 7 } else { 6 };
    let network = if area.height >= 28 { 6 } else { 4 };
    let parts = Layout::vertical([
        Constraint::Length(sentinel),
        Constraint::Length(cpu),
        Constraint::Length(gpu),
        Constraint::Length(memory),
        Constraint::Length(network),
        Constraint::Min(4),
    ])
    .split(area);
    draw_health(frame, parts[0], report, state);
    draw_cpu(frame, parts[1], snapshot, state);
    if gpu > 0 {
        draw_gpu(frame, parts[2], snapshot);
    }
    draw_memory(frame, parts[3], snapshot);
    draw_network(frame, parts[4], snapshot, state);
    draw_storage(frame, parts[5], snapshot);
}
fn plot(frame: &mut ratatui::Frame<'_>, area: Rect, values: &[u64], max: u64, color: Color) {
    if area.is_empty() {
        return;
    }
    // Like Go's right-aligned history: unobserved time stays blank.
    let size = usize::from(area.width);
    let start = values.len().saturating_sub(size);
    let mut data = vec![0; size.saturating_sub(values.len())];
    data.extend_from_slice(&values[start..]);
    frame.render_widget(
        Sparkline::default()
            .data(&data)
            .max(max.max(1))
            .style(Style::default().fg(color)),
        area,
    );
}
fn draw_cpu(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &TuiState,
) {
    let inner = mag_panel(frame, area, "CPU", LIME);
    if inner.is_empty() {
        return;
    }
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label(
                format!("{:.1}%", snapshot.cpu.percent),
                usage_color(snapshot.cpu.percent),
            ),
            muted(format!("  {} THREADS", snapshot.cpu.threads)),
        ])),
        Rect::new(inner.x, inner.y, inner.width, 1),
    );
    let core_rows = if inner.height >= 4 && !snapshot.cpu.per_core.is_empty() {
        (area.height / 3).max(1).min(inner.height.saturating_sub(2))
    } else {
        0
    };
    let chart = Rect::new(
        inner.x,
        inner.y + 1,
        inner.width,
        inner.height.saturating_sub(1 + core_rows),
    );
    let history: Vec<_> = state
        .history
        .iter()
        .map(|p| u64::from(p.cpu_basis_points))
        .collect();
    if history.is_empty() {
        frame.render_widget(Paragraph::new(muted("Collecting history…")), chart);
    } else {
        plot(frame, chart, &history, 10_000, LIME);
    }
    let half = inner.width / 2;
    for row in 0..core_rows {
        let per_column = snapshot
            .cpu
            .per_core
            .len()
            .div_ceil(2)
            .min(usize::from(core_rows));
        for column in 0..2 {
            let index = usize::from(row) + column * per_column;
            if let Some(pct) = snapshot.cpu.per_core.get(index) {
                let x = inner.x + column as u16 * half;
                let cell = Rect::new(x, inner.bottom() - core_rows + row, half, 1);
                let label_width = 4.min(half);
                frame.render_widget(
                    Paragraph::new(muted(format!("C{index:<2} "))),
                    Rect::new(cell.x, cell.y, label_width, 1),
                );
                frame.render_widget(
                    Paragraph::new(meter(
                        *pct,
                        half.saturating_sub(label_width + 1),
                        usage_color(*pct),
                    )),
                    Rect::new(
                        cell.x + label_width,
                        cell.y,
                        half.saturating_sub(label_width),
                        1,
                    ),
                );
            }
        }
    }
}
fn draw_gpu(frame: &mut ratatui::Frame<'_>, area: Rect, snapshot: &metrics::Snapshot) {
    let inner = mag_panel(frame, area, "GPU", LIME);
    if let Some(gpu) = snapshot.gpus.first() {
        frame.render_widget(
            Paragraph::new(vec![
                Line::from(vec![muted("GPU 0: "), label(gpu.name.clone(), TEXT)]),
                Line::from(muted("Sensors unavailable")),
            ]),
            inner,
        );
    }
}
fn draw_memory(frame: &mut ratatui::Frame<'_>, area: Rect, snapshot: &metrics::Snapshot) {
    let inner = mag_panel(frame, area, "MEMORY", CYAN);
    let lines = vec![
        Line::from(vec![
            muted("RAM  "),
            label(compact_bytes(snapshot.memory.used_bytes), TEXT),
            muted(format!("/{}", compact_bytes(snapshot.memory.total_bytes))),
        ]),
        meter(snapshot.memory.percent, inner.width, CYAN),
        Line::from(vec![
            muted("SWAP "),
            label(compact_bytes(snapshot.swap.used_bytes), TEXT),
            muted(format!("/{}", compact_bytes(snapshot.swap.total_bytes))),
        ]),
        meter(
            snapshot.swap.percent,
            inner.width,
            if snapshot.swap.percent > 50.0 {
                AMBER
            } else {
                CYAN
            },
        ),
        Line::from(muted(format!(
            "CACHE {}  BUF {}",
            compact_bytes(snapshot.memory.cached_bytes),
            compact_bytes(snapshot.memory.buffers_bytes)
        ))),
    ];
    frame.render_widget(Paragraph::new(lines), inner);
}
fn draw_network(
    frame: &mut ratatui::Frame<'_>,
    area: Rect,
    snapshot: &metrics::Snapshot,
    state: &TuiState,
) {
    let inner = mag_panel(frame, area, "NETWORK", PINK);
    if inner.is_empty() {
        return;
    }
    let chart_h = inner.height.saturating_sub(2) / 2;
    let rows = Layout::vertical([
        Constraint::Length(1),
        Constraint::Length(chart_h),
        Constraint::Length(1),
        Constraint::Length(chart_h),
    ])
    .split(inner);
    let rate = |bytes| {
        if state.network_rates_available {
            compact_bytes(bytes) + "/s"
        } else {
            "awaiting sample".into()
        }
    };
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("▼ DOWN ", LIME),
            label(rate(snapshot.network.recv_bytes_per_sec), TEXT),
        ])),
        rows[0],
    );
    frame.render_widget(
        Paragraph::new(Line::from(vec![
            label("▲ UP   ", PINK),
            label(rate(snapshot.network.sent_bytes_per_sec), TEXT),
        ])),
        rows[2],
    );
    let samples: Vec<_> = state
        .history
        .iter()
        .rev()
        .take(60)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let recv: Vec<_> = samples
        .iter()
        .map(|p| p.network.map_or(0, |n| n.0))
        .collect();
    let sent: Vec<_> = samples
        .iter()
        .map(|p| p.network.map_or(0, |n| n.1))
        .collect();
    let max = recv.iter().chain(&sent).copied().max().unwrap_or(1).max(1);
    plot(frame, rows[1], &recv, max, LIME);
    plot(frame, rows[3], &sent, max, PINK);
}
fn draw_storage(frame: &mut ratatui::Frame<'_>, area: Rect, snapshot: &metrics::Snapshot) {
    let inner = mag_panel(frame, area, "STORAGE", AMBER);
    let mut lines = vec![];
    for disk in snapshot.disks.iter().take(usize::from(inner.height / 2)) {
        lines.push(Line::from(vec![
            Span::styled(disk.mountpoint.clone(), Style::default().fg(CYAN)),
            muted(format!(
                " {}/{} {}",
                compact_bytes(disk.used_bytes),
                compact_bytes(disk.total_bytes),
                disk.filesystem
            )),
        ]));
        lines.push(meter(disk.percent, inner.width, usage_color(disk.percent)));
    }
    if lines.is_empty() {
        lines.push(Line::from(muted("No mounted disks reported")));
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
    let docker_h = if !snapshot.docker.available {
        5
    } else {
        (area.height * 28 / 100).max(8)
    };
    let rows = Layout::vertical([Constraint::Min(8), Constraint::Length(docker_h)]).split(area);
    draw_processes(frame, rows[0], snapshot, state);
    draw_docker(frame, rows[1], snapshot, state);
}
fn selection_style(focused: bool) -> Style {
    if focused {
        Style::default()
            .bg(LIME)
            .fg(BACKGROUND)
            .add_modifier(Modifier::BOLD)
    } else {
        Style::default().bg(SURFACE).fg(TEXT)
    }
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
    let inner = mag_panel(frame, area, "PROCESSES", LIME);
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
    } else if area.height >= 10 {
        let parts = Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).split(inner);
        frame.render_widget(
            Paragraph::new(Line::from(vec![
                muted(" SORT:"),
                label(
                    if state.sort_by_cpu {
                        "CPU▼"
                    } else {
                        "MEM▼"
                    },
                    if state.sort_by_cpu { LIME } else { PINK },
                ),
                muted(format!("  {count} PROCS")),
                muted(if focused { "  ACTIVE" } else { "  IDLE" }),
                muted(if state.filter.is_empty() {
                    String::new()
                } else {
                    format!("  FILTER:/{}", state.filter)
                }),
            ])),
            parts[0],
        );
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
    let rss = rows.width >= 60;
    let user = rows.width >= 70;
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
    let fixed_width = 23 + if rss { 10 } else { 0 } + if user { 10 } else { 0 };
    let command_width = usize::from(rows.width).saturating_sub(fixed_width);
    let items = processes.iter().map(|p| {
        let mut cells = vec![
            Cell::from(p.pid.to_string()),
            Cell::from(ellipsize(
                if p.command.is_empty() {
                    &p.name
                } else {
                    &p.command
                },
                command_width,
            )),
            Cell::from(if state.process_cpu_available {
                format!("{:>5.1}", p.cpu_percent)
            } else {
                "    —".into()
            }),
            Cell::from(format!("{:>5.1}", p.memory_percent)),
        ];
        if rss {
            cells.push(Cell::from(format!("{:>8}", compact_bytes(p.rss_bytes))));
        }
        if user {
            cells.push(Cell::from(p.user.clone()));
        }
        Row::new(cells).style(Style::default().fg(if p.cpu_percent > 50.0 {
            PINK
        } else if p.cpu_percent > 20.0 {
            AMBER
        } else {
            TEXT
        }))
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
        .header(
            Row::new(names).style(
                Style::default()
                    .fg(BACKGROUND)
                    .bg(MUTED)
                    .add_modifier(Modifier::BOLD),
            ),
        )
        .row_highlight_style(selection_style(focused))
        .highlight_symbol(" ");
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
    let inner = mag_panel(frame, area, "DOCKER", PINK);
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
    let status_height = if inner.height >= 6 { 2 } else { 1 };
    let parts =
        Layout::vertical([Constraint::Length(status_height), Constraint::Min(0)]).split(inner);
    let status = format!(
        " {} {total} CONTAINERS  {running} RUNNING  {} STOPPED",
        if state.docker_focused {
            "ACTIVE"
        } else {
            "IDLE"
        },
        total - running
    );
    frame.render_widget(Paragraph::new(muted(status)), parts[0]);
    let inner = parts[1];
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
            }),
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
            .header(
                Row::new(headers).style(
                    Style::default()
                        .fg(BACKGROUND)
                        .bg(MUTED)
                        .add_modifier(Modifier::BOLD),
                ),
            )
            .row_highlight_style(selection_style(state.docker_focused))
            .highlight_symbol(" "),
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
    let mut keys = if state.filtering {
        vec![("Enter", "APPLY"), ("Esc", "CANCEL"), ("Ctrl+C", "QUIT")]
    } else if state.confirming() {
        vec![("Y", "CONFIRM"), ("N/Esc", "CANCEL")]
    } else if area.width < 90 {
        vec![
            ("q", "QUIT"),
            ("?", "HELP"),
            ("Tab", "PANEL"),
            ("↑/k", "UP"),
            ("↓/j", "DN"),
        ]
    } else if state.docker_focused
        && state.docker_controls_enabled
        && snapshot.docker.available
        && !snapshot.docker.containers.is_empty()
    {
        vec![
            ("↑/k", "UP"),
            ("↓/j", "DN"),
            ("Tab", "PROCS"),
            ("x", "START/STOP"),
            ("r", "RESTART"),
            ("d", "DIM"),
            ("q", "QUIT"),
            ("?", "HELP"),
        ]
    } else if state.docker_focused {
        vec![
            ("↑/k", "UP"),
            ("↓/j", "DN"),
            ("Tab", "PROCS"),
            ("d", "DIM"),
            ("q", "QUIT"),
            ("?", "HELP"),
        ]
    } else {
        vec![
            ("↑/k", "UP"),
            ("↓/j", "DN"),
            ("s", "SORT"),
            ("/", "FIND"),
            ("⏎", "INSPECT"),
            ("Tab", "DOCKER"),
            ("K", "KILL"),
            ("d", "DIM"),
            ("q", "QUIT"),
            ("?", "HELP"),
        ]
    };
    if area.width >= 120 {
        keys.push(("Space", if state.paused { "RESUME" } else { "PAUSE" }));
    }
    let mut spans = vec![];
    let mut used = 0usize;
    for (key, description) in keys {
        let size = Line::from(format!(" {key}  {description}  ")).width();
        if used + size > usize::from(area.width) {
            continue;
        }
        spans.push(Span::styled(
            format!(" {key} "),
            Style::default()
                .bg(MUTED)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        ));
        spans.push(muted(format!(" {description}  ")));
        used += size;
    }
    let row = Rect::new(area.x, area.bottom().saturating_sub(1), area.width, 1);
    frame.render_widget(Paragraph::new(Line::from(spans)).centered(), row);
    if let Some(message) = &state.action_message {
        frame.render_widget(
            Paragraph::new(label(
                message.text.clone(),
                if message.ok { LIME } else { AMBER },
            )),
            Rect::new(area.x, area.y, area.width, 1),
        );
    }
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

fn ellipsize(text: &str, width: usize) -> String {
    let line = Line::from(text);
    if line.width() <= width {
        return text.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut result = String::new();
    let mut used = 0;
    for grapheme in line.styled_graphemes(Style::default()) {
        let cells = Span::raw(grapheme.symbol).width();
        if used + cells >= width {
            break;
        }
        result.push_str(grapheme.symbol);
        used += cells;
    }
    result.push('…');
    result
}

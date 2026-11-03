use super::*;
#[path = "../../examples/support/mod.rs"]
mod support;
use ratatui::{Terminal, backend::TestBackend};

fn press(code: KeyCode, state: &mut TuiState, snapshot: &metrics::Snapshot) -> bool {
    handle_key(KeyEvent::new(code, KeyModifiers::NONE), state, snapshot).unwrap()
}
fn ready() -> TuiState {
    TuiState {
        last_updated: Some(Instant::now()),
        process_cpu_available: true,
        network_rates_available: true,
        ..Default::default()
    }
}
#[test]
fn tab_changes_panel_without_sorting_and_hidden_docker_cannot_take_focus() {
    let snapshot = support::snapshot();
    let mut state = ready();
    press(KeyCode::Tab, &mut state, &snapshot);
    assert!(state.docker_focused);
    assert!(state.sort_by_cpu);
    press(KeyCode::BackTab, &mut state, &snapshot);
    assert!(!state.docker_focused);
    state.docker_panel_visible = false;
    press(KeyCode::Char('c'), &mut state, &snapshot);
    assert!(!state.docker_focused);
}
#[test]
fn search_cancel_restores_previous_filter_and_escape_clears_applied_search() {
    let snapshot = support::snapshot();
    let mut state = TuiState {
        filter: "postgres".into(),
        ..ready()
    };
    press(KeyCode::Char('/'), &mut state, &snapshot);
    press(KeyCode::Char('z'), &mut state, &snapshot);
    assert_eq!(state.filter, "postgresz");
    press(KeyCode::Esc, &mut state, &snapshot);
    assert_eq!(state.filter, "postgres");
    assert!(!state.filtering);
    press(KeyCode::Esc, &mut state, &snapshot);
    assert!(state.filter.is_empty());
    state.filter = "zzzz".into();
    let text = render_dashboard_text_size(
        &snapshot,
        &ops::evaluate_snapshot(&snapshot),
        &state,
        64,
        18,
    );
    assert!(text.contains("No matching processes"));
    assert!(text.contains("Esc clears"));
}
#[test]
fn paging_and_refresh_keep_selection_visible_and_attached_to_identity() {
    let snapshot = support::snapshot();
    let mut state = ready();
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal
        .draw(|f| view::draw(f, &snapshot, &ops::evaluate_snapshot(&snapshot), &mut state))
        .unwrap();
    let page = state.process_page_size;
    press(KeyCode::PageDown, &mut state, &snapshot);
    assert_eq!(state.process_cursor, page);
    let pid = selected_process(&snapshot, &state).unwrap().pid;
    let mut next = snapshot.clone();
    next.top_processes
        .iter_mut()
        .find(|p| p.pid == pid)
        .unwrap()
        .cpu_percent = 999.0;
    reconcile_selection(&mut state, &snapshot, &next);
    assert_eq!(selected_process(&next, &state).unwrap().pid, pid);
    assert_eq!(state.process_cursor, 0);
    press(KeyCode::End, &mut state, &next);
    terminal
        .draw(|f| view::draw(f, &next, &ops::evaluate_snapshot(&next), &mut state))
        .unwrap();
    assert!(state.process_offset > 0);
    let text = render_dashboard_text_size(&next, &ops::evaluate_snapshot(&next), &state, 80, 24);
    assert!(text.contains(&selected_process(&next, &state).unwrap().pid.to_string()));
    let empty = metrics::Snapshot::default();
    reconcile_selection(&mut state, &next, &empty);
    assert_eq!(state.process_cursor, 0);
    assert_eq!(state.docker_cursor, 0);
}
#[test]
fn confirmation_freezes_target_and_cancel_never_executes_an_action() {
    let snapshot = support::snapshot();
    let mut state = ready();
    press(KeyCode::Char('K'), &mut state, &snapshot);
    let pid = state.pending_process.as_ref().unwrap().pid;
    press(KeyCode::Down, &mut state, &snapshot);
    assert_eq!(state.process_cursor, 0);
    assert_eq!(state.pending_process.as_ref().unwrap().pid, pid);
    assert!(!press(KeyCode::Char('q'), &mut state, &snapshot));
    assert!(!state.process_kill_confirm);
    assert!(state.pending_process.is_none());
    press(KeyCode::Tab, &mut state, &snapshot);
    press(KeyCode::Char('x'), &mut state, &snapshot);
    assert_eq!(state.pending_container.as_ref().unwrap().id, "abc123");
    assert_eq!(state.docker_confirm, Some(DockerConfirm::Stop));
    press(KeyCode::Esc, &mut state, &snapshot);
    assert!(state.pending_container.is_none());
    assert!(state.docker_confirm.is_none());
}
#[test]
fn vanished_confirmation_targets_are_canceled_without_invoking_commands() {
    let snapshot = support::snapshot();
    let mut state = ready();
    press(KeyCode::Char('K'), &mut state, &snapshot);
    press(
        KeyCode::Char('y'),
        &mut state,
        &metrics::Snapshot::default(),
    );
    assert!(!state.action_message.as_ref().unwrap().ok);
    assert!(
        state
            .action_message
            .as_ref()
            .unwrap()
            .text
            .contains("canceled")
    );
    state.docker_focused = true;
    press(KeyCode::Char('x'), &mut state, &snapshot);
    press(
        KeyCode::Char('y'),
        &mut state,
        &metrics::Snapshot::default(),
    );
    assert!(
        state
            .action_message
            .as_ref()
            .unwrap()
            .text
            .contains("canceled")
    );
}
#[test]
fn pause_blocks_stale_actions_and_release_events_do_not_toggle_twice() {
    let snapshot = support::snapshot();
    let mut state = ready();
    press(KeyCode::Char(' '), &mut state, &snapshot);
    press(KeyCode::Char('K'), &mut state, &snapshot);
    assert!(!state.process_kill_confirm);
    assert!(state.action_message.is_some());
    let key = KeyEvent::new_with_kind(
        KeyCode::Char(' '),
        KeyModifiers::NONE,
        KeyEventKind::Release,
    );
    handle_key(key, &mut state, &snapshot).unwrap();
    assert!(state.paused);
    assert!(
        handle_key(
            KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
            &mut state,
            &snapshot
        )
        .unwrap()
    );
}
#[test]
fn native_table_highlights_only_the_selected_container_even_with_shared_names() {
    let snapshot = support::snapshot();
    let mut state = TuiState {
        docker_focused: true,
        ..ready()
    };
    let mut terminal = Terminal::new(TestBackend::new(120, 36)).unwrap();
    terminal
        .draw(|f| view::draw(f, &snapshot, &ops::evaluate_snapshot(&snapshot), &mut state))
        .unwrap();
    let buffer = terminal.backend().buffer();
    let row = |name: &str| {
        (0..36)
            .find(|y| {
                (0..120)
                    .map(|x| buffer[(x, *y)].symbol())
                    .collect::<String>()
                    .contains(name)
            })
            .unwrap()
    };
    let selected = row("postgres:16");
    let other = row("redis:7");
    assert_ne!(buffer[(50, selected)].bg, buffer[(50, other)].bg);
}
#[test]
fn dashboard_handles_small_and_unicode_viewports_without_losing_quit_or_focus() {
    let mut snapshot = support::snapshot();
    snapshot.top_processes[0].command = "🦀 工作者 e\u{301} --serve".repeat(30);
    for width in [0, 1, 49, 50, 64, 80, 99, 100, 120, 180] {
        for height in [0, 1, 11, 12, 18, 24, 36] {
            for mode in 0..7 {
                let mut state = ready();
                match mode {
                    1 => state.help_open = true,
                    2 => state.inspector_open = true,
                    3 => state.process_kill_confirm = true,
                    4 => state.docker_focused = true,
                    5 => state.filtering = true,
                    6 => state.alerts_open = true,
                    _ => {}
                }
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal
                    .draw(|f| {
                        view::draw(f, &snapshot, &ops::evaluate_snapshot(&snapshot), &mut state)
                    })
                    .unwrap();
                if width >= 50 && height >= 12 && mode == 0 {
                    let text = render_dashboard_text_size(
                        &snapshot,
                        &ops::evaluate_snapshot(&snapshot),
                        &state,
                        width,
                        height,
                    );
                    assert!(text.contains("q quit"), "{width}x{height}");
                    assert!(
                        text.contains("1200"),
                        "selected row clipped at {width}x{height}"
                    );
                }
            }
        }
    }
}
#[test]
fn compact_help_can_scroll_to_global_shortcuts() {
    let snapshot = support::snapshot();
    let mut state = ready();
    press(KeyCode::Char('?'), &mut state, &snapshot);
    let mut terminal = Terminal::new(TestBackend::new(50, 12)).unwrap();
    for _ in 0..4 {
        press(KeyCode::PageDown, &mut state, &snapshot);
        terminal
            .draw(|f| view::draw(f, &snapshot, &ops::evaluate_snapshot(&snapshot), &mut state))
            .unwrap();
    }
    let text = render_dashboard_text_size(
        &snapshot,
        &ops::evaluate_snapshot(&snapshot),
        &state,
        50,
        12,
    );
    assert!(text.contains("Pause / resume"));
    assert!(text.contains("Quit"));
}

#[test]
fn inspector_and_health_signals_can_scroll_through_long_details() {
    let mut snapshot = support::snapshot();
    snapshot.top_processes[0].command = format!("{} END_COMMAND", "long argument ".repeat(70));
    let mut state = ready();
    press(KeyCode::Enter, &mut state, &snapshot);
    let mut terminal = Terminal::new(TestBackend::new(64, 18)).unwrap();
    for _ in 0..20 {
        press(KeyCode::PageDown, &mut state, &snapshot);
        terminal
            .draw(|f| view::draw(f, &snapshot, &ops::evaluate_snapshot(&snapshot), &mut state))
            .unwrap();
    }
    assert!(
        render_dashboard_text_size(
            &snapshot,
            &ops::evaluate_snapshot(&snapshot),
            &state,
            64,
            18
        )
        .contains("END_COMMAND")
    );
    press(KeyCode::Esc, &mut state, &snapshot);
    snapshot.cpu.percent = 99.0;
    snapshot.memory.percent = 99.0;
    snapshot.swap.percent = 99.0;
    press(KeyCode::Char('a'), &mut state, &snapshot);
    assert!(
        render_dashboard_text_size(
            &snapshot,
            &ops::evaluate_snapshot(&snapshot),
            &state,
            64,
            18
        )
        .contains("CRITICAL")
    );
}

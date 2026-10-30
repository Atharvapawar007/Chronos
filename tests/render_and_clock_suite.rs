//! Render smoke tests (no panics at any terminal size) and clock regression tests.

use chronos::domain::{ActivityKind, ClockEngine, ClockStatus, Preferences};
use chronos::runtime::{PrimaryRoute, WorkspaceRuntime};
use chronos::tui::shell::render_workspace_frame;
use chrono::Utc;
use ratatui::{backend::TestBackend, Terminal};

fn screen_text(term: &Terminal<TestBackend>) -> String {
    let buf = term.backend().buffer();
    let w = buf.area.width as usize;
    buf.content()
        .chunks(w)
        .map(|row| row.iter().map(|c| c.symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

fn isolated_runtime() -> WorkspaceRuntime {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Mutex;
    // Env vars are process-global and paths are resolved at bootstrap, so serialize bootstrap
    // and give every runtime its own directory.
    static LOCK: Mutex<()> = Mutex::new(());
    static COUNTER: AtomicUsize = AtomicUsize::new(0);
    let _guard = LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let dir = std::env::temp_dir().join(format!(
        "chronos-test-{}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst)
    ));
    let _ = std::fs::create_dir_all(&dir);
    std::env::set_var("CHRONOS_VAULT_PATH", dir.join("vault.json"));
    std::env::set_var("CHRONOS_CONFIG_PATH", dir.join("prefs.toml"));
    std::env::set_var("HOME", &dir);
    std::env::set_var("XDG_CACHE_HOME", dir.join("cache"));
    WorkspaceRuntime::bootstrap().expect("bootstrap")
}

#[test]
fn every_route_renders_at_every_size_without_panic() {
    let mut rt = isolated_runtime();
    let sizes = [(1, 1), (10, 4), (40, 12), (60, 20), (85, 24), (100, 30), (120, 40), (200, 60)];
    for route in PrimaryRoute::ALL {
        rt.switch_route(route);
        for (w, h) in sizes {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| render_workspace_frame(f, &rt)).unwrap();
        }
    }
}

#[test]
fn command_deck_shows_banner_and_tagline_when_tall_enough() {
    let mut rt = isolated_runtime();
    rt.switch_route(PrimaryRoute::Overview);
    let mut term = Terminal::new(TestBackend::new(140, 50)).unwrap();
    term.draw(|f| render_workspace_frame(f, &rt)).unwrap();
    let text = screen_text(&term);
    assert!(text.contains("Discipline = Value + Reward - Friction"));
    assert!(text.contains("██████╗"));
}

#[test]
fn adjusting_a_finished_max_length_phase_does_not_panic() {
    let prefs = Preferences::default();
    let mut clock = ClockEngine::new_from_prefs(&prefs);
    let t0 = Utc::now();
    clock.adjust_duration_secs(10 * 3600, t0);
    assert_eq!(clock.target_seconds, 4 * 3600);
    clock.start_or_resume(t0);
    clock.mark_awaiting_completion(t0);
    let before = clock.target_seconds;
    clock.adjust_duration_secs(300, t0);
    clock.adjust_duration_secs(-300, t0);
    assert_eq!(clock.status, ClockStatus::AwaitingCompletion);
    assert_eq!(clock.target_seconds, before);
}

#[test]
fn shrinking_a_running_phase_never_goes_below_elapsed() {
    let prefs = Preferences::default();
    let mut clock = ClockEngine::new_from_prefs(&prefs);
    clock.switch_activity(ActivityKind::Focus, &prefs);
    let t0 = Utc::now();
    clock.start_or_resume(t0);
    let later = t0 + chrono::Duration::minutes(10);
    for _ in 0..20 {
        clock.adjust_duration_secs(-300, later);
    }
    assert!(clock.target_seconds > 600);
}

#[test]
fn launching_from_shelf_never_discards_a_running_session() {
    let mut rt = isolated_runtime();
    rt.clock.switch_activity(ActivityKind::Focus, &rt.prefs.clone());
    rt.clock.start_or_resume(Utc::now());
    let id_before = rt.clock.active_session_id.clone();
    assert!(!rt.vault.reading_shelf.is_empty());
    rt.launch_reading_material(0);
    assert_eq!(rt.clock.status, ClockStatus::Running);
    assert_eq!(rt.clock.activity_kind, ActivityKind::Focus);
    assert_eq!(rt.clock.active_session_id, id_before);
}

#[test]
fn reading_velocity_uses_pages_read_not_absolute_page() {
    use chronos::domain::{MaterialKind, ReadingMaterial, ReadingStatus};
    let item = ReadingMaterial {
        id: "m".into(),
        title: "T".into(),
        author: "A".into(),
        kind: MaterialKind::Book,
        category_id: "c".into(),
        total_pages: 400,
        current_page: 300,
        total_reading_seconds: 3600,
        total_pages_read: 20,
        completed_sessions_count: 1,
        status: ReadingStatus::Reading,
        notes: String::new(),
        created_at_utc: Utc::now(),
        last_read_at_utc: None,
    };
    assert_eq!(item.pages_per_hour(), 20.0);
}

#[test]
fn doctor_flags_orphaned_and_inconsistent_records() {
    use chronos::cli::vault_integrity_problems;
    let rt = isolated_runtime();
    assert!(vault_integrity_problems(&rt.vault).is_empty());
    let mut broken = rt.vault.clone();
    broken.reading_shelf[0].category_id = "cat_missing".into();
    assert_eq!(vault_integrity_problems(&broken).len(), 1);
}

#[test]
fn zen_mode_draws_only_the_ring_and_time() {
    let mut rt = isolated_runtime();
    rt.switch_route(PrimaryRoute::FocusStudio);
    rt.zen_mode = true;
    let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
    term.draw(|f| render_workspace_frame(f, &rt)).unwrap();
    let text = screen_text(&term);
    for word in ["CHRONOS", "Home", "Focus", "Space", "Start", "XP", "Goal", "Ready"] {
        assert!(!text.contains(word), "zen mode leaked '{}'", word);
    }
    assert!(text.contains('█'), "digits should be visible");
    assert!(text.chars().any(|c| ('\u{2800}'..='\u{28ff}').contains(&c)), "ring should be visible");
}

#[test]
fn monochrome_theme_contains_only_greys() {
    use chronos::tui::theme::{ColorDepth, Palette, ThemeId};
    use ratatui::style::Color;
    let p = Palette::with_depth(ThemeId::Mono, ColorDepth::True);
    let mut all = vec![p.bg, p.card, p.card_hi, p.border, p.text, p.dim, p.accent, p.accent2, p.accent3, p.good, p.warn, p.bad, p.info, p.on_accent, p.category("#EF5350"), p.category("#26A69A")];
    all.extend(p.heat);
    for c in all {
        match c {
            Color::Rgb(r, g, b) => assert!(r == g && g == b, "non-grey colour {:?}", c),
            other => panic!("unexpected colour {:?}", other),
        }
    }
    assert_eq!(ThemeId::from_key("monochrome"), ThemeId::Mono);
}

#[test]
fn every_modal_renders_at_small_and_large_sizes() {
    use chronos::runtime::ModalDialog;
    let mut rt = isolated_runtime();
    let cat = rt.vault.categories[0].id.clone();
    let modals = vec![
        ModalDialog::ShortcutHelp,
        ModalDialog::QuitConfirmation,
        ModalDialog::ObjectiveAndNotesEditor { field_index: 0, objective_buf: "x".into(), notes_buf: String::new(), end_page_buf: String::new(), complete_on_submit: true },
        ModalDialog::SessionDeleteConfirm { session_id: "s".into(), summary_label: "demo".into() },
        ModalDialog::PlannedBlockEditor { field_index: 0, title_buf: String::new(), activity_kind: ActivityKind::Focus, category_id: cat.clone(), duration_minutes: 25, priority: chronos::domain::PlanPriority::High },
        ModalDialog::CelebrationBanner { headline: "LEVEL UP".into(), subtitle: "Lv 2".into(), unlocked_titles: vec![("T".into(), "D".into(), 50)], completed_full_cycle: true, opened_at: std::time::Instant::now() },
    ];
    for m in modals {
        rt.modal = Some(m);
        for (w, h) in [(1, 1), (30, 8), (60, 18), (120, 40)] {
            let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
            term.draw(|f| render_workspace_frame(f, &rt)).unwrap();
        }
    }
}

#[test]
fn guide_topics_all_render_and_scroll_is_bounded() {
    let mut rt = isolated_runtime();
    rt.switch_route(PrimaryRoute::Guide);
    for t in 0..chronos::tui::views::guide::TOPICS.len() {
        rt.guide_topic = t;
        rt.guide_scroll = 9999;
        let mut term = Terminal::new(TestBackend::new(110, 30)).unwrap();
        term.draw(|f| render_workspace_frame(f, &rt)).unwrap();
        assert!(rt.guide_max_scroll.get() < 200);
    }
}

#[test]
fn daily_chest_is_stable_and_in_range() {
    use chronos::domain::{chest_tier, daily_chest_xp};
    let mut jackpots = 0;
    for d in 1..=28 {
        let key = format!("2026-02-{:02}", d);
        let a = daily_chest_xp(&key);
        assert_eq!(a, daily_chest_xp(&key));
        assert!((30..90).contains(&a) || a == 150);
        if a == 150 { jackpots += 1; assert_eq!(chest_tier(a), "Jackpot"); }
    }
    assert!(jackpots <= 6);
}

#[test]
fn zen_background_is_pure_black_in_every_theme() {
    use chronos::tui::theme::ThemeId;
    use ratatui::style::Color;
    std::env::set_var("CHRONOS_COLOR_MODE", "truecolor");
    let mut rt = isolated_runtime();
    rt.switch_route(PrimaryRoute::FocusStudio);
    rt.zen_mode = true;
    for t in ThemeId::ALL {
        rt.prefs.appearance.theme = t.key().to_string();
        let mut term = Terminal::new(TestBackend::new(100, 30)).unwrap();
        term.draw(|f| render_workspace_frame(f, &rt)).unwrap();
        for cell in term.backend().buffer().content() {
            assert_eq!(cell.bg, Color::Rgb(0, 0, 0), "theme {:?} leaked a non-black background", t);
        }
    }
}

#[test]
fn all_books_can_be_deleted_and_history_is_kept() {
    use chronos::runtime::ModalDialog;
    let mut rt = isolated_runtime();
    let before_sessions = rt.vault.sessions.len();
    while !rt.vault.reading_shelf.is_empty() {
        rt.ask_delete_reading_material(0);
        assert!(matches!(rt.modal, Some(ModalDialog::ReadingDeleteConfirm { .. })));
        rt.confirm_reading_delete();
    }
    assert!(rt.vault.reading_shelf.is_empty());
    assert_eq!(rt.vault.sessions.len(), before_sessions);
    rt.switch_route(PrimaryRoute::ReadingSanctuary);
    let mut term = Terminal::new(TestBackend::new(120, 36)).unwrap();
    term.draw(|f| render_workspace_frame(f, &rt)).unwrap();
    assert!(screen_text(&term).contains("Your shelf is empty"));
}

#[test]
fn book_states_cycle_and_old_saves_still_load() {
    use chronos::domain::ReadingStatus;
    assert_eq!(ReadingStatus::Have.next(), ReadingStatus::Reading);
    assert_eq!(ReadingStatus::Reading.next(), ReadingStatus::Read);
    assert_eq!(ReadingStatus::Read.next(), ReadingStatus::Abandoned);
    assert_eq!(ReadingStatus::Abandoned.next(), ReadingStatus::Have);
    for (old, new) in [("\"active\"", ReadingStatus::Reading), ("\"completed\"", ReadingStatus::Read), ("\"paused\"", ReadingStatus::Have), ("\"abandoned\"", ReadingStatus::Abandoned)] {
        assert_eq!(serde_json::from_str::<ReadingStatus>(old).unwrap(), new);
    }
    let mut rt = isolated_runtime();
    assert_eq!(rt.vault.reading_shelf[0].status, ReadingStatus::Reading);
    rt.cycle_reading_status(0);
    let b = &rt.vault.reading_shelf[0];
    assert_eq!(b.status, ReadingStatus::Read);
    assert_eq!(b.current_page, b.total_pages);
}

#[test]
fn starting_a_session_on_a_have_book_makes_it_reading() {
    use chronos::domain::ReadingStatus;
    let mut rt = isolated_runtime();
    rt.vault.reading_shelf[0].status = ReadingStatus::Have;
    rt.launch_reading_material(0);
    assert_eq!(rt.vault.reading_shelf[0].status, ReadingStatus::Reading);
}

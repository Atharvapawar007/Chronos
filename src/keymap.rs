//! Context-Aware Keyboard Event Dispatcher (`keymap.rs`).
//!
//! Ensures modal text editors and confirmation phrases (`RESET MY DATA`) capture keystrokes cleanly
//! without firing global timer shortcuts, while providing fast 1-key workflows across all 8 views.

use crate::domain::{
    AchievementFamily, ActivityKind, CadencePreset, CategoryScope, ClockStatus, CompletionState,
    MaterialKind, PlanStatus, APPROVED_CATEGORY_COLORS, CATEGORY_ICON_GLYPHS,
};
use crate::runtime::{ModalDialog, PrimaryRoute, WorkspaceRuntime};
use crate::storage::{save_preferences, ResetOperation};
use crate::tui::theme::ThemeId;
use crate::tui::views::guide::TOPICS as GUIDE_TOPICS;
use crate::tui::views::settings::SETTINGS_SECTIONS;
use chrono::Utc;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn handle_key_event(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    // Ctrl+C always performs a clean, immediate shutdown
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        rt.should_quit = true;
        return;
    }

    // 1. If a Modal Dialog is open, route keys exclusively to the modal handler
    if rt.modal.is_some() {
        handle_modal_key(rt, key);
        return;
    }

    // 2. If Session Ledger search bar is actively capturing text, route to search input
    if rt.route == PrimaryRoute::SessionLedger && rt.ledger_search_editing {
        match key.code {
            KeyCode::Esc | KeyCode::Enter => {
                rt.ledger_search_editing = false;
                rt.ledger_cursor = 0;
            }
            KeyCode::Backspace => {
                rt.ledger_search_query.pop();
                rt.ledger_cursor = 0;
            }
            KeyCode::Char(ch) => {
                rt.ledger_search_query.push(ch);
                rt.ledger_cursor = 0;
            }
            _ => {}
        }
        return;
    }

    // 3. Global Navigation & Cross-View Shortcuts
    match key.code {
        KeyCode::Char('1') => {
            rt.switch_route(PrimaryRoute::Overview);
            return;
        }
        KeyCode::Char('2') => {
            rt.switch_route(PrimaryRoute::FocusStudio);
            return;
        }
        KeyCode::Char('3') => {
            rt.switch_route(PrimaryRoute::ReadingSanctuary);
            return;
        }
        KeyCode::Char('4') => {
            rt.switch_route(PrimaryRoute::Analytics);
            return;
        }
        KeyCode::Char('5') => {
            rt.switch_route(PrimaryRoute::SessionLedger);
            return;
        }
        KeyCode::Char('6') => {
            rt.switch_route(PrimaryRoute::AchievementsHall);
            return;
        }
        KeyCode::Char('7') => {
            rt.switch_route(PrimaryRoute::CategoryStudio);
            return;
        }
        KeyCode::Char('8') => {
            rt.switch_route(PrimaryRoute::SettingsCenter);
            return;
        }
        KeyCode::Char('9') | KeyCode::Char('?') => {
            rt.switch_route(PrimaryRoute::Guide);
            return;
        }
        KeyCode::Char('K') => {
            rt.modal = Some(ModalDialog::ShortcutHelp);
            return;
        }
        KeyCode::Char(' ') => {
            if rt.clock.status == ClockStatus::Idle {
                if rt.route == PrimaryRoute::ReadingSanctuary
                    && rt.clock.activity_kind != ActivityKind::Reading
                {
                    rt.clock.switch_activity(ActivityKind::Reading, &rt.prefs);
                } else if rt.route == PrimaryRoute::FocusStudio
                    && rt.clock.activity_kind == ActivityKind::Reading
                {
                    rt.clock.switch_activity(ActivityKind::Focus, &rt.prefs);
                }
                rt.reconcile_clock_category();
            }
            rt.clock.toggle_run_pause(Utc::now());
            rt.persist_active_clock();
            return;
        }
        KeyCode::Char('t') | KeyCode::Char('T') => {
            rt.cycle_theme();
            return;
        }
        KeyCode::Char('+') | KeyCode::Char('=') => {
            rt.clock.adjust_duration_secs(300, Utc::now());
            rt.persist_active_clock();
            return;
        }
        KeyCode::Char('-') | KeyCode::Char('_') => {
            rt.clock.adjust_duration_secs(-300, Utc::now());
            rt.persist_active_clock();
            return;
        }
        KeyCode::Char('q') | KeyCode::Char('Q') | KeyCode::Esc => {
            if key.code == KeyCode::Esc && rt.zen_mode {
                rt.zen_mode = false;
                return;
            }
            let snap = rt.clock.snapshot(Utc::now());
            if rt.prefs.safety.confirm_quit_active_session
                && snap.status != ClockStatus::Idle
                && snap.elapsed_seconds >= 5
            {
                rt.modal = Some(ModalDialog::QuitConfirmation);
            } else {
                rt.should_quit = true;
            }
            return;
        }
        _ => {}
    }

    // 4. Route-Specific Keybindings
    match rt.route {
        PrimaryRoute::Overview => handle_overview_key(rt, key),
        PrimaryRoute::FocusStudio => handle_focus_key(rt, key),
        PrimaryRoute::ReadingSanctuary => handle_reading_key(rt, key),
        PrimaryRoute::Analytics => handle_analytics_key(rt, key),
        PrimaryRoute::SessionLedger => handle_ledger_key(rt, key),
        PrimaryRoute::AchievementsHall => handle_achievements_key(rt, key),
        PrimaryRoute::CategoryStudio => handle_categories_key(rt, key),
        PrimaryRoute::SettingsCenter => handle_settings_key(rt, key),
        PrimaryRoute::Guide => handle_guide_key(rt, key),
    }
}

fn handle_guide_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    let n = GUIDE_TOPICS.len();
    match key.code {
        KeyCode::Tab | KeyCode::Right | KeyCode::Char('l') => {
            rt.guide_topic = (rt.guide_topic + 1) % n;
            rt.guide_scroll = 0;
        }
        KeyCode::BackTab | KeyCode::Left | KeyCode::Char('h') => {
            rt.guide_topic = (rt.guide_topic + n - 1) % n;
            rt.guide_scroll = 0;
        }
        KeyCode::Up | KeyCode::Char('k') => rt.guide_scroll = rt.guide_scroll.saturating_sub(1),
        KeyCode::Down | KeyCode::Char('j') => {
            rt.guide_scroll = rt.guide_scroll.saturating_add(1).min(rt.guide_max_scroll.get())
        }
        KeyCode::PageUp => rt.guide_scroll = rt.guide_scroll.saturating_sub(10),
        KeyCode::PageDown => {
            rt.guide_scroll = rt.guide_scroll.saturating_add(10).min(rt.guide_max_scroll.get())
        }
        KeyCode::Home => rt.guide_scroll = 0,
        _ => {}
    }
}

fn handle_overview_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    let pending_len = rt
        .vault
        .planned_blocks
        .iter()
        .filter(|p| p.status == PlanStatus::Pending)
        .count();

    match key.code {
        KeyCode::Left => {
            rt.heatmap_cursor_days_ago = (rt.heatmap_cursor_days_ago + 7).min(364);
        }
        KeyCode::Right => {
            rt.heatmap_cursor_days_ago = rt.heatmap_cursor_days_ago.saturating_sub(7);
        }
        KeyCode::Home => {
            rt.heatmap_cursor_days_ago = 0;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            rt.overview_plan_cursor = rt.overview_plan_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if pending_len > 0 {
                rt.overview_plan_cursor = (rt.overview_plan_cursor + 1).min(pending_len - 1);
            }
        }
        KeyCode::Enter => {
            if pending_len > 0 {
                rt.launch_planned_block(rt.overview_plan_cursor);
            } else {
                rt.route = PrimaryRoute::FocusStudio;
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            let cat_id = rt
                .active_category_for(ActivityKind::Focus)
                .map(|c| c.id.clone())
                .unwrap_or_else(|| rt.prefs.focus.default_category_id.clone());
            rt.modal = Some(ModalDialog::PlannedBlockEditor {
                field_index: 0,
                title_buf: String::new(),
                activity_kind: ActivityKind::Focus,
                category_id: cat_id,
                duration_minutes: 50,
                priority: crate::domain::PlanPriority::Standard,
            });
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            rt.switch_route(PrimaryRoute::FocusStudio);
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            rt.switch_route(PrimaryRoute::ReadingSanctuary);
        }
        _ => {}
    }
}

fn handle_focus_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('c') | KeyCode::Char('C') => {
            rt.cycle_category_for_activity(ActivityKind::Focus);
        }
        KeyCode::Char('m') | KeyCode::Char('M') => {
            let next_cadence = match rt.clock.cadence {
                CadencePreset::Harmonic => CadencePreset::DeepBlock,
                CadencePreset::DeepBlock => CadencePreset::Monolith,
                CadencePreset::Monolith => CadencePreset::Custom,
                CadencePreset::Custom => CadencePreset::Harmonic,
            };
            rt.clock.select_cadence(next_cadence, &rt.prefs.focus);
            rt.persist_active_clock();
            rt.set_toast(format!("Switched cadence to {}.", next_cadence.label()));
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            rt.clock.switch_activity(ActivityKind::Focus, &rt.prefs);
            rt.persist_active_clock();
        }
        KeyCode::Char('b') | KeyCode::Char('B') => {
            rt.clock.switch_activity(ActivityKind::ShortBreak, &rt.prefs);
            rt.persist_active_clock();
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            rt.clock.switch_activity(ActivityKind::LongBreak, &rt.prefs);
            rt.persist_active_clock();
        }
        KeyCode::Char('r') | KeyCode::Char('R') => {
            if rt.clock.status != ClockStatus::Idle {
                rt.complete_and_record_active_phase(false);
            } else {
                rt.clock.reset_phase();
            }
        }
        KeyCode::Enter => {
            if rt.clock.status != ClockStatus::Idle {
                rt.modal = Some(ModalDialog::ObjectiveAndNotesEditor {
                    field_index: 1,
                    objective_buf: rt.clock.objective.clone(),
                    notes_buf: rt.clock.completion_notes.clone(),
                    end_page_buf: rt
                        .clock
                        .end_page
                        .or(rt.clock.start_page)
                        .map(|p| p.to_string())
                        .unwrap_or_default(),
                    complete_on_submit: true,
                });
            }
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            rt.modal = Some(ModalDialog::ObjectiveAndNotesEditor {
                field_index: 0,
                objective_buf: rt.clock.objective.clone(),
                notes_buf: rt.clock.completion_notes.clone(),
                end_page_buf: rt
                    .clock
                    .end_page
                    .or(rt.clock.start_page)
                    .map(|p| p.to_string())
                    .unwrap_or_default(),
                complete_on_submit: false,
            });
        }
        KeyCode::Char('z') | KeyCode::Char('Z') => {
            rt.zen_mode = !rt.zen_mode;
        }
        _ => {}
    }
}

fn handle_reading_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    let shelf_len = rt.vault.reading_shelf.len();
    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            rt.reading_shelf_cursor = rt.reading_shelf_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if shelf_len > 0 {
                rt.reading_shelf_cursor = (rt.reading_shelf_cursor + 1).min(shelf_len - 1);
            }
        }
        KeyCode::Enter => {
            if rt.clock.status != ClockStatus::Idle
                && rt.clock.activity_kind == ActivityKind::Reading
            {
                rt.modal = Some(ModalDialog::ObjectiveAndNotesEditor {
                    field_index: 2,
                    objective_buf: rt.clock.objective.clone(),
                    notes_buf: rt.clock.completion_notes.clone(),
                    end_page_buf: rt
                        .clock
                        .end_page
                        .or(rt.clock.start_page)
                        .map(|p| p.to_string())
                        .unwrap_or_default(),
                    complete_on_submit: true,
                });
            } else if shelf_len > 0 {
                rt.launch_reading_material(rt.reading_shelf_cursor);
            }
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            rt.cycle_category_for_activity(ActivityKind::Reading);
        }
        KeyCode::Char('d') | KeyCode::Char('D') => rt.ask_delete_reading_material(rt.reading_shelf_cursor),
        KeyCode::Char('s') | KeyCode::Char('S') => rt.cycle_reading_status(rt.reading_shelf_cursor),
        KeyCode::Char('r') | KeyCode::Char('R') => {
            if rt.clock.status != ClockStatus::Idle {
                rt.complete_and_record_active_phase(false);
            } else {
                rt.clock.reset_phase();
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            let cat_id = rt
                .active_category_for(ActivityKind::Reading)
                .map(|c| c.id.clone())
                .unwrap_or_else(|| rt.prefs.reading.default_category_id.clone());
            rt.modal = Some(ModalDialog::ReadingMaterialEditor {
                editing_id: None,
                field_index: 0,
                title_buf: String::new(),
                author_buf: String::new(),
                kind: MaterialKind::Book,
                category_id: cat_id,
                total_pages_buf: "300".to_string(),
                current_page_buf: "0".to_string(),
                notes_buf: String::new(),
            });
        }
        KeyCode::Char('e') | KeyCode::Char('E') => {
            if let Some(mat) = rt.vault.reading_shelf.get(rt.reading_shelf_cursor) {
                rt.modal = Some(ModalDialog::ReadingMaterialEditor {
                    editing_id: Some(mat.id.clone()),
                    field_index: 0,
                    title_buf: mat.title.clone(),
                    author_buf: mat.author.clone(),
                    kind: mat.kind,
                    category_id: mat.category_id.clone(),
                    total_pages_buf: mat.total_pages.to_string(),
                    current_page_buf: mat.current_page.to_string(),
                    notes_buf: mat.notes.clone(),
                });
            }
        }
        KeyCode::Char('i') | KeyCode::Char('I') => {
            rt.modal = Some(ModalDialog::ObjectiveAndNotesEditor {
                field_index: 2,
                objective_buf: rt.clock.objective.clone(),
                notes_buf: rt.clock.completion_notes.clone(),
                end_page_buf: rt
                    .clock
                    .end_page
                    .or(rt.clock.start_page)
                    .map(|p| p.to_string())
                    .unwrap_or_default(),
                complete_on_submit: false,
            });
        }
        KeyCode::Char('z') | KeyCode::Char('Z') => {
            rt.zen_mode = !rt.zen_mode;
        }
        _ => {}
    }
}

fn handle_analytics_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Char('[') => rt.analytics_horizon = rt.analytics_horizon.prev(),
        KeyCode::Char(']') => rt.analytics_horizon = rt.analytics_horizon.next(),
        KeyCode::Left | KeyCode::Char('h') => {
            rt.heatmap_cursor_days_ago = (rt.heatmap_cursor_days_ago + 7).min(364);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            rt.heatmap_cursor_days_ago = rt.heatmap_cursor_days_ago.saturating_sub(7);
        }
        KeyCode::Up | KeyCode::Char('k') | KeyCode::Char(',') => {
            rt.heatmap_cursor_days_ago = (rt.heatmap_cursor_days_ago + 1).min(364);
        }
        KeyCode::Down | KeyCode::Char('j') | KeyCode::Char('.') => {
            rt.heatmap_cursor_days_ago = rt.heatmap_cursor_days_ago.saturating_sub(1);
        }
        KeyCode::Home => {
            rt.heatmap_cursor_days_ago = 0;
        }
        KeyCode::Tab => rt.analytics_lens = rt.analytics_lens.next(),
        KeyCode::BackTab => rt.analytics_lens = rt.analytics_lens.prev(),
        KeyCode::Char('c') | KeyCode::Char('C') => {
            let ids: Vec<String> = rt.vault.categories.iter().map(|c| c.id.clone()).collect();
            if ids.is_empty() {
                rt.analytics_category_filter = None;
                return;
            }
            match rt.analytics_category_filter {
                None => rt.analytics_category_filter = Some(ids[0].clone()),
                Some(ref cur) => {
                    let pos = ids.iter().position(|x| x == cur).unwrap_or(0);
                    if pos + 1 < ids.len() {
                        rt.analytics_category_filter = Some(ids[pos + 1].clone());
                    } else {
                        rt.analytics_category_filter = None;
                    }
                }
            }
        }
        KeyCode::Char('x') | KeyCode::Char('X') => rt.trigger_csv_export(),
        KeyCode::Char('J') | KeyCode::Char('o') | KeyCode::Char('O') => rt.trigger_json_export(),
        _ => {}
    }
}

fn handle_ledger_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    let filtered_ids: Vec<String> = rt
        .filtered_sessions()
        .iter()
        .map(|s| s.id.clone())
        .collect();
    let len = filtered_ids.len();

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            rt.ledger_cursor = rt.ledger_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if len > 0 {
                rt.ledger_cursor = (rt.ledger_cursor + 1).min(len - 1);
            }
        }
        KeyCode::Char('/') => {
            rt.ledger_search_editing = true;
        }
        KeyCode::Char('f') | KeyCode::Char('F') => {
            rt.ledger_activity_filter = match rt.ledger_activity_filter {
                None => Some(ActivityKind::Focus),
                Some(ActivityKind::Focus) => Some(ActivityKind::Reading),
                Some(ActivityKind::Reading) => Some(ActivityKind::ShortBreak),
                _ => None,
            };
            rt.ledger_cursor = 0;
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            rt.ledger_status_filter = match rt.ledger_status_filter {
                None => Some(CompletionState::Completed),
                Some(CompletionState::Completed) => Some(CompletionState::Interrupted),
                Some(CompletionState::Interrupted) => None,
            };
            rt.ledger_cursor = 0;
        }
        KeyCode::Char('c') | KeyCode::Char('C') => {
            let ids: Vec<String> = rt.vault.categories.iter().map(|c| c.id.clone()).collect();
            match rt.ledger_category_filter {
                None => rt.ledger_category_filter = ids.first().cloned(),
                Some(ref cur) => {
                    let pos = ids.iter().position(|x| x == cur).unwrap_or(0);
                    if pos + 1 < ids.len() {
                        rt.ledger_category_filter = Some(ids[pos + 1].clone());
                    } else {
                        rt.ledger_category_filter = None;
                    }
                }
            }
            rt.ledger_cursor = 0;
        }
        KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
            let cur_idx = rt.ledger_cursor.min(len.saturating_sub(1));
            if let Some(sid) = filtered_ids.get(cur_idx) {
                if let Some(s) = rt.vault.sessions.iter().find(|x| &x.id == sid) {
                    let cat_idx = rt
                        .vault
                        .categories
                        .iter()
                        .position(|c| c.id == s.category_id)
                        .unwrap_or(0);
                    let pages_str = s
                        .reading_meta
                        .as_ref()
                        .map(|m| m.pages_read.to_string())
                        .unwrap_or_else(|| "0".to_string());
                    rt.modal = Some(ModalDialog::SessionEditor {
                        session_id: s.id.clone(),
                        field_index: 0,
                        objective_buf: s.objective.clone(),
                        notes_buf: s.completion_notes.clone(),
                        elapsed_mins_buf: (s.elapsed_seconds / 60).max(1).to_string(),
                        category_idx: cat_idx,
                        pages_read_buf: pages_str,
                        completion_state: s.completion_state,
                    });
                }
            }
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            let cur_idx = rt.ledger_cursor.min(len.saturating_sub(1));
            if let Some(sid) = filtered_ids.get(cur_idx) {
                if let Some(s) = rt.vault.sessions.iter().find(|x| &x.id == sid) {
                    rt.modal = Some(ModalDialog::SessionDeleteConfirm {
                        session_id: s.id.clone(),
                        summary_label: format!(
                            "{} ({}, {})",
                            if s.objective.is_empty() {
                                s.activity_kind.label()
                            } else {
                                &s.objective
                            },
                            s.primary_date_key,
                            crate::domain::format_hours_minutes(s.elapsed_seconds)
                        ),
                    });
                }
            }
        }
        KeyCode::Char('x') | KeyCode::Char('X') => rt.trigger_csv_export(),
        KeyCode::Char('J') | KeyCode::Char('o') | KeyCode::Char('O') => rt.trigger_json_export(),
        _ => {}
    }
}

fn handle_achievements_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    let max_idx = match rt.achievements_family_filter {
        None => 23,
        Some(_) => 4,
    };
    match key.code {
        KeyCode::Left | KeyCode::Char('h') => {
            rt.achievements_cursor = rt.achievements_cursor.saturating_sub(1);
        }
        KeyCode::Right | KeyCode::Char('l') => {
            rt.achievements_cursor = (rt.achievements_cursor + 1).min(max_idx);
        }
        KeyCode::Up | KeyCode::Char('k') => {
            rt.achievements_cursor = rt.achievements_cursor.saturating_sub(3);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            rt.achievements_cursor = (rt.achievements_cursor + 3).min(max_idx);
        }
        KeyCode::Tab => {
            rt.achievements_family_filter = match rt.achievements_family_filter {
                None => Some(AchievementFamily::Consistency),
                Some(AchievementFamily::Consistency) => Some(AchievementFamily::DeepWork),
                Some(AchievementFamily::DeepWork) => Some(AchievementFamily::Reading),
                Some(AchievementFamily::Reading) => Some(AchievementFamily::Exploration),
                Some(AchievementFamily::Exploration) => {
                    Some(AchievementFamily::PersonalImprovement)
                }
                Some(AchievementFamily::PersonalImprovement) => {
                    Some(AchievementFamily::LongTermMilestones)
                }
                Some(AchievementFamily::LongTermMilestones) => None,
            };
            rt.achievements_cursor = 0;
        }
        _ => {}
    }
}

fn handle_categories_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    let visible_ids: Vec<String> = rt
        .vault
        .categories
        .iter()
        .filter(|c| rt.show_archived_categories || !c.is_archived)
        .map(|c| c.id.clone())
        .collect();
    let len = visible_ids.len();

    match key.code {
        KeyCode::Up | KeyCode::Char('k') => {
            rt.category_cursor = rt.category_cursor.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            if len > 0 {
                rt.category_cursor = (rt.category_cursor + 1).min(len - 1);
            }
        }
        KeyCode::Char('n') | KeyCode::Char('N') => {
            rt.open_category_editor(None);
        }
        KeyCode::Char('e') | KeyCode::Char('E') | KeyCode::Enter => {
            let idx = rt.category_cursor.min(len.saturating_sub(1));
            if let Some(cid) = visible_ids.get(idx).cloned() {
                rt.open_category_editor(Some(cid));
            }
        }
        KeyCode::Char('s') | KeyCode::Char('S') => {
            let idx = rt.category_cursor.min(len.saturating_sub(1));
            if let Some(cid) = visible_ids.get(idx).cloned() {
                if let Some(cat) = rt.vault.categories.iter().find(|c| c.id == cid).cloned() {
                    if !cat.is_archived {
                        if cat.scope.supports_activity(ActivityKind::Focus) {
                            rt.prefs.focus.default_category_id = cat.id.clone();
                        }
                        if cat.scope.supports_activity(ActivityKind::Reading) {
                            rt.prefs.reading.default_category_id = cat.id.clone();
                        }
                        if rt.clock.status == ClockStatus::Idle {
                            rt.clock.category_id = cat.id.clone();
                        }
                        let _ = save_preferences(&rt.paths, &rt.prefs);
                        rt.persist_active_clock();
                        rt.set_toast(format!(
                            "Set '{}' [{}] as active skill class.",
                            cat.name,
                            cat.scope.label()
                        ));
                    }
                }
            }
        }
        KeyCode::Char('a') | KeyCode::Char('A') => {
            rt.toggle_selected_category_archive();
        }
        KeyCode::Char('v') | KeyCode::Char('V') => {
            rt.show_archived_categories = !rt.show_archived_categories;
            rt.category_cursor = 0;
        }
        KeyCode::Char('d') | KeyCode::Char('D') => {
            rt.open_category_delete_dialog();
        }
        _ => {}
    }
}

fn handle_settings_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    match key.code {
        KeyCode::Tab => {
            rt.settings_section_idx = (rt.settings_section_idx + 1) % SETTINGS_SECTIONS.len();
            rt.settings_row_idx = 0;
        }
        KeyCode::BackTab => {
            rt.settings_section_idx =
                (rt.settings_section_idx + SETTINGS_SECTIONS.len() - 1) % SETTINGS_SECTIONS.len();
            rt.settings_row_idx = 0;
        }
        KeyCode::Up | KeyCode::Char('k') => {
            rt.settings_row_idx = rt.settings_row_idx.saturating_sub(1);
        }
        KeyCode::Down | KeyCode::Char('j') => {
            let max_rows = crate::tui::views::settings::build_section_rows(rt).len();
            if max_rows > 0 {
                rt.settings_row_idx = (rt.settings_row_idx + 1).min(max_rows - 1);
            }
        }
        KeyCode::Char('b') | KeyCode::Char('B') => {
            rt.trigger_manual_backup();
        }
        KeyCode::Char('l') | KeyCode::Char('L') => {
            rt.open_latest_backup_restore_modal();
        }
        KeyCode::Left | KeyCode::Right | KeyCode::Enter => {
            let dir: i32 = if key.code == KeyCode::Left { -1 } else { 1 };
            apply_settings_adjustment(rt, dir);
        }
        _ => {}
    }
}

fn apply_settings_adjustment(rt: &mut WorkspaceRuntime, dir: i32) {
    let sec = rt.settings_section_idx % SETTINGS_SECTIONS.len();
    let row = rt.settings_row_idx;

    match (sec, row) {
        (0, 0) => {
            let cur = ThemeId::from_key(&rt.prefs.appearance.theme);
            rt.prefs.appearance.theme = cur.next().key().to_string();
        }
        (0, 1) => {
            rt.prefs.appearance.density = rt.prefs.appearance.density.next();
        }
        (0, 2) => {
            rt.prefs.appearance.motion = rt.prefs.appearance.motion.next();
        }
        (0, 3) => {
            rt.prefs.appearance.show_gamification = !rt.prefs.appearance.show_gamification;
        }
        (0, 4) => {
            rt.prefs.appearance.show_navigation_labels =
                !rt.prefs.appearance.show_navigation_labels;
        }
        (1, 0) => {
            rt.prefs.focus.active_cadence = match rt.prefs.focus.active_cadence {
                CadencePreset::Harmonic => CadencePreset::DeepBlock,
                CadencePreset::DeepBlock => CadencePreset::Monolith,
                CadencePreset::Monolith => CadencePreset::Custom,
                CadencePreset::Custom => CadencePreset::Harmonic,
            };
            rt.clock
                .select_cadence(rt.prefs.focus.active_cadence, &rt.prefs.focus);
        }
        (1, 1) => {
            rt.prefs.focus.custom_focus_minutes =
                (rt.prefs.focus.custom_focus_minutes as i32 + dir * 5).clamp(5, 240) as u32;
        }
        (1, 2) => {
            rt.prefs.focus.custom_short_break_minutes =
                (rt.prefs.focus.custom_short_break_minutes as i32 + dir).clamp(1, 120) as u32;
        }
        (1, 3) => {
            rt.prefs.focus.custom_long_break_minutes =
                (rt.prefs.focus.custom_long_break_minutes as i32 + dir * 5).clamp(5, 180) as u32;
        }
        (1, 4) => {
            rt.prefs.focus.auto_start_breaks = !rt.prefs.focus.auto_start_breaks;
        }
        (1, 5) => {
            rt.prefs.focus.count_interrupted_in_totals =
                !rt.prefs.focus.count_interrupted_in_totals;
        }
        (2, 0) => {
            rt.prefs.reading.default_duration_minutes =
                (rt.prefs.reading.default_duration_minutes as i32 + dir * 5).clamp(5, 240) as u32;
        }
        (2, 1) => {
            rt.prefs.reading.track_pages_read = !rt.prefs.reading.track_pages_read;
        }
        (2, 2) => {
            rt.prefs.reading.prompt_notes_on_complete =
                !rt.prefs.reading.prompt_notes_on_complete;
        }
        (3, 0) => {
            rt.prefs.goals.daily_focus_minutes =
                (rt.prefs.goals.daily_focus_minutes as i32 + dir * 15).clamp(0, 720) as u32;
        }
        (3, 1) => {
            rt.prefs.goals.weekly_focus_minutes =
                (rt.prefs.goals.weekly_focus_minutes as i32 + dir * 60).clamp(0, 4200) as u32;
        }
        (3, 2) => {
            rt.prefs.goals.daily_reading_minutes =
                (rt.prefs.goals.daily_reading_minutes as i32 + dir * 10).clamp(0, 480) as u32;
        }
        (3, 3) => {
            rt.prefs.goals.weekly_reading_minutes =
                (rt.prefs.goals.weekly_reading_minutes as i32 + dir * 30).clamp(0, 2400) as u32;
        }
        (4, 0) => {
            rt.prefs.notifications.desktop_notifications =
                !rt.prefs.notifications.desktop_notifications;
        }
        (4, 1) => {
            rt.prefs.notifications.sound_enabled = !rt.prefs.notifications.sound_enabled;
        }
        (4, 2) => {
            rt.prefs.notifications.status_bar_export = !rt.prefs.notifications.status_bar_export;
        }
        (5, 0) => {
            rt.trigger_manual_backup();
            return;
        }
        (5, 1) => {
            rt.open_latest_backup_restore_modal();
            return;
        }
        (5, 2) => {
            rt.trigger_csv_export();
            rt.trigger_json_export();
            return;
        }
        (5, 3) => {
            rt.modal = Some(ModalDialog::DestructiveResetConfirm {
                operation: ResetOperation::StatisticsOnly,
                typed_phrase: String::new(),
            });
            return;
        }
        (5, 4) => {
            rt.modal = Some(ModalDialog::DestructiveResetConfirm {
                operation: ResetOperation::ProgressionOnly,
                typed_phrase: String::new(),
            });
            return;
        }
        (5, 5) => {
            rt.modal = Some(ModalDialog::DestructiveResetConfirm {
                operation: ResetOperation::PreferencesOnly,
                typed_phrase: String::new(),
            });
            return;
        }
        (5, 6) => {
            rt.modal = Some(ModalDialog::DestructiveResetConfirm {
                operation: ResetOperation::FactoryEverything,
                typed_phrase: String::new(),
            });
            return;
        }
        _ => {}
    }

    let _ = save_preferences(&rt.paths, &rt.prefs);
    rt.set_toast("Saved preference update to preferences.toml.");
}

fn handle_modal_key(rt: &mut WorkspaceRuntime, key: KeyEvent) {
    if key.code == KeyCode::Esc {
        rt.modal = None;
        return;
    }

    let Some(modal_clone) = rt.modal.clone() else {
        return;
    };

    match modal_clone {
        ModalDialog::ShortcutHelp | ModalDialog::CelebrationBanner { .. } => {
            if matches!(
                key.code,
                KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('?')
            ) {
                rt.modal = None;
            }
        }
        ModalDialog::QuitConfirmation => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                rt.complete_and_record_active_phase(false);
                rt.should_quit = true;
            }
            KeyCode::Char('s') | KeyCode::Char('S') => {
                rt.clock.pause(Utc::now());
                rt.persist_active_clock();
                rt.should_quit = true;
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                rt.modal = None;
            }
            _ => {}
        },
        ModalDialog::ObjectiveAndNotesEditor {
            mut field_index,
            mut objective_buf,
            mut notes_buf,
            mut end_page_buf,
            complete_on_submit,
        } => {
            let max_fields = if rt.clock.activity_kind == ActivityKind::Reading {
                3
            } else {
                2
            };
            match key.code {
                KeyCode::Tab | KeyCode::Down => field_index = (field_index + 1) % max_fields,
                KeyCode::BackTab | KeyCode::Up => {
                    field_index = (field_index + max_fields - 1) % max_fields
                }
                KeyCode::Backspace => match field_index {
                    0 => {
                        objective_buf.pop();
                    }
                    1 => {
                        notes_buf.pop();
                    }
                    _ => {
                        end_page_buf.pop();
                    }
                },
                KeyCode::Char(ch) => match field_index {
                    0 => objective_buf.push(ch),
                    1 => notes_buf.push(ch),
                    _ if ch.is_ascii_digit() => end_page_buf.push(ch),
                    _ => {}
                },
                KeyCode::Enter => {
                    rt.clock.objective = objective_buf.trim().to_string();
                    rt.clock.completion_notes = notes_buf.trim().to_string();
                    if let Ok(ep) = end_page_buf.trim().parse::<u32>() {
                        rt.clock.end_page = Some(ep);
                    }
                    rt.modal = None;
                    if complete_on_submit {
                        rt.complete_and_record_active_phase(true);
                    } else {
                        rt.persist_active_clock();
                        rt.set_toast("Updated active session metadata.");
                    }
                    return;
                }
                _ => {}
            }
            rt.modal = Some(ModalDialog::ObjectiveAndNotesEditor {
                field_index,
                objective_buf,
                notes_buf,
                end_page_buf,
                complete_on_submit,
            });
        }
        ModalDialog::CategoryEditor {
            editing_id,
            mut field_index,
            mut name_buf,
            mut color_idx,
            mut icon_idx,
            mut scope,
            mut daily_goal_minutes,
            mut weekly_goal_minutes,
            mut description_buf,
        } => {
            match key.code {
                KeyCode::Tab | KeyCode::Down => field_index = (field_index + 1) % 7,
                KeyCode::BackTab | KeyCode::Up => field_index = (field_index + 6) % 7,
                KeyCode::Left => match field_index {
                    1 => {
                        color_idx = (color_idx + APPROVED_CATEGORY_COLORS.len() - 1)
                            % APPROVED_CATEGORY_COLORS.len()
                    }
                    2 => {
                        icon_idx = (icon_idx + CATEGORY_ICON_GLYPHS.len() - 1)
                            % CATEGORY_ICON_GLYPHS.len()
                    }
                    3 => {
                        scope = match scope {
                            CategoryScope::FocusOnly => CategoryScope::Universal,
                            CategoryScope::ReadingOnly => CategoryScope::FocusOnly,
                            CategoryScope::Universal => CategoryScope::ReadingOnly,
                        }
                    }
                    4 => daily_goal_minutes = daily_goal_minutes.saturating_sub(15),
                    5 => weekly_goal_minutes = weekly_goal_minutes.saturating_sub(30),
                    _ => {}
                },
                KeyCode::Right => match field_index {
                    1 => color_idx = (color_idx + 1) % APPROVED_CATEGORY_COLORS.len(),
                    2 => icon_idx = (icon_idx + 1) % CATEGORY_ICON_GLYPHS.len(),
                    3 => {
                        scope = match scope {
                            CategoryScope::FocusOnly => CategoryScope::ReadingOnly,
                            CategoryScope::ReadingOnly => CategoryScope::Universal,
                            CategoryScope::Universal => CategoryScope::FocusOnly,
                        }
                    }
                    4 => daily_goal_minutes = (daily_goal_minutes + 15).min(720),
                    5 => weekly_goal_minutes = (weekly_goal_minutes + 30).min(5040),
                    _ => {}
                },
                KeyCode::Backspace => match field_index {
                    0 => {
                        name_buf.pop();
                    }
                    6 => {
                        description_buf.pop();
                    }
                    _ => {}
                },
                KeyCode::Char(ch) => match field_index {
                    0 => name_buf.push(ch),
                    6 => description_buf.push(ch),
                    _ => {}
                },
                KeyCode::Enter => {
                    rt.modal = Some(ModalDialog::CategoryEditor {
                        editing_id,
                        field_index,
                        name_buf,
                        color_idx,
                        icon_idx,
                        scope,
                        daily_goal_minutes,
                        weekly_goal_minutes,
                        description_buf,
                    });
                    rt.submit_category_editor();
                    return;
                }
                _ => {}
            }
            rt.modal = Some(ModalDialog::CategoryEditor {
                editing_id,
                field_index,
                name_buf,
                color_idx,
                icon_idx,
                scope,
                daily_goal_minutes,
                weekly_goal_minutes,
                description_buf,
            });
        }
        ModalDialog::CategoryDeleteConfirm {
            category_id,
            category_name,
            linked_sessions_count,
            mut use_archive_instead,
            mut replacement_category_idx,
        } => {
            match key.code {
                KeyCode::Tab => use_archive_instead = !use_archive_instead,
                KeyCode::Left => {
                    replacement_category_idx = replacement_category_idx.saturating_sub(1)
                }
                KeyCode::Right => replacement_category_idx += 1,
                KeyCode::Enter => {
                    rt.modal = Some(ModalDialog::CategoryDeleteConfirm {
                        category_id,
                        category_name,
                        linked_sessions_count,
                        use_archive_instead,
                        replacement_category_idx,
                    });
                    rt.confirm_category_delete();
                    return;
                }
                _ => {}
            }
            rt.modal = Some(ModalDialog::CategoryDeleteConfirm {
                category_id,
                category_name,
                linked_sessions_count,
                use_archive_instead,
                replacement_category_idx,
            });
        }
        ModalDialog::ReadingMaterialEditor {
            editing_id,
            mut field_index,
            mut title_buf,
            mut author_buf,
            mut kind,
            mut category_id,
            mut total_pages_buf,
            mut current_page_buf,
            mut notes_buf,
        } => {
            match key.code {
                KeyCode::Tab | KeyCode::Down => field_index = (field_index + 1) % 7,
                KeyCode::BackTab | KeyCode::Up => field_index = (field_index + 6) % 7,
                KeyCode::Left | KeyCode::Right if field_index == 2 => kind = kind.next(),
                KeyCode::Left | KeyCode::Right if field_index == 6 => {
                    let read_ids: Vec<String> = rt
                        .vault
                        .categories
                        .iter()
                        .filter(|c| !c.is_archived && c.scope.supports_activity(ActivityKind::Reading))
                        .map(|c| c.id.clone())
                        .collect();
                    if !read_ids.is_empty() {
                        let pos = read_ids.iter().position(|id| id == &category_id).unwrap_or(0);
                        let next_pos = if key.code == KeyCode::Left {
                            (pos + read_ids.len() - 1) % read_ids.len()
                        } else {
                            (pos + 1) % read_ids.len()
                        };
                        category_id = read_ids[next_pos].clone();
                    }
                }
                KeyCode::Backspace => match field_index {
                    0 => {
                        title_buf.pop();
                    }
                    1 => {
                        author_buf.pop();
                    }
                    3 => {
                        total_pages_buf.pop();
                    }
                    4 => {
                        current_page_buf.pop();
                    }
                    5 => {
                        notes_buf.pop();
                    }
                    _ => {}
                },
                KeyCode::Char(ch) => match field_index {
                    0 => title_buf.push(ch),
                    1 => author_buf.push(ch),
                    3 if ch.is_ascii_digit() => total_pages_buf.push(ch),
                    4 if ch.is_ascii_digit() => current_page_buf.push(ch),
                    5 => notes_buf.push(ch),
                    _ => {}
                },
                KeyCode::Enter => {
                    rt.modal = Some(ModalDialog::ReadingMaterialEditor {
                        editing_id,
                        field_index,
                        title_buf,
                        author_buf,
                        kind,
                        category_id,
                        total_pages_buf,
                        current_page_buf,
                        notes_buf,
                    });
                    rt.submit_reading_material_editor();
                    return;
                }
                _ => {}
            }
            rt.modal = Some(ModalDialog::ReadingMaterialEditor {
                editing_id,
                field_index,
                title_buf,
                author_buf,
                kind,
                category_id,
                total_pages_buf,
                current_page_buf,
                notes_buf,
            });
        }
        ModalDialog::PlannedBlockEditor {
            mut field_index,
            mut title_buf,
            mut activity_kind,
            mut category_id,
            mut duration_minutes,
            mut priority,
        } => {
            match key.code {
                KeyCode::Tab | KeyCode::Down => field_index = (field_index + 1) % 5,
                KeyCode::BackTab | KeyCode::Up => field_index = (field_index + 4) % 5,
                KeyCode::Left | KeyCode::Right => match field_index {
                    1 => {
                        activity_kind = if activity_kind == ActivityKind::Focus {
                            ActivityKind::Reading
                        } else {
                            ActivityKind::Focus
                        };
                        if let Some(c) = rt.active_category_for(activity_kind) {
                            category_id = c.id.clone();
                        }
                    }
                    2 => {
                        let matching_ids: Vec<String> = rt
                            .vault
                            .categories
                            .iter()
                            .filter(|c| !c.is_archived && c.scope.supports_activity(activity_kind))
                            .map(|c| c.id.clone())
                            .collect();
                        if !matching_ids.is_empty() {
                            let pos = matching_ids.iter().position(|id| id == &category_id).unwrap_or(0);
                            let next_pos = if key.code == KeyCode::Left {
                                (pos + matching_ids.len() - 1) % matching_ids.len()
                            } else {
                                (pos + 1) % matching_ids.len()
                            };
                            category_id = matching_ids[next_pos].clone();
                        }
                    }
                    3 => {
                        let d: i32 = if key.code == KeyCode::Left { -5 } else { 5 };
                        duration_minutes = (duration_minutes as i32 + d).clamp(5, 240) as u32;
                    }
                    4 => priority = priority.next(),
                    _ => {}
                },
                KeyCode::Backspace if field_index == 0 => {
                    title_buf.pop();
                }
                KeyCode::Char(ch) if field_index == 0 => {
                    title_buf.push(ch);
                }
                KeyCode::Enter => {
                    rt.modal = Some(ModalDialog::PlannedBlockEditor {
                        field_index,
                        title_buf,
                        activity_kind,
                        category_id,
                        duration_minutes,
                        priority,
                    });
                    rt.submit_planned_block_editor();
                    return;
                }
                _ => {}
            }
            rt.modal = Some(ModalDialog::PlannedBlockEditor {
                field_index,
                title_buf,
                activity_kind,
                category_id,
                duration_minutes,
                priority,
            });
        }
        ModalDialog::SessionEditor {
            session_id,
            mut field_index,
            mut objective_buf,
            mut notes_buf,
            mut elapsed_mins_buf,
            mut category_idx,
            mut pages_read_buf,
            mut completion_state,
        } => {
            match key.code {
                KeyCode::Tab | KeyCode::Down => field_index = (field_index + 1) % 6,
                KeyCode::BackTab | KeyCode::Up => field_index = (field_index + 5) % 6,
                KeyCode::Left | KeyCode::Right => match field_index {
                    3 => {
                        let n = rt.vault.categories.len().max(1);
                        category_idx = if key.code == KeyCode::Left {
                            (category_idx + n - 1) % n
                        } else {
                            (category_idx + 1) % n
                        };
                    }
                    5 => {
                        completion_state = match completion_state {
                            CompletionState::Completed => CompletionState::Interrupted,
                            CompletionState::Interrupted => CompletionState::Completed,
                        }
                    }
                    _ => {}
                },
                KeyCode::Backspace => match field_index {
                    0 => {
                        objective_buf.pop();
                    }
                    1 => {
                        notes_buf.pop();
                    }
                    2 => {
                        elapsed_mins_buf.pop();
                    }
                    4 => {
                        pages_read_buf.pop();
                    }
                    _ => {}
                },
                KeyCode::Char(ch) => match field_index {
                    0 => objective_buf.push(ch),
                    1 => notes_buf.push(ch),
                    2 if ch.is_ascii_digit() => elapsed_mins_buf.push(ch),
                    4 if ch.is_ascii_digit() => pages_read_buf.push(ch),
                    _ => {}
                },
                KeyCode::Enter => {
                    rt.modal = Some(ModalDialog::SessionEditor {
                        session_id,
                        field_index,
                        objective_buf,
                        notes_buf,
                        elapsed_mins_buf,
                        category_idx,
                        pages_read_buf,
                        completion_state,
                    });
                    rt.submit_session_editor();
                    return;
                }
                _ => {}
            }
            rt.modal = Some(ModalDialog::SessionEditor {
                session_id,
                field_index,
                objective_buf,
                notes_buf,
                elapsed_mins_buf,
                category_idx,
                pages_read_buf,
                completion_state,
            });
        }
        ModalDialog::ReadingDeleteConfirm { .. } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => rt.confirm_reading_delete(),
            KeyCode::Char('n') | KeyCode::Char('N') => rt.modal = None,
            _ => {}
        },
        ModalDialog::SessionDeleteConfirm { .. } => match key.code {
            KeyCode::Char('y') | KeyCode::Char('Y') | KeyCode::Enter => {
                rt.confirm_session_delete();
            }
            KeyCode::Char('n') | KeyCode::Char('N') => {
                rt.modal = None;
            }
            _ => {}
        },
        ModalDialog::BackupRestoreConfirm {
            backup_path,
            raw_json,
            preview,
            mut strategy,
        } => {
            match key.code {
                KeyCode::Tab | KeyCode::Left | KeyCode::Right => strategy = strategy.next(),
                KeyCode::Enter => {
                    rt.modal = Some(ModalDialog::BackupRestoreConfirm {
                        backup_path,
                        raw_json,
                        preview,
                        strategy,
                    });
                    rt.confirm_backup_restore();
                    return;
                }
                _ => {}
            }
            rt.modal = Some(ModalDialog::BackupRestoreConfirm {
                backup_path,
                raw_json,
                preview,
                strategy,
            });
        }
        ModalDialog::DestructiveResetConfirm {
            operation,
            mut typed_phrase,
        } => {
            match key.code {
                KeyCode::Backspace => {
                    typed_phrase.pop();
                }
                KeyCode::Char(ch) => {
                    typed_phrase.push(ch);
                }
                KeyCode::Enter => {
                    rt.modal = Some(ModalDialog::DestructiveResetConfirm {
                        operation,
                        typed_phrase,
                    });
                    rt.confirm_destructive_reset();
                    return;
                }
                _ => {}
            }
            rt.modal = Some(ModalDialog::DestructiveResetConfirm {
                operation,
                typed_phrase,
            });
        }
    }
}

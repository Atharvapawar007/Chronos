//! Unidirectional Workspace Runtime & State Controller (`runtime.rs`).
//!
//! Coordinates all 8 primary destinations (`PrimaryRoute`), active timer (`ClockEngine`),
//! persistent storage (`ChronosVault` + `Preferences`), audio/notification synthesis,
//! and the Material 3 modal dialog stack (`ModalDialog`).

use crate::audio::AudioNotifier;
use crate::domain::{
    add_planned_block, add_reading_material, commit_session_record, create_category,
    delete_category_safely, delete_session_record, edit_session_record, evaluate_goals,
    evaluate_progression, format_duration_hms, set_category_archived, to_local_date_key,
    update_category, update_reading_material, AchievementFamily, AchievementStatusItem,
    ActivityKind, CategoryDeleteStrategy, CategoryDraft, CategoryScope, ClockEngine, ClockStatus,
    CompletionState, GoalsEvaluation, HorizonFilter, LevelSnapshot, MaterialKind, PlanPriority,
    PlanStatus, Preferences, ReadingMaterialDraft, SessionEditPatch, APPROVED_CATEGORY_COLORS,
    CATEGORY_ICON_GLYPHS,
};
use crate::error::ChronosResult;
use crate::storage::{
    apply_backup_restore, create_verified_backup, execute_guarded_reset, export_sessions_csv_file,
    export_structured_json_file, inspect_and_validate_backup, list_available_backups,
    load_preferences, load_vault, remove_status_bar_snapshot, save_preferences, save_vault,
    write_status_bar_snapshot, BackupConflictPreview, ChronosPaths, ChronosVault, ResetOperation,
    RestoreStrategy,
};
use crate::tui::theme::{Palette, ThemeId};
use chrono::Utc;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrimaryRoute {
    Overview,
    FocusStudio,
    ReadingSanctuary,
    Analytics,
    SessionLedger,
    AchievementsHall,
    CategoryStudio,
    SettingsCenter,
    Guide,
}

impl PrimaryRoute {
    pub const ALL: [PrimaryRoute; 9] = [
        PrimaryRoute::Overview,
        PrimaryRoute::FocusStudio,
        PrimaryRoute::ReadingSanctuary,
        PrimaryRoute::Analytics,
        PrimaryRoute::SessionLedger,
        PrimaryRoute::AchievementsHall,
        PrimaryRoute::CategoryStudio,
        PrimaryRoute::SettingsCenter,
        PrimaryRoute::Guide,
    ];

    pub fn index(self) -> usize {
        match self {
            PrimaryRoute::Overview => 0,
            PrimaryRoute::FocusStudio => 1,
            PrimaryRoute::ReadingSanctuary => 2,
            PrimaryRoute::Analytics => 3,
            PrimaryRoute::SessionLedger => 4,
            PrimaryRoute::AchievementsHall => 5,
            PrimaryRoute::CategoryStudio => 6,
            PrimaryRoute::SettingsCenter => 7,
            PrimaryRoute::Guide => 8,
        }
    }

    pub fn from_index(idx: usize) -> Self {
        Self::ALL[idx % Self::ALL.len()]
    }

    pub fn key_num(self) -> &'static str {
        match self {
            PrimaryRoute::Overview => "1",
            PrimaryRoute::FocusStudio => "2",
            PrimaryRoute::ReadingSanctuary => "3",
            PrimaryRoute::Analytics => "4",
            PrimaryRoute::SessionLedger => "5",
            PrimaryRoute::AchievementsHall => "6",
            PrimaryRoute::CategoryStudio => "7",
            PrimaryRoute::SettingsCenter => "8",
            PrimaryRoute::Guide => "9",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            PrimaryRoute::Overview => "◈",
            PrimaryRoute::FocusStudio => "◉",
            PrimaryRoute::ReadingSanctuary => "▣",
            PrimaryRoute::Analytics => "▲",
            PrimaryRoute::SessionLedger => "≡",
            PrimaryRoute::AchievementsHall => "✦",
            PrimaryRoute::CategoryStudio => "◆",
            PrimaryRoute::SettingsCenter => "❖",
            PrimaryRoute::Guide => "?",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            PrimaryRoute::Overview => "Home",
            PrimaryRoute::FocusStudio => "Focus",
            PrimaryRoute::ReadingSanctuary => "Reading",
            PrimaryRoute::Analytics => "Stats",
            PrimaryRoute::SessionLedger => "Log",
            PrimaryRoute::AchievementsHall => "Trophies",
            PrimaryRoute::CategoryStudio => "Skills",
            PrimaryRoute::SettingsCenter => "Settings",
            PrimaryRoute::Guide => "Guide",
        }
    }

    pub fn next(self) -> Self {
        Self::from_index(self.index() + 1)
    }

    pub fn prev(self) -> Self {
        Self::from_index((self.index() + Self::ALL.len() - 1) % Self::ALL.len())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnalyticsLens {
    Summary,
    TrendsAndRhythm,
    CategoryMatrix,
    DayExplorer,
    WeeklyReflection,
}

impl AnalyticsLens {
    pub const ALL: [AnalyticsLens; 5] = [
        AnalyticsLens::Summary,
        AnalyticsLens::TrendsAndRhythm,
        AnalyticsLens::CategoryMatrix,
        AnalyticsLens::DayExplorer,
        AnalyticsLens::WeeklyReflection,
    ];

    pub fn index(self) -> usize {
        Self::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }

    pub fn label(self) -> &'static str {
        match self {
            AnalyticsLens::Summary => "Overview",
            AnalyticsLens::TrendsAndRhythm => "Rhythm",
            AnalyticsLens::CategoryMatrix => "Skills",
            AnalyticsLens::DayExplorer => "Days",
            AnalyticsLens::WeeklyReflection => "Review",
        }
    }

    pub fn next(self) -> Self {
        Self::ALL[(self.index() + 1) % Self::ALL.len()]
    }

    pub fn prev(self) -> Self {
        Self::ALL[(self.index() + Self::ALL.len() - 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone)]
pub enum ModalDialog {
    ShortcutHelp,
    QuitConfirmation,
    ObjectiveAndNotesEditor {
        field_index: usize,
        objective_buf: String,
        notes_buf: String,
        end_page_buf: String,
        complete_on_submit: bool,
    },
    CategoryEditor {
        editing_id: Option<String>,
        field_index: usize,
        name_buf: String,
        color_idx: usize,
        icon_idx: usize,
        scope: CategoryScope,
        daily_goal_minutes: u32,
        weekly_goal_minutes: u32,
        description_buf: String,
    },
    CategoryDeleteConfirm {
        category_id: String,
        category_name: String,
        linked_sessions_count: usize,
        use_archive_instead: bool,
        replacement_category_idx: usize,
    },
    ReadingMaterialEditor {
        editing_id: Option<String>,
        field_index: usize,
        title_buf: String,
        author_buf: String,
        kind: MaterialKind,
        category_id: String,
        total_pages_buf: String,
        current_page_buf: String,
        notes_buf: String,
    },
    PlannedBlockEditor {
        field_index: usize,
        title_buf: String,
        activity_kind: ActivityKind,
        category_id: String,
        duration_minutes: u32,
        priority: PlanPriority,
    },
    SessionEditor {
        session_id: String,
        field_index: usize,
        objective_buf: String,
        notes_buf: String,
        elapsed_mins_buf: String,
        category_idx: usize,
        pages_read_buf: String,
        completion_state: CompletionState,
    },
    SessionDeleteConfirm {
        session_id: String,
        summary_label: String,
    },
    ReadingDeleteConfirm {
        material_id: String,
        title: String,
        sessions_count: usize,
    },
    BackupRestoreConfirm {
        backup_path: PathBuf,
        raw_json: String,
        preview: BackupConflictPreview,
        strategy: RestoreStrategy,
    },
    DestructiveResetConfirm {
        operation: ResetOperation,
        typed_phrase: String,
    },
    CelebrationBanner {
        headline: String,
        subtitle: String,
        unlocked_titles: Vec<(String, String, u64)>,
        completed_full_cycle: bool,
        opened_at: Instant,
    },
}

pub struct WorkspaceRuntime {
    pub paths: ChronosPaths,
    pub prefs: Preferences,
    pub vault: ChronosVault,
    pub clock: ClockEngine,
    pub route: PrimaryRoute,
    pub modal: Option<ModalDialog>,
    pub zen_mode: bool,
    pub should_quit: bool,
    pub audio: AudioNotifier,
    pub toast: Option<(String, Instant)>,

    pub overview_plan_cursor: usize,
    pub reading_shelf_cursor: usize,
    pub category_cursor: usize,
    pub show_archived_categories: bool,
    pub analytics_horizon: HorizonFilter,
    pub analytics_lens: AnalyticsLens,
    pub analytics_category_filter: Option<String>,
    pub ledger_cursor: usize,
    pub ledger_search_query: String,
    pub ledger_search_editing: bool,
    pub ledger_activity_filter: Option<ActivityKind>,
    pub ledger_status_filter: Option<CompletionState>,
    pub ledger_category_filter: Option<String>,
    pub achievements_family_filter: Option<AchievementFamily>,
    pub achievements_cursor: usize,
    pub settings_section_idx: usize,
    pub settings_row_idx: usize,
    pub available_backups: Vec<PathBuf>,
    pub frame_tick: u64,
    pub heatmap_cursor_days_ago: u16,
    pub guide_topic: usize,
    pub guide_scroll: u16,
    /// Set by the Guide screen while drawing, so scrolling stops exactly at the last line.
    pub guide_max_scroll: std::cell::Cell<u16>,
}

impl WorkspaceRuntime {
    pub fn bootstrap() -> ChronosResult<Self> {
        let now_utc = Utc::now();
        let paths = ChronosPaths::resolve();
        let (prefs, pref_warn) = load_preferences(&paths);
        let (mut vault, mut notices) = load_vault(&paths, &prefs, now_utc)?;
        if let Some(w) = pref_warn {
            notices.insert(0, w);
        }

        crate::domain::reconcile_reading_shelf_totals(&mut vault.reading_shelf, &vault.sessions);

        let mut clock = vault
            .active_clock
            .take()
            .unwrap_or_else(|| ClockEngine::new_from_prefs(&prefs));

        if clock.recover_after_restart(now_utc) {
            notices.push(format!(
                "Recovered interrupted {} session ({} elapsed).",
                clock.activity_kind.short_label(),
                format_duration_hms(clock.accumulated_elapsed_seconds)
            ));
        }

        let initial_toast = notices.first().cloned().map(|msg| (msg, Instant::now()));
        let available_backups = list_available_backups(&paths);
        let zen_default = prefs.focus.zen_mode_default;

        let mut rt = Self {
            paths,
            prefs,
            vault,
            clock,
            route: PrimaryRoute::Overview,
            modal: None,
            zen_mode: zen_default,
            should_quit: false,
            audio: AudioNotifier::new(),
            toast: initial_toast,
            overview_plan_cursor: 0,
            reading_shelf_cursor: 0,
            category_cursor: 0,
            show_archived_categories: false,
            analytics_horizon: HorizonFilter::Days7,
            analytics_lens: AnalyticsLens::Summary,
            analytics_category_filter: None,
            ledger_cursor: 0,
            ledger_search_query: String::new(),
            ledger_search_editing: false,
            ledger_activity_filter: None,
            ledger_status_filter: None,
            ledger_category_filter: None,
            achievements_family_filter: None,
            achievements_cursor: 0,
            settings_section_idx: 0,
            settings_row_idx: 0,
            available_backups,
            frame_tick: 0,
            heatmap_cursor_days_ago: 0,
            guide_topic: 0,
            guide_scroll: 0,
            guide_max_scroll: std::cell::Cell::new(0),
        };

        rt.reconcile_clock_category();
        rt.persist_active_clock();
        Ok(rt)
    }

    pub fn palette(&self) -> Palette {
        Palette::resolve(ThemeId::from_key(&self.prefs.appearance.theme))
    }

    pub fn set_toast(&mut self, msg: impl Into<String>) {
        self.toast = Some((msg.into(), Instant::now()));
    }

    pub fn active_toast(&self) -> Option<&str> {
        if let Some((ref text, ts)) = self.toast {
            if ts.elapsed() < Duration::from_secs(6) {
                return Some(text.as_str());
            }
        }
        None
    }

    pub fn persist_active_clock(&mut self) {
        let now_utc = Utc::now();
        self.vault.active_clock = if self.clock.status == ClockStatus::Idle {
            None
        } else {
            Some(self.clock.clone())
        };
        if let Err(e) = save_vault(&self.paths, &self.vault) {
            self.set_toast(format!("Could not save data: {}", e));
        }
        write_status_bar_snapshot(&self.paths, &self.vault, &self.clock, &self.prefs, now_utc);
    }

    pub fn on_tick(&mut self) {
        self.frame_tick = self.frame_tick.wrapping_add(1);
        let now_utc = Utc::now();
        if self.clock.sync_wall_clock(now_utc) {
            self.audio.notify_phase_complete(
                self.clock.activity_kind,
                &self.prefs.notifications,
            );
            self.complete_and_record_active_phase(true);
        } else if self.clock.status == ClockStatus::Running && self.frame_tick.is_multiple_of(20) {
            // Keep the Waybar/Polybar countdown fresh (once per second).
            write_status_bar_snapshot(&self.paths, &self.vault, &self.clock, &self.prefs, now_utc);
        }
    }

    pub fn complete_and_record_active_phase(&mut self, completed: bool) {
        let now_utc = Utc::now();
        let prev_kind = self.clock.activity_kind;
        let (maybe_record, full_cycle) =
            self.clock.finalize_session(completed, &self.prefs, now_utc, None);

        let mut newly_unlocked_list = Vec::new();

        if let Some(rec) = maybe_record {
            let committed = commit_session_record(
                &mut self.vault.sessions,
                &mut self.vault.reading_shelf,
                &mut self.vault.planned_blocks,
                rec,
                None,
            );

            if let Some(saved) = committed {
                let ref_date = to_local_date_key(now_utc, None);
                let (_, _, newly_unlocked) = evaluate_progression(
                    &mut self.vault.progression,
                    &self.vault.sessions,
                    &self.vault.categories,
                    &self.vault.planned_blocks,
                    &self.prefs,
                    &ref_date,
                    false,
                    now_utc,
                );

                for spec in newly_unlocked {
                    newly_unlocked_list.push((
                        spec.title.to_string(),
                        spec.rarity.label().to_string(),
                        spec.rarity.xp_bonus(),
                    ));
                }

                if saved.completion_state == CompletionState::Completed
                    && (full_cycle || !newly_unlocked_list.is_empty())
                    && self.prefs.appearance.show_gamification
                {
                    let headline = if full_cycle {
                        format!(
                            "Full {} Cadence Completed!",
                            saved.cadence.short_name()
                        )
                    } else {
                        "Milestone Unlocked!".to_string()
                    };
                    let subtitle = format!(
                        "Recorded {} of {} · Level {} ({} XP)",
                        format_duration_hms(saved.elapsed_seconds),
                        prev_kind.short_label(),
                        self.vault.progression.level,
                        self.vault.progression.total_xp
                    );
                    self.modal = Some(ModalDialog::CelebrationBanner {
                        headline,
                        subtitle,
                        unlocked_titles: newly_unlocked_list,
                        completed_full_cycle: full_cycle,
                        opened_at: Instant::now(),
                    });
                } else {
                    self.set_toast(format!(
                        "Recorded {} {} session ({}).",
                        saved.completion_state.label().to_lowercase(),
                        prev_kind.short_label(),
                        format_duration_hms(saved.elapsed_seconds)
                    ));
                }
            }
        } else {
            self.set_toast(format!("Reset {} timer.", prev_kind.short_label()));
        }

        self.persist_active_clock();
    }

    pub fn current_goals(&self) -> GoalsEvaluation {
        let today_key = to_local_date_key(Utc::now(), None);
        evaluate_goals(
            &self.vault.sessions,
            &self.vault.categories,
            &self.prefs,
            &today_key,
        )
    }

    pub fn current_progression_snapshot(&self) -> (LevelSnapshot, Vec<AchievementStatusItem>) {
        let now_utc = Utc::now();
        let ref_date = to_local_date_key(now_utc, None);
        let mut ledger_clone = self.vault.progression.clone();
        let (lvl, items, _) = evaluate_progression(
            &mut ledger_clone,
            &self.vault.sessions,
            &self.vault.categories,
            &self.vault.planned_blocks,
            &self.prefs,
            &ref_date,
            true,
            now_utc,
        );
        (lvl, items)
    }

    /// Resolves the active non-archived category for a given `ActivityKind` (`Focus` vs `Reading`).
    /// Guarantees that deleted, archived, or incompatible-scope categories are never returned.
    pub fn active_category_for(&self, activity: ActivityKind) -> Option<&crate::domain::CategoryRecord> {
        let target_act = match activity {
            ActivityKind::Reading => ActivityKind::Reading,
            _ => ActivityKind::Focus,
        };

        // 1. Check current clock.category_id if it is active and supports `target_act`
        if let Some(cat) = self
            .vault
            .categories
            .iter()
            .find(|c| c.id == self.clock.category_id && !c.is_archived && c.scope.supports_activity(target_act))
        {
            return Some(cat);
        }

        // 2. Check the user's saved default category for `target_act`
        let pref_id = if target_act == ActivityKind::Reading {
            &self.prefs.reading.default_category_id
        } else {
            &self.prefs.focus.default_category_id
        };
        if let Some(cat) = self
            .vault
            .categories
            .iter()
            .find(|c| &c.id == pref_id && !c.is_archived && c.scope.supports_activity(target_act))
        {
            return Some(cat);
        }

        // 3. Fallback to the first non-archived category supporting `target_act`
        self.vault
            .categories
            .iter()
            .find(|c| !c.is_archived && c.scope.supports_activity(target_act))
            .or_else(|| self.vault.categories.iter().find(|c| !c.is_archived))
    }

    /// Reconciles `clock.category_id` and default preferences against `vault.categories`
    /// so deleted, archived, or scope-changed categories never remain stuck on the timer.
    pub fn reconcile_clock_category(&mut self) {
        let valid_focus_id = self
            .vault
            .categories
            .iter()
            .find(|c| {
                c.id == self.prefs.focus.default_category_id
                    && !c.is_archived
                    && c.scope.supports_activity(ActivityKind::Focus)
            })
            .or_else(|| {
                self.vault
                    .categories
                    .iter()
                    .find(|c| !c.is_archived && c.scope.supports_activity(ActivityKind::Focus))
            })
            .map(|c| c.id.clone());

        if let Some(fid) = valid_focus_id {
            if self.prefs.focus.default_category_id != fid {
                self.prefs.focus.default_category_id = fid;
                let _ = save_preferences(&self.paths, &self.prefs);
            }
        }

        let valid_reading_id = self
            .vault
            .categories
            .iter()
            .find(|c| {
                c.id == self.prefs.reading.default_category_id
                    && !c.is_archived
                    && c.scope.supports_activity(ActivityKind::Reading)
            })
            .or_else(|| {
                self.vault
                    .categories
                    .iter()
                    .find(|c| !c.is_archived && c.scope.supports_activity(ActivityKind::Reading))
            })
            .map(|c| c.id.clone());

        if let Some(rid) = valid_reading_id {
            if self.prefs.reading.default_category_id != rid {
                self.prefs.reading.default_category_id = rid;
                let _ = save_preferences(&self.paths, &self.prefs);
            }
        }

        let current_act = match self.clock.activity_kind {
            ActivityKind::Reading => ActivityKind::Reading,
            _ => ActivityKind::Focus,
        };
        let clock_cat_valid = self.vault.categories.iter().any(|c| {
            c.id == self.clock.category_id && !c.is_archived && c.scope.supports_activity(current_act)
        });

        if !clock_cat_valid {
            if let Some( fallback_id ) = self.active_category_for(current_act).map(|c| c.id.clone()) {
                self.clock.category_id = fallback_id;
            }
        }
    }

    /// Switches route and, when the timer is Idle, automatically aligns `clock.activity_kind`
    /// and `clock.category_id` with Focus Arena (`Focus`) or Reading Deck (`Reading`).
    pub fn switch_route(&mut self, next_route: PrimaryRoute) {
        self.route = next_route;
        if self.clock.status == ClockStatus::Idle {
            match next_route {
                PrimaryRoute::FocusStudio => {
                    if self.clock.activity_kind == ActivityKind::Reading {
                        self.clock.switch_activity(ActivityKind::Focus, &self.prefs);
                    }
                    self.reconcile_clock_category();
                }
                PrimaryRoute::ReadingSanctuary => {
                    if self.clock.activity_kind != ActivityKind::Reading {
                        self.clock.switch_activity(ActivityKind::Reading, &self.prefs);
                    }
                    self.reconcile_clock_category();
                }
                _ => {
                    self.reconcile_clock_category();
                }
            }
        } else {
            self.reconcile_clock_category();
        }
    }

    pub fn cycle_active_category(&mut self) {
        let target_activity = if self.route == PrimaryRoute::ReadingSanctuary {
            ActivityKind::Reading
        } else if self.route == PrimaryRoute::FocusStudio {
            ActivityKind::Focus
        } else {
            self.clock.activity_kind
        };
        self.cycle_category_for_activity(target_activity);
    }

    pub fn cycle_category_for_activity(&mut self, target_activity: ActivityKind) {
        if self.clock.status == ClockStatus::Idle && self.clock.activity_kind != target_activity {
            self.clock.switch_activity(target_activity, &self.prefs);
        }

        let matching: Vec<(String, String)> = self
            .vault
            .categories
            .iter()
            .filter(|c| !c.is_archived && c.scope.supports_activity(target_activity))
            .map(|c| (c.id.clone(), c.name.clone()))
            .collect();

        if matching.is_empty() {
            return;
        }

        let current_id = self
            .active_category_for(target_activity)
            .map(|c| c.id.as_str())
            .unwrap_or(self.clock.category_id.as_str());

        let cur_pos = matching
            .iter()
            .position(|(id, _)| id == current_id)
            .unwrap_or(0);
        let (next_id, next_name) = matching[(cur_pos + 1) % matching.len()].clone();

        self.clock.category_id = next_id.clone();
        if target_activity == ActivityKind::Reading {
            self.prefs.reading.default_category_id = next_id;
        } else {
            self.prefs.focus.default_category_id = next_id;
        }
        let _ = save_preferences(&self.paths, &self.prefs);
        self.persist_active_clock();
        self.set_toast(format!(
            "Active {} class switched to '{}'.",
            target_activity.short_label(),
            next_name
        ));
    }

    pub fn cycle_theme(&mut self) {
        let current = ThemeId::from_key(&self.prefs.appearance.theme);
        let next = current.next();
        self.prefs.appearance.theme = next.key().to_string();
        let _ = save_preferences(&self.paths, &self.prefs);
        self.set_toast(format!("Theme switched to {}.", next.label()));
    }

    pub fn filtered_sessions(&self) -> Vec<&crate::domain::SessionRecord> {
        let q = self.ledger_search_query.trim().to_lowercase();
        self.vault
            .sessions
            .iter()
            .filter(|s| {
                if let Some(act) = self.ledger_activity_filter {
                    if s.activity_kind != act {
                        return false;
                    }
                }
                if let Some(st) = self.ledger_status_filter {
                    if s.completion_state != st {
                        return false;
                    }
                }
                if let Some(ref cid) = self.ledger_category_filter {
                    if &s.category_id != cid {
                        return false;
                    }
                }
                if !q.is_empty() {
                    let r_title = s
                        .reading_meta
                        .as_ref()
                        .map(|m| m.title.as_str())
                        .unwrap_or("");
                    let r_author = s
                        .reading_meta
                        .as_ref()
                        .map(|m| m.author.as_str())
                        .unwrap_or("");
                    let hay = format!(
                        "{} {} {} {} {}",
                        s.primary_date_key, s.objective, s.completion_notes, r_title, r_author
                    )
                    .to_lowercase();
                    if !hay.contains(&q) {
                        return false;
                    }
                }
                true
            })
            .collect()
    }

    pub fn launch_planned_block(&mut self, plan_idx: usize) {
        let pending: Vec<crate::domain::PlannedBlock> = self
            .vault
            .planned_blocks
            .iter()
            .filter(|p| p.status == PlanStatus::Pending)
            .cloned()
            .collect();
        let Some(plan) = pending.get(plan_idx) else {
            return;
        };
        if self.clock.status != ClockStatus::Idle {
            self.set_toast("A session is already active. Finish or reset it before launching another.");
            return;
        }

        self.clock.switch_activity(plan.activity_kind, &self.prefs);
        self.clock.category_id = plan.category_id.clone();
        let dur_secs = (plan.target_minutes as u64) * 60;
        self.clock.configured_seconds = dur_secs;
        self.clock.target_seconds = dur_secs;
        self.clock.objective = plan.title.clone();
        self.clock.planned_block_id = Some(plan.id.clone());
        self.clock.end_page = None;

        if plan.activity_kind == ActivityKind::Reading {
            if let Some(ref mat_id) = plan.reading_material_id {
                if let Some(mat) = self.vault.reading_shelf.iter().find(|m| &m.id == mat_id) {
                    self.clock.reading_material_id = Some(mat.id.clone());
                    self.clock.reading_title = mat.title.clone();
                    self.clock.reading_author = mat.author.clone();
                    self.clock.reading_kind = mat.kind;
                    self.clock.start_page = Some(mat.current_page);
                }
            }
            self.route = PrimaryRoute::ReadingSanctuary;
        } else {
            self.route = PrimaryRoute::FocusStudio;
        }

        self.clock.start_or_resume(Utc::now());
        self.persist_active_clock();
        self.set_toast(format!("Started planned session: {}", plan.title));
    }

    pub fn launch_reading_material(&mut self, shelf_idx: usize) {
        let Some(mat) = self.vault.reading_shelf.get(shelf_idx).cloned() else {
            return;
        };
        if self.clock.status != ClockStatus::Idle
            && !(self.clock.activity_kind == ActivityKind::Reading
                && self.clock.reading_material_id.as_deref() == Some(mat.id.as_str()))
        {
            self.set_toast("A session is already active. Finish or reset it before starting another.");
            return;
        }
        if self.clock.status != ClockStatus::Idle {
            // Same material: just resume/continue rather than rebuilding the session.
            self.clock.start_or_resume(Utc::now());
            self.persist_active_clock();
            return;
        }
        if matches!(mat.status, crate::domain::ReadingStatus::Have | crate::domain::ReadingStatus::Abandoned) {
            if let Some(m) = self.vault.reading_shelf.get_mut(shelf_idx) {
                m.status = crate::domain::ReadingStatus::Reading;
            }
        }
        self.clock
            .switch_activity(ActivityKind::Reading, &self.prefs);
        self.clock.category_id = mat.category_id.clone();
        self.clock.reading_material_id = Some(mat.id.clone());
        self.clock.reading_title = mat.title.clone();
        self.clock.reading_author = mat.author.clone();
        self.clock.reading_kind = mat.kind;
        self.clock.start_page = Some(mat.current_page);
        self.clock.end_page = None;
        self.clock.objective = format!("Read {}", mat.title);
        self.clock.start_or_resume(Utc::now());
        self.persist_active_clock();
        self.set_toast(format!(
            "Started Reading Sanctuary session for '{}' (from p. {}).",
            mat.title, mat.current_page
        ));
    }

    pub fn open_category_editor(&mut self, editing_id: Option<String>) {
        if let Some(ref cid) = editing_id {
            if let Some(cat) = self.vault.categories.iter().find(|c| &c.id == cid) {
                let color_idx = APPROVED_CATEGORY_COLORS
                    .iter()
                    .position(|(hex, _)| hex.eq_ignore_ascii_case(&cat.color_hex))
                    .unwrap_or(0);
                let icon_idx = CATEGORY_ICON_GLYPHS
                    .iter()
                    .position(|(g, _)| *g == cat.icon_glyph)
                    .unwrap_or(0);
                self.modal = Some(ModalDialog::CategoryEditor {
                    editing_id: Some(cat.id.clone()),
                    field_index: 0,
                    name_buf: cat.name.clone(),
                    color_idx,
                    icon_idx,
                    scope: cat.scope,
                    daily_goal_minutes: cat.daily_goal_minutes,
                    weekly_goal_minutes: cat.weekly_goal_minutes,
                    description_buf: cat.description.clone(),
                });
                return;
            }
        }
        self.modal = Some(ModalDialog::CategoryEditor {
            editing_id: None,
            field_index: 0,
            name_buf: String::new(),
            color_idx: 0,
            icon_idx: 0,
            scope: CategoryScope::Universal,
            daily_goal_minutes: 45,
            weekly_goal_minutes: 225,
            description_buf: String::new(),
        });
    }

    pub fn submit_category_editor(&mut self) {
        let Some(ModalDialog::CategoryEditor {
            editing_id,
            name_buf,
            color_idx,
            icon_idx,
            scope,
            daily_goal_minutes,
            weekly_goal_minutes,
            description_buf,
            ..
        }) = self.modal.clone()
        else {
            return;
        };

        let color_hex = APPROVED_CATEGORY_COLORS
            .get(color_idx % APPROVED_CATEGORY_COLORS.len())
            .map(|(h, _)| (*h).to_string())
            .unwrap_or_else(|| "#26A69A".to_string());
        let icon_glyph = CATEGORY_ICON_GLYPHS
            .get(icon_idx % CATEGORY_ICON_GLYPHS.len())
            .map(|(g, _)| (*g).to_string())
            .unwrap_or_else(|| "◈".to_string());

        let draft = CategoryDraft {
            name: name_buf,
            color_hex,
            icon_glyph,
            scope,
            description: description_buf,
            daily_goal_minutes,
            weekly_goal_minutes,
        };

        let now_utc = Utc::now();
        let res = if let Some(cid) = editing_id {
            update_category(&mut self.vault.categories, &cid, draft)
        } else {
            create_category(&mut self.vault.categories, draft, now_utc)
        };

        match res {
            Ok(saved) => {
                self.reconcile_clock_category();
                self.vault.recompute_progression(&self.prefs, false, now_utc);
                let _ = save_vault(&self.paths, &self.vault);
                self.persist_active_clock();
                self.modal = None;
                self.set_toast(format!("Saved category '{}' [{}].", saved.name, saved.scope.label()));
            }
            Err(err) => {
                self.set_toast(err.to_string());
            }
        }
    }

    pub fn toggle_selected_category_archive(&mut self) {
        let visible: Vec<String> = self
            .vault
            .categories
            .iter()
            .filter(|c| self.show_archived_categories || !c.is_archived)
            .map(|c| c.id.clone())
            .collect();
        let Some(cid) = visible.get(self.category_cursor).cloned() else {
            return;
        };
        let currently_archived = self
            .vault
            .categories
            .iter()
            .find(|c| c.id == cid)
            .map(|c| c.is_archived)
            .unwrap_or(false);
        let active_cat = if self.clock.status != ClockStatus::Idle {
            Some(self.clock.category_id.as_str())
        } else {
            None
        };

        match set_category_archived(
            &mut self.vault.categories,
            &cid,
            !currently_archived,
            active_cat,
        ) {
            Ok(()) => {
                self.reconcile_clock_category();
                let _ = save_vault(&self.paths, &self.vault);
                self.persist_active_clock();
                self.set_toast(if currently_archived {
                    "Category restored from archive."
                } else {
                    "Category archived (historical records preserved)."
                });
            }
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn open_category_delete_dialog(&mut self) {
        let visible: Vec<crate::domain::CategoryRecord> = self
            .vault
            .categories
            .iter()
            .filter(|c| self.show_archived_categories || !c.is_archived)
            .cloned()
            .collect();
        let Some(cat) = visible.get(self.category_cursor) else {
            return;
        };
        let linked = self
            .vault
            .sessions
            .iter()
            .filter(|s| s.category_id == cat.id)
            .count();

        self.modal = Some(ModalDialog::CategoryDeleteConfirm {
            category_id: cat.id.clone(),
            category_name: cat.name.clone(),
            linked_sessions_count: linked,
            use_archive_instead: false,
            replacement_category_idx: 0,
        });
    }

    pub fn confirm_category_delete(&mut self) {
        let Some(ModalDialog::CategoryDeleteConfirm {
            category_id,
            category_name,
            use_archive_instead,
            replacement_category_idx,
            ..
        }) = self.modal.clone()
        else {
            return;
        };

        let candidates: Vec<String> = self
            .vault
            .categories
            .iter()
            .filter(|c| c.id != category_id && !c.is_archived)
            .map(|c| c.id.clone())
            .collect();

        let strategy = if use_archive_instead {
            CategoryDeleteStrategy::ArchiveInstead
        } else {
            let Some(rep_id) = candidates.get(replacement_category_idx % candidates.len().max(1)) else {
                self.set_toast("No replacement category available.");
                return;
            };
            CategoryDeleteStrategy::ReassignTo(rep_id.clone())
        };

        let active_cat = if self.clock.status != ClockStatus::Idle {
            Some(self.clock.category_id.as_str())
        } else {
            None
        };

        match delete_category_safely(
            &mut self.vault.categories,
            &mut self.vault.sessions,
            &mut self.vault.reading_shelf,
            &mut self.vault.planned_blocks,
            &mut self.prefs,
            &category_id,
            active_cat,
            strategy,
        ) {
            Ok(reassigned) => {
                self.reconcile_clock_category();
                let _ = save_vault(&self.paths, &self.vault);
                let _ = save_preferences(&self.paths, &self.prefs);
                self.persist_active_clock();
                self.modal = None;
                self.category_cursor = self.category_cursor.saturating_sub(1);
                if use_archive_instead {
                    self.set_toast(format!("Archived category '{}'.", category_name));
                } else {
                    self.set_toast(format!(
                        "Deleted '{}' and reassigned {} historical session(s).",
                        category_name, reassigned
                    ));
                }
            }
            Err(err) => self.set_toast(err.to_string()),
        }
    }

    pub fn submit_reading_material_editor(&mut self) {
        let Some(ModalDialog::ReadingMaterialEditor {
            editing_id,
            title_buf,
            author_buf,
            kind,
            category_id,
            total_pages_buf,
            current_page_buf,
            notes_buf,
            ..
        }) = self.modal.clone()
        else {
            return;
        };

        let total_pages = total_pages_buf.trim().parse::<u32>().unwrap_or(250);
        let current_page = current_page_buf.trim().parse::<u32>().unwrap_or(0);
        let draft = ReadingMaterialDraft {
            title: title_buf,
            author: author_buf,
            kind,
            category_id,
            total_pages,
            current_page,
            notes: notes_buf,
        };

        let now_utc = Utc::now();
        let res = if let Some(mid) = editing_id {
            update_reading_material(&mut self.vault.reading_shelf, &mid, draft)
        } else {
            add_reading_material(&mut self.vault.reading_shelf, draft, now_utc)
        };

        match res {
            Ok(item) => {
                let _ = save_vault(&self.paths, &self.vault);
                self.modal = None;
                self.set_toast(format!("Saved '{}' to Reading Sanctuary.", item.title));
            }
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn submit_planned_block_editor(&mut self) {
        let Some(ModalDialog::PlannedBlockEditor {
            title_buf,
            activity_kind,
            category_id,
            duration_minutes,
            priority,
            ..
        }) = self.modal.clone()
        else {
            return;
        };

        let now_utc = Utc::now();
        let today_key = to_local_date_key(now_utc, None);
        match add_planned_block(
            &mut self.vault.planned_blocks,
            &title_buf,
            activity_kind,
            &category_id,
            duration_minutes,
            &today_key,
            priority,
            None,
            now_utc,
        ) {
            Ok(plan) => {
                let _ = save_vault(&self.paths, &self.vault);
                self.modal = None;
                self.set_toast(format!("Scheduled planned session '{}'.", plan.title));
            }
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn submit_session_editor(&mut self) {
        let Some(ModalDialog::SessionEditor {
            session_id,
            objective_buf,
            notes_buf,
            elapsed_mins_buf,
            category_idx,
            pages_read_buf,
            completion_state,
            ..
        }) = self.modal.clone()
        else {
            return;
        };

        let cat_id = self
            .vault
            .categories
            .get(category_idx % self.vault.categories.len().max(1))
            .map(|c| c.id.clone())
            .unwrap_or_else(|| "cat_software_eng".to_string());
        let mins = elapsed_mins_buf.trim().parse::<u64>().unwrap_or(25).clamp(1, 240);
        let pages = pages_read_buf.trim().parse::<u32>().ok();

        let patch = SessionEditPatch {
            category_id: cat_id,
            objective: objective_buf,
            completion_notes: notes_buf,
            elapsed_seconds: mins * 60,
            completion_state,
            reading_title: None,
            reading_author: None,
            pages_read: pages,
        };

        match edit_session_record(
            &mut self.vault.sessions,
            &mut self.vault.reading_shelf,
            &session_id,
            patch,
            None,
        ) {
            Ok(_) => {
                self.vault.recompute_progression(&self.prefs, true, Utc::now());
                let _ = save_vault(&self.paths, &self.vault);
                self.modal = None;
                self.set_toast("Updated session record and recalculated statistics.");
            }
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn confirm_session_delete(&mut self) {
        let Some(ModalDialog::SessionDeleteConfirm { session_id, .. }) = self.modal.clone() else {
            return;
        };
        match delete_session_record(
            &mut self.vault.sessions,
            &mut self.vault.reading_shelf,
            &mut self.vault.planned_blocks,
            &session_id,
        ) {
            Ok(_) => {
                self.vault.recompute_progression(&self.prefs, true, Utc::now());
                let _ = save_vault(&self.paths, &self.vault);
                self.modal = None;
                self.ledger_cursor = self.ledger_cursor.saturating_sub(1);
                self.set_toast("Deleted session and recalculated statistics.");
            }
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    /// Opens the "delete this book?" dialog for the selected shelf item.
    pub fn ask_delete_reading_material(&mut self, shelf_idx: usize) {
        let Some(m) = self.vault.reading_shelf.get(shelf_idx) else {
            return;
        };
        if self.clock.status != ClockStatus::Idle
            && self.clock.activity_kind == ActivityKind::Reading
            && self.clock.reading_material_id.as_deref() == Some(m.id.as_str())
        {
            self.set_toast("You are reading this book right now. Finish or reset the session first.");
            return;
        }
        let sessions_count = self
            .vault
            .sessions
            .iter()
            .filter(|s| s.reading_meta.as_ref().and_then(|r| r.material_id.as_deref()) == Some(m.id.as_str()))
            .count();
        self.modal = Some(ModalDialog::ReadingDeleteConfirm { material_id: m.id.clone(), title: m.title.clone(), sessions_count });
    }

    /// Removes the book from the shelf. Past sessions stay in your history and statistics.
    pub fn confirm_reading_delete(&mut self) {
        let Some(ModalDialog::ReadingDeleteConfirm { material_id, title, .. }) = self.modal.clone() else {
            return;
        };
        self.vault.reading_shelf.retain(|m| m.id != material_id);
        let _ = save_vault(&self.paths, &self.vault);
        self.modal = None;
        self.reading_shelf_cursor = self.reading_shelf_cursor.min(self.vault.reading_shelf.len().saturating_sub(1));
        self.set_toast(format!("Removed '{}' from your shelf. Past sessions are kept.", title));
    }

    /// Cycles a book through Have → Reading → Read → Abandoned.
    pub fn cycle_reading_status(&mut self, shelf_idx: usize) {
        let Some(m) = self.vault.reading_shelf.get_mut(shelf_idx) else {
            return;
        };
        m.status = m.status.next();
        if m.status == crate::domain::ReadingStatus::Read && m.total_pages > 0 {
            m.current_page = m.total_pages;
        }
        let msg = format!("'{}' is now: {}", m.title, m.status.label());
        let _ = save_vault(&self.paths, &self.vault);
        self.set_toast(msg);
    }

    pub fn trigger_csv_export(&mut self) {
        match export_sessions_csv_file(&self.paths, &self.vault, None, Utc::now()) {
            Ok(path) => self.set_toast(format!("Exported CSV to {}", path.display())),
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn trigger_json_export(&mut self) {
        match export_structured_json_file(&self.paths, &self.vault, None, Utc::now()) {
            Ok(path) => self.set_toast(format!("Exported JSON to {}", path.display())),
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn trigger_manual_backup(&mut self) {
        match create_verified_backup(&self.paths, &self.vault, "manual", None, Utc::now()) {
            Ok(path) => {
                self.available_backups = list_available_backups(&self.paths);
                self.set_toast(format!("Created verified backup: {}", path.display()));
            }
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn open_latest_backup_restore_modal(&mut self) {
        self.available_backups = list_available_backups(&self.paths);
        let Some(latest_path) = self.available_backups.first().cloned() else {
            self.set_toast("No backup files found in backups/ folder yet. Press [B] to create one.");
            return;
        };
        let Ok(raw_json) = fs::read_to_string(&latest_path) else {
            self.set_toast("Could not read backup file from disk.");
            return;
        };
        match inspect_and_validate_backup(&raw_json, &self.vault, &self.prefs, Utc::now()) {
            Ok((_, preview)) => {
                self.modal = Some(ModalDialog::BackupRestoreConfirm {
                    backup_path: latest_path,
                    raw_json,
                    preview,
                    strategy: RestoreStrategy::MergeKeepExisting,
                });
            }
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn confirm_backup_restore(&mut self) {
        let Some(ModalDialog::BackupRestoreConfirm {
            raw_json,
            strategy,
            ..
        }) = self.modal.clone()
        else {
            return;
        };

        match apply_backup_restore(
            &self.paths,
            &mut self.vault,
            &raw_json,
            strategy,
            &self.prefs,
            Utc::now(),
        ) {
            Ok(preview) => {
                self.available_backups = list_available_backups(&self.paths);
                self.modal = None;
                self.set_toast(format!(
                    "Backup restored ({}) — {} session(s) processed.",
                    strategy.label(),
                    preview.incoming_sessions
                ));
            }
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn confirm_destructive_reset(&mut self) {
        let Some(ModalDialog::DestructiveResetConfirm {
            operation,
            typed_phrase,
        }) = self.modal.clone()
        else {
            return;
        };

        match execute_guarded_reset(
            &self.paths,
            &mut self.vault,
            &mut self.prefs,
            &mut self.clock,
            operation,
            &typed_phrase,
            Utc::now(),
        ) {
            Ok(backup_opt) => {
                self.available_backups = list_available_backups(&self.paths);
                self.modal = None;
                let suffix = backup_opt
                    .map(|p| format!(" (Pre-reset backup saved to {})", p.display()))
                    .unwrap_or_default();
                self.set_toast(format!("Completed {}{}", operation.title(), suffix));
            }
            Err(e) => self.set_toast(e.to_string()),
        }
    }

    pub fn clean_shutdown(&mut self) {
        self.persist_active_clock();
        remove_status_bar_snapshot(&self.paths);
    }
}

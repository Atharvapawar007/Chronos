//! Chronos Goals, Planned Session Queue & Weekly Reflection Engine (`planner.rs`).

use crate::domain::models::{
    generate_entity_id, monday_of_week_key, shift_date_key, ActivityKind, CategoryScope,
    CompletionState, Preferences, SessionRecord,
};
use crate::domain::taxonomy::CategoryRecord;
use crate::error::{ChronosError, ChronosResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanPriority {
    High,
    Standard,
    Low,
}

impl PlanPriority {
    pub fn label(self) -> &'static str {
        match self {
            PlanPriority::High => "High Priority",
            PlanPriority::Standard => "Standard",
            PlanPriority::Low => "Flexible",
        }
    }

    pub fn next(self) -> Self {
        match self {
            PlanPriority::High => PlanPriority::Standard,
            PlanPriority::Standard => PlanPriority::Low,
            PlanPriority::Low => PlanPriority::High,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Pending,
    Completed,
    Skipped,
}

impl PlanStatus {
    pub fn label(self) -> &'static str {
        match self {
            PlanStatus::Pending => "Pending",
            PlanStatus::Completed => "Completed",
            PlanStatus::Skipped => "Skipped",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedBlock {
    pub id: String,
    pub title: String,
    pub activity_kind: ActivityKind,
    pub category_id: String,
    pub target_minutes: u32,
    pub scheduled_date_key: String,
    pub priority: PlanPriority,
    pub status: PlanStatus,
    pub reading_material_id: Option<String>,
    pub linked_session_id: Option<String>,
    pub created_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DayAttribution {
    pub date_key: String,
    pub focus_seconds: u64,
    pub reading_seconds: u64,
    pub break_seconds: u64,
    pub by_category_seconds: BTreeMap<String, u64>,
    pub completed_sessions: u32,
    pub interrupted_sessions: u32,
    pub pages_read: u32,
}

impl DayAttribution {
    pub fn productive_seconds(&self) -> u64 {
        self.focus_seconds.saturating_add(self.reading_seconds)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalMeter {
    pub enabled: bool,
    pub done_seconds: u64,
    pub target_seconds: u64,
    pub percent: u16,
    pub is_met: bool,
}

impl GoalMeter {
    pub fn compute(done_seconds: u64, target_minutes: u32) -> Self {
        let target_seconds = (target_minutes as u64) * 60;
        if target_seconds == 0 {
            return Self {
                enabled: false,
                done_seconds,
                target_seconds: 0,
                percent: 0,
                is_met: false,
            };
        }
        let raw_pct = ((done_seconds as u128 * 100) / (target_seconds as u128)).min(999) as u16;
        Self {
            enabled: true,
            done_seconds,
            target_seconds,
            percent: raw_pct.min(100),
            is_met: done_seconds >= target_seconds,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryGoalStatus {
    pub category_id: String,
    pub category_name: String,
    pub color_hex: String,
    pub icon_glyph: String,
    pub scope: CategoryScope,
    pub daily: GoalMeter,
    pub weekly: GoalMeter,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GoalsEvaluation {
    pub reference_date_key: String,
    pub week_monday_key: String,
    pub daily_focus: GoalMeter,
    pub daily_reading: GoalMeter,
    pub weekly_focus: GoalMeter,
    pub weekly_reading: GoalMeter,
    pub category_goals: Vec<CategoryGoalStatus>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeeklyReflectionReport {
    pub start_date_key: String,
    pub end_date_key: String,
    pub focus_seconds: u64,
    pub reading_seconds: u64,
    pub total_productive_seconds: u64,
    pub previous_week_seconds: u64,
    pub delta_percent: Option<i32>,
    pub focus_goal_days_met: u32,
    pub reading_goal_days_met: u32,
    pub best_day_key: Option<String>,
    pub best_day_seconds: u64,
    pub top_category_name: Option<String>,
    pub top_category_seconds: u64,
    pub recommendations: Vec<String>,
}

/// Computes exact seconds attributed to a single local `date_key` across all sessions,
/// honoring midnight-split `day_slices`.
pub fn attribute_day_seconds(
    sessions: &[SessionRecord],
    date_key: &str,
    include_interrupted: bool,
) -> DayAttribution {
    let mut focus_seconds = 0u64;
    let mut reading_seconds = 0u64;
    let mut break_seconds = 0u64;
    let mut by_category_seconds = BTreeMap::new();
    let mut completed_sessions = 0u32;
    let mut interrupted_sessions = 0u32;
    let mut pages_read = 0u32;

    for s in sessions {
        if !include_interrupted && s.completion_state == CompletionState::Interrupted {
            continue;
        }

        let slice_secs: u64 = if s.day_slices.is_empty() {
            if s.primary_date_key == date_key {
                s.elapsed_seconds
            } else {
                0
            }
        } else {
            s.day_slices
                .iter()
                .filter(|sl| sl.date_key == date_key)
                .map(|sl| sl.seconds)
                .sum()
        };

        if slice_secs > 0 {
            match s.activity_kind {
                ActivityKind::Focus => {
                    focus_seconds = focus_seconds.saturating_add(slice_secs);
                    *by_category_seconds
                        .entry(s.category_id.clone())
                        .or_insert(0) += slice_secs;
                }
                ActivityKind::Reading => {
                    reading_seconds = reading_seconds.saturating_add(slice_secs);
                    *by_category_seconds
                        .entry(s.category_id.clone())
                        .or_insert(0) += slice_secs;
                }
                ActivityKind::ShortBreak | ActivityKind::LongBreak => {
                    break_seconds = break_seconds.saturating_add(slice_secs);
                }
            }
        }

        if s.primary_date_key == date_key && s.activity_kind.is_productive() {
            match s.completion_state {
                CompletionState::Completed => completed_sessions += 1,
                CompletionState::Interrupted => interrupted_sessions += 1,
            }
            if let Some(ref meta) = s.reading_meta {
                pages_read = pages_read.saturating_add(meta.pages_read);
            }
        }
    }

    DayAttribution {
        date_key: date_key.to_string(),
        focus_seconds,
        reading_seconds,
        break_seconds,
        by_category_seconds,
        completed_sessions,
        interrupted_sessions,
        pages_read,
    }
}

/// Evaluates Daily, Weekly, and Per-Category Goals for `reference_date_key`.
pub fn evaluate_goals(
    sessions: &[SessionRecord],
    categories: &[CategoryRecord],
    prefs: &Preferences,
    reference_date_key: &str,
) -> GoalsEvaluation {
    let include_interrupted = prefs.focus.count_interrupted_in_totals;
    let today = attribute_day_seconds(sessions, reference_date_key, include_interrupted);
    let week_monday = monday_of_week_key(reference_date_key);

    let mut week_focus_secs = 0u64;
    let mut week_reading_secs = 0u64;
    let mut week_cat_secs: BTreeMap<String, u64> = BTreeMap::new();

    for offset in 0..7 {
        let d_key = shift_date_key(&week_monday, offset);
        let day = attribute_day_seconds(sessions, &d_key, include_interrupted);
        week_focus_secs = week_focus_secs.saturating_add(day.focus_seconds);
        week_reading_secs = week_reading_secs.saturating_add(day.reading_seconds);
        for (cid, secs) in day.by_category_seconds {
            *week_cat_secs.entry(cid).or_insert(0) += secs;
        }
    }

    let category_goals = categories
        .iter()
        .filter(|c| !c.is_archived)
        .map(|cat| {
            let d_done = *today.by_category_seconds.get(&cat.id).unwrap_or(&0);
            let w_done = *week_cat_secs.get(&cat.id).unwrap_or(&0);
            CategoryGoalStatus {
                category_id: cat.id.clone(),
                category_name: cat.name.clone(),
                color_hex: cat.color_hex.clone(),
                icon_glyph: crate::domain::taxonomy::sanitize_icon_glyph(&cat.icon_glyph),
                scope: cat.scope,
                daily: GoalMeter::compute(d_done, cat.daily_goal_minutes),
                weekly: GoalMeter::compute(w_done, cat.weekly_goal_minutes),
            }
        })
        .collect();

    GoalsEvaluation {
        reference_date_key: reference_date_key.to_string(),
        week_monday_key: week_monday,
        daily_focus: GoalMeter::compute(today.focus_seconds, prefs.goals.daily_focus_minutes),
        daily_reading: GoalMeter::compute(
            today.reading_seconds,
            prefs.goals.daily_reading_minutes,
        ),
        weekly_focus: GoalMeter::compute(week_focus_secs, prefs.goals.weekly_focus_minutes),
        weekly_reading: GoalMeter::compute(week_reading_secs, prefs.goals.weekly_reading_minutes),
        category_goals,
    }
}

pub fn add_planned_block(
    plans: &mut Vec<PlannedBlock>,
    title: &str,
    activity_kind: ActivityKind,
    category_id: &str,
    target_minutes: u32,
    scheduled_date_key: &str,
    priority: PlanPriority,
    reading_material_id: Option<String>,
    now_utc: DateTime<Utc>,
) -> ChronosResult<PlannedBlock> {
    let clean_title = title.trim();
    if clean_title.is_empty() {
        return Err(ChronosError::Validation(
            "Planned session title cannot be empty.".to_string(),
        ));
    }

    let block = PlannedBlock {
        id: generate_entity_id("plan"),
        title: clean_title.to_string(),
        activity_kind,
        category_id: category_id.to_string(),
        target_minutes: target_minutes.clamp(5, 240),
        scheduled_date_key: scheduled_date_key.to_string(),
        priority,
        status: PlanStatus::Pending,
        reading_material_id,
        linked_session_id: None,
        created_at_utc: now_utc,
    };

    plans.insert(0, block.clone());
    Ok(block)
}

/// Synthesizes a 7-day Weekly Reflection & Review comparing `[reference_date_key - 6d .. reference_date_key]`
/// against the preceding 7 days. Never fabricates percentage deltas when the prior baseline is zero.
pub fn synthesize_weekly_reflection(
    sessions: &[SessionRecord],
    categories: &[CategoryRecord],
    prefs: &Preferences,
    reference_date_key: &str,
) -> WeeklyReflectionReport {
    let include_interrupted = prefs.focus.count_interrupted_in_totals;
    let daily_focus_target = (prefs.goals.daily_focus_minutes as u64) * 60;
    let daily_reading_target = (prefs.goals.daily_reading_minutes as u64) * 60;

    let mut focus_seconds = 0u64;
    let mut reading_seconds = 0u64;
    let mut prev_seconds = 0u64;
    let mut focus_goal_days_met = 0u32;
    let mut reading_goal_days_met = 0u32;
    let mut best_day_key: Option<String> = None;
    let mut best_day_seconds = 0u64;
    let mut cat_totals: BTreeMap<String, u64> = BTreeMap::new();

    for i in 0..7 {
        let cur_key = shift_date_key(reference_date_key, -(i as i64));
        let prev_key = shift_date_key(reference_date_key, -((i + 7) as i64));

        let cur = attribute_day_seconds(sessions, &cur_key, include_interrupted);
        let prev = attribute_day_seconds(sessions, &prev_key, include_interrupted);

        focus_seconds = focus_seconds.saturating_add(cur.focus_seconds);
        reading_seconds = reading_seconds.saturating_add(cur.reading_seconds);
        prev_seconds = prev_seconds.saturating_add(prev.productive_seconds());

        if daily_focus_target > 0 && cur.focus_seconds >= daily_focus_target {
            focus_goal_days_met += 1;
        }
        if daily_reading_target > 0 && cur.reading_seconds >= daily_reading_target {
            reading_goal_days_met += 1;
        }
        if cur.productive_seconds() > best_day_seconds {
            best_day_seconds = cur.productive_seconds();
            best_day_key = Some(cur_key.clone());
        }
        for (cid, secs) in cur.by_category_seconds {
            *cat_totals.entry(cid).or_insert(0) += secs;
        }
    }

    let total_productive_seconds = focus_seconds.saturating_add(reading_seconds);
    let delta_percent = if prev_seconds > 0 {
        let diff = (total_productive_seconds as f64) - (prev_seconds as f64);
        Some(((diff / (prev_seconds as f64)) * 100.0).round() as i32)
    } else {
        None
    };

    let (top_category_name, top_category_seconds) = cat_totals
        .into_iter()
        .max_by_key(|(_, secs)| *secs)
        .map(|(cid, secs)| {
            let name = categories
                .iter()
                .find(|c| c.id == cid)
                .map(|c| c.name.clone())
                .unwrap_or(cid);
            (Some(name), secs)
        })
        .unwrap_or((None, 0));

    let mut recommendations = Vec::new();
    if total_productive_seconds == 0 {
        recommendations.push(
            "Launch a 25-minute Harmonic Focus session or 20-minute Reading session today to start your weekly cadence."
                .to_string(),
        );
    } else {
        if focus_goal_days_met < 3 && daily_focus_target > 0 {
            recommendations.push(format!(
                "Focus goal met on {}/7 days. Consider recalibrating your daily focus target or scheduling a dedicated morning block.",
                focus_goal_days_met
            ));
        } else if focus_goal_days_met >= 5 {
            recommendations.push(format!(
                "Strong deep-work consistency ({}/7 days met). Your daily focus target matches your sustainable output.",
                focus_goal_days_met
            ));
        }

        if reading_seconds == 0 && daily_reading_target > 0 {
            recommendations.push(
                "No reading sessions logged in the past 7 days. Pair a 25-minute Reading Sanctuary session after your main focus block."
                    .to_string(),
            );
        } else if reading_goal_days_met >= 4 {
            recommendations.push(format!(
                "Reading rhythm is well established ({}/7 days at goal).",
                reading_goal_days_met
            ));
        }
    }

    WeeklyReflectionReport {
        start_date_key: shift_date_key(reference_date_key, -6),
        end_date_key: reference_date_key.to_string(),
        focus_seconds,
        reading_seconds,
        total_productive_seconds,
        previous_week_seconds: prev_seconds,
        delta_percent,
        focus_goal_days_met,
        reading_goal_days_met,
        best_day_key,
        best_day_seconds,
        top_category_name,
        top_category_seconds,
        recommendations,
    }
}

//! Chronos Progression, XP, Level & Achievement Engine (`progression.rs`).
//!
//! Designed around sustainable, healthy productivity:
//! - Caps daily XP-earning credit at 5 hours (`MAX_DAILY_REWARDED_SECS = 18,000s`) per calendar day.
//! - Requires meaningful daily targets (`>= 60m` focus, `>= 20m` reading) for goal achievements.
//! - Evaluates 24 deterministic achievements across 6 families (`Consistency`, `DeepWork`,
//!   `Reading`, `Exploration`, `PersonalImprovement`, `LongTermMilestones`) and 4 rarity tiers.
//! - Idempotent unlock map keyed by achievement ID so historical edits or duplicate imports never duplicate rewards.

use crate::domain::models::{
    parse_date_key, ActivityKind, CompletionState, Preferences, SessionRecord,
    MAX_DAILY_REWARDED_SECS, MIN_FOCUS_GOAL_MINS_FOR_REWARD, MIN_READING_GOAL_MINS_FOR_REWARD,
};
use crate::domain::observatory::compute_streaks;
use crate::domain::planner::{PlanStatus, PlannedBlock};
use crate::domain::taxonomy::CategoryRecord;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AchievementFamily {
    Consistency,
    DeepWork,
    Reading,
    Exploration,
    PersonalImprovement,
    LongTermMilestones,
}

impl AchievementFamily {
    pub const ALL: [AchievementFamily; 6] = [
        AchievementFamily::Consistency,
        AchievementFamily::DeepWork,
        AchievementFamily::Reading,
        AchievementFamily::Exploration,
        AchievementFamily::PersonalImprovement,
        AchievementFamily::LongTermMilestones,
    ];

    pub fn label(self) -> &'static str {
        match self {
            AchievementFamily::Consistency => "Consistency",
            AchievementFamily::DeepWork => "Deep Work",
            AchievementFamily::Reading => "Reading Sanctuary",
            AchievementFamily::Exploration => "Exploration",
            AchievementFamily::PersonalImprovement => "Personal Improvement",
            AchievementFamily::LongTermMilestones => "Long-Term Milestones",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RarityTier {
    Common,
    Uncommon,
    Rare,
    Mythic,
}

impl RarityTier {
    pub fn label(self) -> &'static str {
        match self {
            RarityTier::Common => "Common",
            RarityTier::Uncommon => "Uncommon",
            RarityTier::Rare => "Rare",
            RarityTier::Mythic => "Mythic",
        }
    }

    pub fn glyph(self) -> &'static str {
        match self {
            RarityTier::Common => "◇",
            RarityTier::Uncommon => "◈",
            RarityTier::Rare => "✦",
            RarityTier::Mythic => "❂",
        }
    }

    pub fn xp_bonus(self) -> u64 {
        match self {
            RarityTier::Common => 50,
            RarityTier::Uncommon => 120,
            RarityTier::Rare => 280,
            RarityTier::Mythic => 600,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MetricSelector {
    CompletedSessions,
    BestCombinedStreak,
    ReturnedAfterBreak,
    LongestFocusSessionMins,
    CompletedFocusHours,
    FocusGoalDaysMet,
    CompletedReadingSessions,
    BestReadingStreak,
    CompletedReadingHours,
    TotalPagesRead,
    CustomCategoriesCount,
    DistinctCategoriesUsed,
    DualModalityDays,
    CompletedPlannedBlocks,
    AnnotatedSessions,
    PersonalBestBroken,
    BothGoalsMetDays,
    CompletedCombinedHours,
    CurrentLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AchievementSpec {
    pub id: &'static str,
    pub family: AchievementFamily,
    pub title: &'static str,
    pub description: &'static str,
    pub rarity: RarityTier,
    pub target: u64,
    pub unit: &'static str,
    pub metric: MetricSelector,
}

pub const ACHIEVEMENT_CATALOG: &[AchievementSpec] = &[
    // 1. CONSISTENCY
    AchievementSpec {
        id: "ach_first_step",
        family: AchievementFamily::Consistency,
        title: "First Deliberate Hour",
        description: "Complete your first Focus or Reading session in Chronos.",
        rarity: RarityTier::Common,
        target: 1,
        unit: "session",
        metric: MetricSelector::CompletedSessions,
    },
    AchievementSpec {
        id: "ach_streak_3",
        family: AchievementFamily::Consistency,
        title: "Kindled Rhythm",
        description: "Maintain a 3-day consecutive focus or reading streak.",
        rarity: RarityTier::Common,
        target: 3,
        unit: "days",
        metric: MetricSelector::BestCombinedStreak,
    },
    AchievementSpec {
        id: "ach_streak_7",
        family: AchievementFamily::Consistency,
        title: "Seven-Day Equilibrium",
        description: "Maintain an active streak across a full 7-day week.",
        rarity: RarityTier::Uncommon,
        target: 7,
        unit: "days",
        metric: MetricSelector::BestCombinedStreak,
    },
    AchievementSpec {
        id: "ach_streak_21",
        family: AchievementFamily::Consistency,
        title: "Habit Anchor",
        description: "Reach a 21-day streak of sustainable daily practice.",
        rarity: RarityTier::Rare,
        target: 21,
        unit: "days",
        metric: MetricSelector::BestCombinedStreak,
    },
    AchievementSpec {
        id: "ach_resilient_return",
        family: AchievementFamily::Consistency,
        title: "Resilient Return",
        description: "Return to complete a session after a rest gap of 2 or more days.",
        rarity: RarityTier::Uncommon,
        target: 1,
        unit: "return",
        metric: MetricSelector::ReturnedAfterBreak,
    },
    // 2. DEEP WORK
    AchievementSpec {
        id: "ach_deep_first_block",
        family: AchievementFamily::DeepWork,
        title: "Immersion Threshold",
        description: "Complete a single uninterrupted Focus session of at least 45 minutes.",
        rarity: RarityTier::Common,
        target: 45,
        unit: "minutes",
        metric: MetricSelector::LongestFocusSessionMins,
    },
    AchievementSpec {
        id: "ach_deep_hours_10",
        family: AchievementFamily::DeepWork,
        title: "Ten Hours of Craft",
        description: "Accumulate 10 hours of completed Focus sessions.",
        rarity: RarityTier::Common,
        target: 10,
        unit: "hours",
        metric: MetricSelector::CompletedFocusHours,
    },
    AchievementSpec {
        id: "ach_deep_hours_50",
        family: AchievementFamily::DeepWork,
        title: "Fifty-Hour Foundry",
        description: "Accumulate 50 hours of completed Focus sessions.",
        rarity: RarityTier::Rare,
        target: 50,
        unit: "hours",
        metric: MetricSelector::CompletedFocusHours,
    },
    AchievementSpec {
        id: "ach_deep_goal_days_5",
        family: AchievementFamily::DeepWork,
        title: "Intentional Fulfilment",
        description: "Meet a valid Daily Focus Goal (>= 60 min) on 5 separate days.",
        rarity: RarityTier::Uncommon,
        target: 5,
        unit: "days",
        metric: MetricSelector::FocusGoalDaysMet,
    },
    // 3. READING
    AchievementSpec {
        id: "ach_reading_initiate",
        family: AchievementFamily::Reading,
        title: "Sanctuary Opened",
        description: "Complete your first dedicated Reading Sanctuary session.",
        rarity: RarityTier::Common,
        target: 1,
        unit: "session",
        metric: MetricSelector::CompletedReadingSessions,
    },
    AchievementSpec {
        id: "ach_reading_streak_5",
        family: AchievementFamily::Reading,
        title: "Daily Scholar",
        description: "Achieve a 5-day consecutive Reading streak.",
        rarity: RarityTier::Uncommon,
        target: 5,
        unit: "days",
        metric: MetricSelector::BestReadingStreak,
    },
    AchievementSpec {
        id: "ach_reading_hours_15",
        family: AchievementFamily::Reading,
        title: "Fifteen Hours in Print",
        description: "Accumulate 15 hours of completed Reading sessions.",
        rarity: RarityTier::Rare,
        target: 15,
        unit: "hours",
        metric: MetricSelector::CompletedReadingHours,
    },
    AchievementSpec {
        id: "ach_reading_pages_250",
        family: AchievementFamily::Reading,
        title: "Monograph Completed",
        description: "Log 250 pages read across your Reading Sanctuary sessions.",
        rarity: RarityTier::Uncommon,
        target: 250,
        unit: "pages",
        metric: MetricSelector::TotalPagesRead,
    },
    // 4. EXPLORATION
    AchievementSpec {
        id: "ach_custom_category",
        family: AchievementFamily::Exploration,
        title: "Taxonomy Architect",
        description: "Create a custom category tailored to your personal workflow.",
        rarity: RarityTier::Common,
        target: 1,
        unit: "category",
        metric: MetricSelector::CustomCategoriesCount,
    },
    AchievementSpec {
        id: "ach_polymath_categories",
        family: AchievementFamily::Exploration,
        title: "Multi-Disciplinary Balance",
        description: "Complete sessions across at least 3 distinct categories.",
        rarity: RarityTier::Uncommon,
        target: 3,
        unit: "categories",
        metric: MetricSelector::DistinctCategoriesUsed,
    },
    AchievementSpec {
        id: "ach_dual_modality",
        family: AchievementFamily::Exploration,
        title: "Dual Modality Day",
        description: "Complete both a Focus session and a Reading session on the same day.",
        rarity: RarityTier::Common,
        target: 1,
        unit: "day",
        metric: MetricSelector::DualModalityDays,
    },
    // 5. PERSONAL IMPROVEMENT
    AchievementSpec {
        id: "ach_planned_execution_3",
        family: AchievementFamily::PersonalImprovement,
        title: "Blueprint Executed",
        description: "Complete 3 scheduled sessions from your Planned Sessions queue.",
        rarity: RarityTier::Uncommon,
        target: 3,
        unit: "planned blocks",
        metric: MetricSelector::CompletedPlannedBlocks,
    },
    AchievementSpec {
        id: "ach_reflective_notes_5",
        family: AchievementFamily::PersonalImprovement,
        title: "Reflective Practitioner",
        description: "Record both an objective and completion note on 5 completed sessions.",
        rarity: RarityTier::Uncommon,
        target: 5,
        unit: "annotated sessions",
        metric: MetricSelector::AnnotatedSessions,
    },
    AchievementSpec {
        id: "ach_personal_best_day",
        family: AchievementFamily::PersonalImprovement,
        title: "New Personal Horizon",
        description: "Surpass a previous daily record (where prior record was >= 60 minutes).",
        rarity: RarityTier::Rare,
        target: 1,
        unit: "record",
        metric: MetricSelector::PersonalBestBroken,
    },
    AchievementSpec {
        id: "ach_both_goals_day",
        family: AchievementFamily::PersonalImprovement,
        title: "Full Daily Harmony",
        description: "Meet both your Daily Focus Goal and Daily Reading Goal on the same day.",
        rarity: RarityTier::Rare,
        target: 1,
        unit: "day",
        metric: MetricSelector::BothGoalsMetDays,
    },
    // 6. LONG-TERM MILESTONES
    AchievementSpec {
        id: "ach_sessions_25",
        family: AchievementFamily::LongTermMilestones,
        title: "Quarter-Century of Sessions",
        description: "Complete 25 total Focus and Reading sessions.",
        rarity: RarityTier::Uncommon,
        target: 25,
        unit: "sessions",
        metric: MetricSelector::CompletedSessions,
    },
    AchievementSpec {
        id: "ach_sessions_100",
        family: AchievementFamily::LongTermMilestones,
        title: "Centurion of Attention",
        description: "Complete 100 total Focus and Reading sessions.",
        rarity: RarityTier::Rare,
        target: 100,
        unit: "sessions",
        metric: MetricSelector::CompletedSessions,
    },
    AchievementSpec {
        id: "ach_total_hours_100",
        family: AchievementFamily::LongTermMilestones,
        title: "Hundred-Hour Athenaeum",
        description: "Accumulate 100 combined hours of completed Focus and Reading work.",
        rarity: RarityTier::Mythic,
        target: 100,
        unit: "hours",
        metric: MetricSelector::CompletedCombinedHours,
    },
    AchievementSpec {
        id: "ach_level_10",
        family: AchievementFamily::LongTermMilestones,
        title: "Fellow of Chronos",
        description: "Reach Progression Level 10 through sustained practice.",
        rarity: RarityTier::Mythic,
        target: 10,
        unit: "level",
        metric: MetricSelector::CurrentLevel,
    },
];

pub const LEVEL_CURVE: &[(u32, &str, u64)] = &[
    (1, "Novice Timekeeper", 0),
    (2, "Apprentice of Attention", 150),
    (3, "Cadence Initiate", 380),
    (4, "Deliberate Practitioner", 700),
    (5, "Scholar of the Quiet Hour", 1150),
    (6, "Deep Work Artisan", 1750),
    (7, "Keeper of Momentum", 2500),
    (8, "Synthesist", 3450),
    (9, "Architect of Focus", 4600),
    (10, "Fellow of the Athenaeum", 6000),
    (11, "Master of Intervals", 7700),
    (12, "Monolith Builder", 9700),
    (13, "Sage of Sustained Inquiry", 12000),
    (14, "Principal Polymath", 14700),
    (15, "Vanguard of Clarity", 17800),
    (16, "Luminary of Deep Thought", 21400),
    (17, "Chronos Steward", 25500),
    (18, "Grandmaster of Craft", 30200),
    (19, "Oracle of Quiet Mastery", 35500),
    (20, "Sovereign of Kairos", 42000),
];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnlockRecord {
    pub unlocked_at_utc: DateTime<Utc>,
    pub silent_seed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProgressionLedger {
    pub unlocked: BTreeMap<String, UnlockRecord>,
    pub total_xp: u64,
    pub level: u32,
    pub level_title: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LevelSnapshot {
    pub level: u32,
    pub title: &'static str,
    pub next_title: Option<&'static str>,
    pub total_xp: u64,
    pub xp_into_level: u64,
    pub xp_span_for_level: u64,
    pub xp_needed_for_next: u64,
    pub progress_percent: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AchievementStatusItem {
    pub spec: &'static AchievementSpec,
    pub is_unlocked: bool,
    pub unlocked_at_utc: Option<DateTime<Utc>>,
    pub current_value: u64,
    pub progress_percent: u8,
}

#[derive(Debug, Clone, Default)]
pub struct ProgressionMetrics {
    pub completed_sessions: u64,
    pub completed_reading_sessions: u64,
    pub completed_focus_hours: u64,
    pub completed_reading_hours: u64,
    pub completed_combined_hours: u64,
    pub longest_focus_session_mins: u64,
    pub total_pages_read: u64,
    pub best_combined_streak: u64,
    pub best_reading_streak: u64,
    pub returned_after_break: u64,
    pub focus_goal_days_met: u64,
    pub both_goals_met_days: u64,
    pub custom_categories_count: u64,
    pub distinct_categories_used: u64,
    pub dual_modality_days: u64,
    pub completed_planned_blocks: u64,
    pub annotated_sessions: u64,
    pub personal_best_broken: u64,
    pub base_session_xp: u64,
    pub current_level: u64,
}

impl ProgressionMetrics {
    pub fn get(&self, selector: MetricSelector) -> u64 {
        match selector {
            MetricSelector::CompletedSessions => self.completed_sessions,
            MetricSelector::BestCombinedStreak => self.best_combined_streak,
            MetricSelector::ReturnedAfterBreak => self.returned_after_break,
            MetricSelector::LongestFocusSessionMins => self.longest_focus_session_mins,
            MetricSelector::CompletedFocusHours => self.completed_focus_hours,
            MetricSelector::FocusGoalDaysMet => self.focus_goal_days_met,
            MetricSelector::CompletedReadingSessions => self.completed_reading_sessions,
            MetricSelector::BestReadingStreak => self.best_reading_streak,
            MetricSelector::CompletedReadingHours => self.completed_reading_hours,
            MetricSelector::TotalPagesRead => self.total_pages_read,
            MetricSelector::CustomCategoriesCount => self.custom_categories_count,
            MetricSelector::DistinctCategoriesUsed => self.distinct_categories_used,
            MetricSelector::DualModalityDays => self.dual_modality_days,
            MetricSelector::CompletedPlannedBlocks => self.completed_planned_blocks,
            MetricSelector::AnnotatedSessions => self.annotated_sessions,
            MetricSelector::PersonalBestBroken => self.personal_best_broken,
            MetricSelector::BothGoalsMetDays => self.both_goals_met_days,
            MetricSelector::CompletedCombinedHours => self.completed_combined_hours,
            MetricSelector::CurrentLevel => self.current_level,
        }
    }
}

/// Daily Chest: a surprise XP reward for every day the daily focus goal is met.
/// The amount is derived from the date, so it is stable (recomputed identically every launch)
/// yet unpredictable to the player: 30–89 XP, with a rare jackpot of 150 XP.
pub fn daily_chest_xp(date_key: &str) -> u64 {
    let mut h: u64 = 1469598103934665603;
    for b in date_key.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(1099511628211);
    }
    if h.is_multiple_of(11) {
        150
    } else {
        30 + (h >> 8) % 60
    }
}

pub fn chest_tier(xp: u64) -> &'static str {
    match xp {
        150.. => "Jackpot",
        75..=149 => "Gold",
        50..=74 => "Silver",
        _ => "Bronze",
    }
}

pub fn resolve_level_snapshot(total_xp: u64) -> LevelSnapshot {
    let mut current_idx = 0usize;
    for (idx, &(_, _, min_xp)) in LEVEL_CURVE.iter().enumerate() {
        if total_xp >= min_xp {
            current_idx = idx;
        } else {
            break;
        }
    }

    let (level, title, start_xp) = LEVEL_CURVE[current_idx];
    if let Some(&(_, next_title, next_xp)) = LEVEL_CURVE.get(current_idx + 1) {
        let span = next_xp.saturating_sub(start_xp).max(1);
        let into = total_xp.saturating_sub(start_xp);
        let pct = ((into as u128 * 100) / (span as u128)).min(100) as u8;
        LevelSnapshot {
            level,
            title,
            next_title: Some(next_title),
            total_xp,
            xp_into_level: into,
            xp_span_for_level: span,
            xp_needed_for_next: next_xp.saturating_sub(total_xp),
            progress_percent: pct,
        }
    } else {
        LevelSnapshot {
            level,
            title,
            next_title: None,
            total_xp,
            xp_into_level: total_xp.saturating_sub(start_xp),
            xp_span_for_level: 1,
            xp_needed_for_next: 0,
            progress_percent: 100,
        }
    }
}

pub fn compute_progression_metrics(
    sessions: &[SessionRecord],
    categories: &[CategoryRecord],
    planned_blocks: &[PlannedBlock],
    prefs: &Preferences,
    reference_date_key: &str,
) -> ProgressionMetrics {
    let mut completed_sessions = 0u64;
    let mut completed_reading_sessions = 0u64;
    let mut longest_focus_secs = 0u64;
    let mut total_pages_read = 0u64;
    let mut annotated_sessions = 0u64;
    let mut distinct_cats = BTreeSet::new();
    let mut day_map: BTreeMap<String, (u64, u64)> = BTreeMap::new();

    for s in sessions {
        if s.completion_state != CompletionState::Completed || !s.activity_kind.is_productive() {
            continue;
        }
        completed_sessions += 1;
        distinct_cats.insert(s.category_id.clone());

        if !s.objective.trim().is_empty() && !s.completion_notes.trim().is_empty() {
            annotated_sessions += 1;
        }

        if s.activity_kind == ActivityKind::Focus {
            if s.elapsed_seconds > longest_focus_secs {
                longest_focus_secs = s.elapsed_seconds;
            }
        } else if s.activity_kind == ActivityKind::Reading {
            completed_reading_sessions += 1;
            if let Some(ref meta) = s.reading_meta {
                total_pages_read = total_pages_read.saturating_add(meta.pages_read as u64);
            }
        }

        if s.day_slices.is_empty() {
            let entry = day_map.entry(s.primary_date_key.clone()).or_insert((0, 0));
            if s.activity_kind == ActivityKind::Focus {
                entry.0 = entry.0.saturating_add(s.elapsed_seconds);
            } else {
                entry.1 = entry.1.saturating_add(s.elapsed_seconds);
            }
        } else {
            for sl in &s.day_slices {
                if sl.seconds == 0 {
                    continue;
                }
                let entry = day_map.entry(sl.date_key.clone()).or_insert((0, 0));
                if s.activity_kind == ActivityKind::Focus {
                    entry.0 = entry.0.saturating_add(sl.seconds);
                } else {
                    entry.1 = entry.1.saturating_add(sl.seconds);
                }
            }
        }
    }

    let valid_focus_goal_secs =
        if prefs.goals.daily_focus_minutes >= MIN_FOCUS_GOAL_MINS_FOR_REWARD {
            Some((prefs.goals.daily_focus_minutes as u64) * 60)
        } else {
            None
        };
    let valid_reading_goal_secs =
        if prefs.goals.daily_reading_minutes >= MIN_READING_GOAL_MINS_FOR_REWARD {
            Some((prefs.goals.daily_reading_minutes as u64) * 60)
        } else {
            None
        };

    let mut raw_focus_secs = 0u64;
    let mut raw_reading_secs = 0u64;
    let mut capped_reward_secs = 0u64;
    let mut dual_modality_days = 0u64;
    let mut focus_goal_days_met = 0u64;
    let mut chest_xp = 0u64;
    let mut both_goals_met_days = 0u64;
    let mut returned_after_break = 0u64;
    let mut personal_best_broken = 0u64;
    let mut running_best_day_secs = 0u64;
    let mut prev_active_date: Option<chrono::NaiveDate> = None;

    for (d_key, (f_secs, r_secs)) in &day_map {
        let day_total = f_secs.saturating_add(*r_secs);
        raw_focus_secs = raw_focus_secs.saturating_add(*f_secs);
        raw_reading_secs = raw_reading_secs.saturating_add(*r_secs);
        capped_reward_secs =
            capped_reward_secs.saturating_add(day_total.min(MAX_DAILY_REWARDED_SECS));

        if *f_secs >= 600 && *r_secs >= 600 {
            dual_modality_days += 1;
        }

        let met_focus = valid_focus_goal_secs.is_some_and(|t| *f_secs >= t);
        let met_reading = valid_reading_goal_secs.is_some_and(|t| *r_secs >= t);
        if met_focus {
            focus_goal_days_met += 1;
            chest_xp += daily_chest_xp(d_key);
        }
        if met_focus && met_reading {
            both_goals_met_days += 1;
        }

        if let Some(cur_date) = parse_date_key(d_key) {
            if let Some(prev_date) = prev_active_date {
                let gap = (cur_date - prev_date).num_days();
                if gap >= 3 && day_total >= 600 {
                    returned_after_break += 1;
                }
            }
            if day_total >= 60 {
                prev_active_date = Some(cur_date);
            }
        }

        if running_best_day_secs >= 3600 && day_total > running_best_day_secs {
            personal_best_broken += 1;
        }
        if day_total > running_best_day_secs {
            running_best_day_secs = day_total;
        }
    }

    let streaks = compute_streaks(sessions, reference_date_key, false);
    let custom_categories_count = categories.iter().filter(|c| !c.is_default).count() as u64;
    let completed_planned_blocks = planned_blocks
        .iter()
        .filter(|p| p.status == PlanStatus::Completed)
        .count() as u64;

    let base_session_xp = (capped_reward_secs / 60) * 2
        + completed_sessions * 15
        + annotated_sessions * 10
        + completed_planned_blocks * 20
        + (total_pages_read / 2)
        + chest_xp;

    ProgressionMetrics {
        completed_sessions,
        completed_reading_sessions,
        completed_focus_hours: raw_focus_secs / 3600,
        completed_reading_hours: raw_reading_secs / 3600,
        completed_combined_hours: (raw_focus_secs + raw_reading_secs) / 3600,
        longest_focus_session_mins: longest_focus_secs / 60,
        total_pages_read,
        best_combined_streak: streaks.combined.best_days as u64,
        best_reading_streak: streaks.reading.best_days as u64,
        returned_after_break,
        focus_goal_days_met,
        both_goals_met_days,
        custom_categories_count,
        distinct_categories_used: distinct_cats.len() as u64,
        dual_modality_days,
        completed_planned_blocks,
        annotated_sessions,
        personal_best_broken,
        base_session_xp,
        current_level: 1,
    }
}

/// Evaluates all 24 achievements against current store data, updates `ledger` idempotently,
/// and returns `(LevelSnapshot, Vec<AchievementStatusItem>, Vec<&'static AchievementSpec>)`.
pub fn evaluate_progression(
    ledger: &mut ProgressionLedger,
    sessions: &[SessionRecord],
    categories: &[CategoryRecord],
    planned_blocks: &[PlannedBlock],
    prefs: &Preferences,
    reference_date_key: &str,
    silent_seed: bool,
    now_utc: DateTime<Utc>,
) -> (
    LevelSnapshot,
    Vec<AchievementStatusItem>,
    Vec<&'static AchievementSpec>,
) {
    let mut metrics = compute_progression_metrics(
        sessions,
        categories,
        planned_blocks,
        prefs,
        reference_date_key,
    );

    let mut newly_unlocked: Vec<&'static AchievementSpec> = Vec::new();

    // Pass 1: non-level achievements
    for spec in ACHIEVEMENT_CATALOG {
        if spec.metric == MetricSelector::CurrentLevel {
            continue;
        }
        let val = metrics.get(spec.metric);
        if val >= spec.target && !ledger.unlocked.contains_key(spec.id) {
            ledger.unlocked.insert(
                spec.id.to_string(),
                UnlockRecord {
                    unlocked_at_utc: now_utc,
                    silent_seed,
                },
            );
            if !silent_seed {
                newly_unlocked.push(spec);
            }
        }
    }

    let bonus_xp = |unlocked_map: &BTreeMap<String, UnlockRecord>| -> u64 {
        ACHIEVEMENT_CATALOG
            .iter()
            .filter(|s| unlocked_map.contains_key(s.id))
            .map(|s| s.rarity.xp_bonus())
            .sum()
    };

    let mut total_xp = metrics.base_session_xp + bonus_xp(&ledger.unlocked);
    let mut level_snap = resolve_level_snapshot(total_xp);
    metrics.current_level = level_snap.level as u64;

    // Pass 2: level-dependent achievements
    for spec in ACHIEVEMENT_CATALOG {
        if spec.metric != MetricSelector::CurrentLevel {
            continue;
        }
        if metrics.current_level >= spec.target && !ledger.unlocked.contains_key(spec.id) {
            ledger.unlocked.insert(
                spec.id.to_string(),
                UnlockRecord {
                    unlocked_at_utc: now_utc,
                    silent_seed,
                },
            );
            if !silent_seed {
                newly_unlocked.push(spec);
            }
        }
    }

    total_xp = metrics.base_session_xp + bonus_xp(&ledger.unlocked);
    level_snap = resolve_level_snapshot(total_xp);

    ledger.total_xp = total_xp;
    ledger.level = level_snap.level;
    ledger.level_title = level_snap.title.to_string();

    let items = ACHIEVEMENT_CATALOG
        .iter()
        .map(|spec| {
            let rec = ledger.unlocked.get(spec.id);
            let cur = metrics.get(spec.metric);
            let pct = if rec.is_some() {
                100
            } else {
                ((cur.min(spec.target) as u128 * 100) / (spec.target.max(1) as u128)).min(99) as u8
            };
            AchievementStatusItem {
                spec,
                is_unlocked: rec.is_some(),
                unlocked_at_utc: rec.map(|r| r.unlocked_at_utc),
                current_value: cur,
                progress_percent: pct,
            }
        })
        .collect();

    (level_snap, items, newly_unlocked)
}

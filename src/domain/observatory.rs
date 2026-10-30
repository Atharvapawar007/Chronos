//! Chronos Analytics Observatory Engine (`observatory.rs`).
//!
//! Computes trustworthy statistics across configurable horizons (`Today`, `7d`, `30d`, `90d`, `365d`, `All`),
//! period-over-period comparisons (with zero-baseline safety), streaks, circadian 24h distributions,
//! category breakdowns, completion/interruption rates, and personal records.

use crate::domain::models::{
    parse_date_key, shift_date_key, to_local_hour, ActivityKind, CompletionState, Preferences,
    SessionRecord,
};
use crate::domain::planner::attribute_day_seconds;
use crate::domain::taxonomy::CategoryRecord;
use chrono::Datelike;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HorizonFilter {
    Today,
    Days7,
    Days30,
    Days90,
    Year1,
    AllTime,
}

impl HorizonFilter {
    pub const ALL: [HorizonFilter; 6] = [
        HorizonFilter::Today,
        HorizonFilter::Days7,
        HorizonFilter::Days30,
        HorizonFilter::Days90,
        HorizonFilter::Year1,
        HorizonFilter::AllTime,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HorizonFilter::Today => "Today",
            HorizonFilter::Days7 => "7 Days",
            HorizonFilter::Days30 => "30 Days",
            HorizonFilter::Days90 => "90 Days",
            HorizonFilter::Year1 => "365 Days",
            HorizonFilter::AllTime => "All Time",
        }
    }

    pub fn short_key(self) -> &'static str {
        match self {
            HorizonFilter::Today => "today",
            HorizonFilter::Days7 => "7d",
            HorizonFilter::Days30 => "30d",
            HorizonFilter::Days90 => "90d",
            HorizonFilter::Year1 => "1y",
            HorizonFilter::AllTime => "all",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key.trim().to_lowercase().as_str() {
            "today" | "1d" => HorizonFilter::Today,
            "7d" | "week" => HorizonFilter::Days7,
            "30d" | "month" => HorizonFilter::Days30,
            "90d" | "quarter" => HorizonFilter::Days90,
            "1y" | "365d" | "year" => HorizonFilter::Year1,
            "all" | "lifetime" => HorizonFilter::AllTime,
            _ => HorizonFilter::Days7,
        }
    }

    pub fn next(self) -> Self {
        match self {
            HorizonFilter::Today => HorizonFilter::Days7,
            HorizonFilter::Days7 => HorizonFilter::Days30,
            HorizonFilter::Days30 => HorizonFilter::Days90,
            HorizonFilter::Days90 => HorizonFilter::Year1,
            HorizonFilter::Year1 => HorizonFilter::AllTime,
            HorizonFilter::AllTime => HorizonFilter::Today,
        }
    }

    pub fn prev(self) -> Self {
        match self {
            HorizonFilter::Today => HorizonFilter::AllTime,
            HorizonFilter::Days7 => HorizonFilter::Today,
            HorizonFilter::Days30 => HorizonFilter::Days7,
            HorizonFilter::Days90 => HorizonFilter::Days30,
            HorizonFilter::Year1 => HorizonFilter::Days90,
            HorizonFilter::AllTime => HorizonFilter::Year1,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct StreakPair {
    pub current_days: u32,
    pub best_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct StreakSummary {
    pub combined: StreakPair,
    pub focus: StreakPair,
    pub reading: StreakPair,
    pub total_active_days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DailyPoint {
    pub date_key: String,
    pub focus_seconds: u64,
    pub reading_seconds: u64,
    pub break_seconds: u64,
    pub pages_read: u32,
}

impl DailyPoint {
    pub fn total_productive(&self) -> u64 {
        self.focus_seconds.saturating_add(self.reading_seconds)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CategoryAnalyticsRow {
    pub category_id: String,
    pub name: String,
    pub color_hex: String,
    pub icon_glyph: String,
    pub is_archived: bool,
    pub total_seconds: u64,
    pub share_percent: f64,
    pub sessions_count: u32,
    pub average_session_seconds: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObservatoryReport {
    pub horizon: HorizonFilter,
    pub start_date_key: String,
    pub end_date_key: String,
    pub span_days: u32,
    pub total_focus_seconds: u64,
    pub total_reading_seconds: u64,
    pub total_productive_seconds: u64,
    pub total_break_seconds: u64,
    pub total_pages_read: u32,
    pub completed_sessions_count: u32,
    pub interrupted_sessions_count: u32,
    pub completion_rate_percent: Option<u8>,
    pub interruption_rate_percent: Option<u8>,
    pub average_session_seconds: u64,
    pub daily_average_seconds: u64,
    pub previous_period_seconds: Option<u64>,
    pub period_delta_percent: Option<i32>,
    pub daily_series: Vec<DailyPoint>,
    pub hourly_distribution: [u64; 24],
    pub weekday_distribution: [u64; 7],
    pub category_rows: Vec<CategoryAnalyticsRow>,
    pub streaks: StreakSummary,
    pub peak_day_key: Option<String>,
    pub peak_day_seconds: u64,
    pub longest_session_seconds: u64,
    pub longest_session_label: String,
    pub insights: Vec<String>,
}

/// Computes current and best consecutive-day streaks across all recorded history.
pub fn compute_streaks(
    sessions: &[SessionRecord],
    reference_date_key: &str,
    include_interrupted: bool,
) -> StreakSummary {
    let mut day_totals: BTreeMap<String, (u64, u64)> = BTreeMap::new();

    for s in sessions {
        if !include_interrupted && s.completion_state == CompletionState::Interrupted {
            continue;
        }
        if !s.activity_kind.is_productive() {
            continue;
        }

        if s.day_slices.is_empty() {
            let entry = day_totals
                .entry(s.primary_date_key.clone())
                .or_insert((0, 0));
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
                let entry = day_totals.entry(sl.date_key.clone()).or_insert((0, 0));
                if s.activity_kind == ActivityKind::Focus {
                    entry.0 = entry.0.saturating_add(sl.seconds);
                } else {
                    entry.1 = entry.1.saturating_add(sl.seconds);
                }
            }
        }
    }

    if day_totals.is_empty() {
        return StreakSummary::default();
    }

    let evaluate_pair = |pred: fn(&(u64, u64)) -> bool| -> StreakPair {
        let mut best = 0u32;
        let mut running = 0u32;
        let mut prev_key: Option<String> = None;

        for (d_key, pair) in &day_totals {
            if !pred(pair) {
                continue;
            }
            if let Some(ref pk) = prev_key {
                if shift_date_key(pk, 1) == *d_key {
                    running += 1;
                } else {
                    running = 1;
                }
            } else {
                running = 1;
            }
            if running > best {
                best = running;
            }
            prev_key = Some(d_key.clone());
        }

        // Compute current streak anchored at reference_date_key or yesterday
        let mut current = 0u32;
        let mut cursor = reference_date_key.to_string();
        if !day_totals.get(&cursor).is_some_and(pred) {
            cursor = shift_date_key(reference_date_key, -1);
        }
        while day_totals.get(&cursor).is_some_and(pred) {
            current += 1;
            cursor = shift_date_key(&cursor, -1);
        }

        StreakPair {
            current_days: current,
            best_days: best.max(current),
        }
    };

    let total_active_days = day_totals
        .values()
        .filter(|(f, r)| f.saturating_add(*r) >= 60)
        .count() as u32;

    StreakSummary {
        combined: evaluate_pair(|(f, r)| f.saturating_add(*r) >= 60),
        focus: evaluate_pair(|(f, _)| *f >= 60),
        reading: evaluate_pair(|(_, r)| *r >= 60),
        total_active_days,
    }
}

/// Generates a full `ObservatoryReport` for the selected `HorizonFilter` and optional `category_filter_id`.
pub fn build_observatory_report(
    sessions: &[SessionRecord],
    categories: &[CategoryRecord],
    prefs: &Preferences,
    horizon: HorizonFilter,
    category_filter_id: Option<&str>,
    reference_date_key: &str,
    fixed_offset_minutes: Option<i32>,
) -> ObservatoryReport {
    let include_interrupted = prefs.focus.count_interrupted_in_totals;

    let (start_date_key, span_days, has_prev_period) = match horizon {
        HorizonFilter::Today => (reference_date_key.to_string(), 1u32, true),
        HorizonFilter::Days7 => (shift_date_key(reference_date_key, -6), 7u32, true),
        HorizonFilter::Days30 => (shift_date_key(reference_date_key, -29), 30u32, true),
        HorizonFilter::Days90 => (shift_date_key(reference_date_key, -89), 90u32, true),
        HorizonFilter::Year1 => (shift_date_key(reference_date_key, -364), 365u32, true),
        HorizonFilter::AllTime => {
            let mut earliest = reference_date_key.to_string();
            for s in sessions {
                if s.primary_date_key < earliest {
                    earliest = s.primary_date_key.clone();
                }
                for sl in &s.day_slices {
                    if sl.date_key < earliest {
                        earliest = sl.date_key.clone();
                    }
                }
            }
            let days = match (parse_date_key(&earliest), parse_date_key(reference_date_key)) {
                (Some(s), Some(e)) if e >= s => ((e - s).num_days() as u32).saturating_add(1),
                _ => 1,
            };
            (earliest, days.max(1), false)
        }
    };

    let mut daily_series = Vec::with_capacity(span_days as usize);
    let mut total_focus_seconds = 0u64;
    let mut total_reading_seconds = 0u64;
    let mut total_break_seconds = 0u64;
    let mut total_pages_read = 0u32;
    let mut category_totals: BTreeMap<String, u64> = BTreeMap::new();

    for offset in 0..span_days {
        let d_key = shift_date_key(&start_date_key, offset as i64);
        let day_attr = attribute_day_seconds(sessions, &d_key, include_interrupted);

        let (day_focus, day_reading) = if let Some(cid) = category_filter_id {
            let cat_secs = *day_attr.by_category_seconds.get(cid).unwrap_or(&0);
            let is_reading_cat = categories
                .iter()
                .find(|c| c.id == cid)
                .is_some_and(|c| matches!(c.scope, crate::domain::models::CategoryScope::ReadingOnly));
            if is_reading_cat {
                (0, cat_secs)
            } else {
                (cat_secs, 0)
            }
        } else {
            (day_attr.focus_seconds, day_attr.reading_seconds)
        };

        total_focus_seconds = total_focus_seconds.saturating_add(day_focus);
        total_reading_seconds = total_reading_seconds.saturating_add(day_reading);
        total_break_seconds = total_break_seconds.saturating_add(day_attr.break_seconds);
        total_pages_read = total_pages_read.saturating_add(day_attr.pages_read);

        for (cid, secs) in day_attr.by_category_seconds {
            if category_filter_id.is_none_or(|f| f == cid) {
                *category_totals.entry(cid).or_insert(0) += secs;
            }
        }

        daily_series.push(DailyPoint {
            date_key: d_key,
            focus_seconds: day_focus,
            reading_seconds: day_reading,
            break_seconds: day_attr.break_seconds,
            pages_read: day_attr.pages_read,
        });
    }

    // Previous equivalent period comparison
    let (previous_period_seconds, period_delta_percent) = if has_prev_period {
        let prev_start = shift_date_key(&start_date_key, -(span_days as i64));
        let mut prev_total = 0u64;
        for offset in 0..span_days {
            let p_key = shift_date_key(&prev_start, offset as i64);
            let p_attr = attribute_day_seconds(sessions, &p_key, include_interrupted);
            if let Some(cid) = category_filter_id {
                prev_total =
                    prev_total.saturating_add(*p_attr.by_category_seconds.get(cid).unwrap_or(&0));
            } else {
                prev_total = prev_total.saturating_add(p_attr.productive_seconds());
            }
        }
        let cur_total = total_focus_seconds.saturating_add(total_reading_seconds);
        let delta = if prev_total > 0 {
            let diff = (cur_total as f64) - (prev_total as f64);
            Some(((diff / (prev_total as f64)) * 100.0).round() as i32)
        } else {
            None
        };
        (Some(prev_total), delta)
    } else {
        (None, None)
    };

    // Session-level metrics within the date window
    let mut completed_sessions_count = 0u32;
    let mut interrupted_sessions_count = 0u32;
    let mut hourly_distribution = [0u64; 24];
    let mut weekday_distribution = [0u64; 7];
    let mut cat_session_counts: BTreeMap<String, u32> = BTreeMap::new();

    for s in sessions {
        if !s.activity_kind.is_productive() {
            continue;
        }
        if let Some(cid) = category_filter_id {
            if s.category_id != cid {
                continue;
            }
        }
        let in_window = if s.day_slices.is_empty() {
            s.primary_date_key.as_str() >= start_date_key.as_str()
                && s.primary_date_key.as_str() <= reference_date_key
        } else {
            s.day_slices.iter().any(|sl| {
                sl.date_key.as_str() >= start_date_key.as_str()
                    && sl.date_key.as_str() <= reference_date_key
            })
        };
        if !in_window {
            continue;
        }

        match s.completion_state {
            CompletionState::Completed => completed_sessions_count += 1,
            CompletionState::Interrupted => interrupted_sessions_count += 1,
        }

        if include_interrupted || s.completion_state == CompletionState::Completed {
            *cat_session_counts.entry(s.category_id.clone()).or_insert(0) += 1;
            let hr = to_local_hour(s.started_at_utc, fixed_offset_minutes).min(23);
            hourly_distribution[hr] = hourly_distribution[hr].saturating_add(s.elapsed_seconds);

            if let Some(d) = parse_date_key(&s.primary_date_key) {
                let wd = d.weekday().num_days_from_monday() as usize;
                weekday_distribution[wd] =
                    weekday_distribution[wd].saturating_add(s.elapsed_seconds);
            }
        }
    }

    let total_evaluated_sessions = completed_sessions_count + interrupted_sessions_count;
    let (completion_rate_percent, interruption_rate_percent) = if total_evaluated_sessions > 0 {
        let comp = ((completed_sessions_count as u64 * 100) / (total_evaluated_sessions as u64))
            .min(100) as u8;
        let intr = 100u8.saturating_sub(comp);
        (Some(comp), Some(intr))
    } else {
        (None, None)
    };

    let active_sessions_for_avg = if include_interrupted {
        total_evaluated_sessions
    } else {
        completed_sessions_count
    };
    let total_productive_seconds = total_focus_seconds.saturating_add(total_reading_seconds);
    let average_session_seconds = if active_sessions_for_avg > 0 {
        total_productive_seconds / (active_sessions_for_avg as u64)
    } else {
        0
    };
    let daily_average_seconds = total_productive_seconds / (span_days.max(1) as u64);

    let mut category_rows: Vec<CategoryAnalyticsRow> = categories
        .iter()
        .map(|cat| {
            let secs = *category_totals.get(&cat.id).unwrap_or(&0);
            let cnt = *cat_session_counts.get(&cat.id).unwrap_or(&0);
            let share = if total_productive_seconds > 0 {
                ((secs as f64) / (total_productive_seconds as f64) * 1000.0).round() / 10.0
            } else {
                0.0
            };
            CategoryAnalyticsRow {
                category_id: cat.id.clone(),
                name: cat.name.clone(),
                color_hex: cat.color_hex.clone(),
                icon_glyph: cat.icon_glyph.clone(),
                is_archived: cat.is_archived,
                total_seconds: secs,
                share_percent: share,
                sessions_count: cnt,
                average_session_seconds: if cnt > 0 { secs / (cnt as u64) } else { 0 },
            }
        })
        .filter(|r| r.total_seconds > 0 || !r.is_archived)
        .collect();
    category_rows.sort_by_key(|r| std::cmp::Reverse(r.total_seconds));

    // All-time Personal Records
    let mut all_day_map: BTreeMap<String, u64> = BTreeMap::new();
    let mut longest_session_seconds = 0u64;
    let mut longest_session_label = "None yet".to_string();

    for s in sessions {
        if !s.activity_kind.is_productive() {
            continue;
        }
        if !include_interrupted && s.completion_state == CompletionState::Interrupted {
            continue;
        }
        if s.elapsed_seconds > longest_session_seconds {
            longest_session_seconds = s.elapsed_seconds;
            longest_session_label = if !s.objective.is_empty() {
                s.objective.clone()
            } else if let Some(ref m) = s.reading_meta {
                m.title.clone()
            } else {
                s.activity_kind.label().to_string()
            };
        }
        if s.day_slices.is_empty() {
            *all_day_map.entry(s.primary_date_key.clone()).or_insert(0) += s.elapsed_seconds;
        } else {
            for sl in &s.day_slices {
                *all_day_map.entry(sl.date_key.clone()).or_insert(0) += sl.seconds;
            }
        }
    }

    let (peak_day_key, peak_day_seconds) = all_day_map
        .into_iter()
        .max_by_key(|(_, secs)| *secs)
        .map(|(k, v)| (Some(k), v))
        .unwrap_or((None, 0));

    let streaks = compute_streaks(sessions, reference_date_key, include_interrupted);

    let mut insights = Vec::new();
    if total_productive_seconds == 0 {
        insights.push(
            "No productive focus or reading time recorded in this horizon yet.".to_string(),
        );
    } else {
        if let Some((peak_hr, &hr_secs)) = hourly_distribution
            .iter()
            .enumerate()
            .max_by_key(|(_, &v)| v)
        {
            if hr_secs > 0 {
                insights.push(format!(
                    "Peak circadian concentration window is {:02}:00–{:02}:00.",
                    peak_hr,
                    (peak_hr + 1) % 24
                ));
            }
        }
        if let Some(delta) = period_delta_percent {
            let sign = if delta >= 0 { "+" } else { "" };
            insights.push(format!(
                "Productive output shifted by {}{}% compared to the prior {}-day period.",
                sign, delta, span_days
            ));
        }
        if let Some(comp) = completion_rate_percent {
            insights.push(format!(
                "Session completion rate is {}% ({} completed, {} interrupted).",
                comp, completed_sessions_count, interrupted_sessions_count
            ));
        }
    }

    ObservatoryReport {
        horizon,
        start_date_key,
        end_date_key: reference_date_key.to_string(),
        span_days,
        total_focus_seconds,
        total_reading_seconds,
        total_productive_seconds,
        total_break_seconds,
        total_pages_read,
        completed_sessions_count,
        interrupted_sessions_count,
        completion_rate_percent,
        interruption_rate_percent,
        average_session_seconds,
        daily_average_seconds,
        previous_period_seconds,
        period_delta_percent,
        daily_series,
        hourly_distribution,
        weekday_distribution,
        category_rows,
        streaks,
        peak_day_key,
        peak_day_seconds,
        longest_session_seconds,
        longest_session_label,
        insights,
    }
}

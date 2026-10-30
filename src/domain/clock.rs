//! Chronos Monotonic / Wall-Clock Anchored Timer Engine (`ClockEngine`).
//!
//! Guarantees:
//! - Accurate timekeeping anchored to UTC wall-clock timestamps (`DateTime<Utc>`) so OS sleep,
//!   suspend, or delayed UI frame renders never cause timer drift.
//! - Explicit state transitions: `Idle` -> `Running` <-> `Paused` -> `AwaitingCompletion` -> `Idle`.
//! - Safe duration adjustments (`+5m` / `-5m`) that modify the target length without ever fabricating elapsed time.
//! - Automatic midnight splitting (`split_across_midnights`) so sessions crossing `00:00` credit each calendar day accurately.
//! - Overlap guard (`resolve_session_overlap`) so overlapping intervals never double-count elapsed time.

use crate::domain::models::{
    generate_entity_id, parse_date_key, start_of_local_date_utc, to_local_date_key, ActivityKind,
    CadencePreset, CompletionState, DaySlice, FocusPrefs, MaterialKind, Preferences,
    ReadingSessionMeta, SessionRecord,
};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};

pub const MIN_PHASE_SECONDS: u64 = 60;
pub const MAX_PHASE_SECONDS: u64 = 4 * 3600;
pub const MIN_RECORDABLE_INTERRUPTED_SECS: u64 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClockStatus {
    Idle,
    Running,
    Paused,
    AwaitingCompletion,
}

impl ClockStatus {
    pub fn label(self) -> &'static str {
        match self {
            ClockStatus::Idle => "Ready",
            ClockStatus::Running => "In Progress",
            ClockStatus::Paused => "Paused",
            ClockStatus::AwaitingCompletion => "Phase Complete",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LiveClockSnapshot {
    pub status: ClockStatus,
    pub elapsed_seconds: u64,
    pub remaining_seconds: u64,
    pub target_seconds: u64,
    pub progress_permille: u16,
    pub is_complete: bool,
}

/// Serializable state of the active timer, persisted atomically so interrupted or crashed
/// sessions can be recovered on restart without duplicating elapsed time.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClockEngine {
    pub status: ClockStatus,
    pub active_session_id: Option<String>,
    pub activity_kind: ActivityKind,
    pub cadence: CadencePreset,
    pub category_id: String,
    pub configured_seconds: u64,
    pub target_seconds: u64,
    pub accumulated_elapsed_seconds: u64,
    pub segment_started_at_utc: Option<DateTime<Utc>>,
    pub session_started_at_utc: Option<DateTime<Utc>>,
    pub completed_at_utc: Option<DateTime<Utc>>,
    pub completed_focus_intervals: u32,
    pub required_focus_intervals: u32,
    pub objective: String,
    pub completion_notes: String,
    pub planned_block_id: Option<String>,
    // Reading context
    pub reading_material_id: Option<String>,
    pub reading_title: String,
    pub reading_author: String,
    pub reading_kind: MaterialKind,
    pub start_page: Option<u32>,
    pub end_page: Option<u32>,
}

impl ClockEngine {
    pub fn new_from_prefs(prefs: &Preferences) -> Self {
        let (focus_m, _, _, intervals) = prefs
            .focus
            .active_cadence
            .schedule_minutes(&prefs.focus);
        let dur_secs = (focus_m as u64) * 60;
        Self {
            status: ClockStatus::Idle,
            active_session_id: None,
            activity_kind: ActivityKind::Focus,
            cadence: prefs.focus.active_cadence,
            category_id: prefs.focus.default_category_id.clone(),
            configured_seconds: dur_secs,
            target_seconds: dur_secs,
            accumulated_elapsed_seconds: 0,
            segment_started_at_utc: None,
            session_started_at_utc: None,
            completed_at_utc: None,
            completed_focus_intervals: 0,
            required_focus_intervals: intervals,
            objective: String::new(),
            completion_notes: String::new(),
            planned_block_id: None,
            reading_material_id: None,
            reading_title: String::new(),
            reading_author: String::new(),
            reading_kind: MaterialKind::Book,
            start_page: None,
            end_page: None,
        }
    }

    /// Computes the live wall-clock snapshot at `now_utc`.
    pub fn snapshot(&self, now_utc: DateTime<Utc>) -> LiveClockSnapshot {
        let target = self.target_seconds.max(1);
        if self.status == ClockStatus::Idle {
            return LiveClockSnapshot {
                status: ClockStatus::Idle,
                elapsed_seconds: 0,
                remaining_seconds: target,
                target_seconds: target,
                progress_permille: 0,
                is_complete: false,
            };
        }

        let mut raw_elapsed = self.accumulated_elapsed_seconds;
        if self.status == ClockStatus::Running {
            if let Some(seg_start) = self.segment_started_at_utc {
                let delta = (now_utc - seg_start).num_seconds().max(0) as u64;
                raw_elapsed = raw_elapsed.saturating_add(delta);
            }
        }

        let clamped_elapsed = raw_elapsed.min(target);
        let remaining = target.saturating_sub(raw_elapsed);
        let is_complete = self.status == ClockStatus::AwaitingCompletion || remaining == 0;
        let permille = ((clamped_elapsed as u128 * 1000) / (target as u128)).min(1000) as u16;

        LiveClockSnapshot {
            status: if is_complete {
                ClockStatus::AwaitingCompletion
            } else {
                self.status
            },
            elapsed_seconds: clamped_elapsed,
            remaining_seconds: remaining,
            target_seconds: target,
            progress_permille: permille,
            is_complete,
        }
    }

    /// Starts a new session or resumes a paused session.
    pub fn start_or_resume(&mut self, now_utc: DateTime<Utc>) {
        match self.status {
            ClockStatus::Idle => {
                self.status = ClockStatus::Running;
                self.active_session_id = Some(generate_entity_id("ses"));
                self.accumulated_elapsed_seconds = 0;
                self.segment_started_at_utc = Some(now_utc);
                self.session_started_at_utc = Some(now_utc);
                self.completed_at_utc = None;
            }
            ClockStatus::Paused => {
                self.status = ClockStatus::Running;
                self.segment_started_at_utc = Some(now_utc);
            }
            ClockStatus::Running | ClockStatus::AwaitingCompletion => {}
        }
    }

    /// Pauses a running session and locks in exact elapsed seconds.
    pub fn pause(&mut self, now_utc: DateTime<Utc>) {
        if self.status != ClockStatus::Running {
            return;
        }
        let snap = self.snapshot(now_utc);
        if snap.is_complete {
            self.mark_awaiting_completion(now_utc);
            return;
        }
        self.status = ClockStatus::Paused;
        self.accumulated_elapsed_seconds = snap.elapsed_seconds;
        self.segment_started_at_utc = None;
    }

    /// Toggles between Running and Paused/Idle.
    pub fn toggle_run_pause(&mut self, now_utc: DateTime<Utc>) {
        match self.status {
            ClockStatus::Idle | ClockStatus::Paused => self.start_or_resume(now_utc),
            ClockStatus::Running => self.pause(now_utc),
            ClockStatus::AwaitingCompletion => {}
        }
    }

    /// Marks the current phase as having reached `00:00` (`AwaitingCompletion`).
    pub fn mark_awaiting_completion(&mut self, now_utc: DateTime<Utc>) {
        if self.status == ClockStatus::Idle {
            return;
        }
        self.status = ClockStatus::AwaitingCompletion;
        self.accumulated_elapsed_seconds = self.target_seconds;
        self.segment_started_at_utc = None;
        if self.completed_at_utc.is_none() {
            self.completed_at_utc = Some(now_utc);
        }
    }

    /// Checks whether a running clock has reached `00:00` and transitions it if so.
    /// Returns `true` on the exact transition into `AwaitingCompletion`.
    pub fn sync_wall_clock(&mut self, now_utc: DateTime<Utc>) -> bool {
        if self.status == ClockStatus::Running {
            let snap = self.snapshot(now_utc);
            if snap.is_complete {
                self.mark_awaiting_completion(now_utc);
                return true;
            }
        }
        false
    }

    /// Adjusts the target duration by `delta_seconds` (e.g., `+300` or `-300`).
    /// Never fabricates elapsed time and always leaves at least 1 second remaining when active.
    pub fn adjust_duration_secs(&mut self, delta_seconds: i64, now_utc: DateTime<Utc>) {
        // A finished phase is awaiting its record; changing its target would corrupt the result.
        if self.status == ClockStatus::AwaitingCompletion {
            return;
        }
        let snap = self.snapshot(now_utc);
        let min_allowed = if self.status == ClockStatus::Idle {
            MIN_PHASE_SECONDS
        } else {
            MIN_PHASE_SECONDS.max(snap.elapsed_seconds.saturating_add(1))
        }
        .min(MAX_PHASE_SECONDS);

        let signed_current = self.target_seconds as i64;
        let next_target = (signed_current + delta_seconds)
            .clamp(min_allowed as i64, MAX_PHASE_SECONDS as i64) as u64;

        self.target_seconds = next_target;
        if self.status == ClockStatus::Idle {
            self.configured_seconds = next_target;
        }
    }

    /// Resets the current phase back to `Idle` at its configured length.
    pub fn reset_phase(&mut self) {
        self.status = ClockStatus::Idle;
        self.active_session_id = None;
        self.target_seconds = self.configured_seconds;
        self.accumulated_elapsed_seconds = 0;
        self.segment_started_at_utc = None;
        self.session_started_at_utc = None;
        self.completed_at_utc = None;
    }

    /// Configures the clock for a specific `ActivityKind` and updates durations from preferences.
    pub fn switch_activity(&mut self, kind: ActivityKind, prefs: &Preferences) {
        let (focus_m, short_m, long_m, intervals) =
            self.cadence.schedule_minutes(&prefs.focus);
        let dur_secs = match kind {
            ActivityKind::Focus => (focus_m as u64) * 60,
            ActivityKind::Reading => (prefs.reading.default_duration_minutes as u64) * 60,
            ActivityKind::ShortBreak => (short_m as u64) * 60,
            ActivityKind::LongBreak => (long_m as u64) * 60,
        };
        self.activity_kind = kind;
        self.configured_seconds = dur_secs;
        self.target_seconds = dur_secs;
        self.required_focus_intervals = intervals;
        if kind == ActivityKind::Reading && self.category_id == prefs.focus.default_category_id {
            self.category_id = prefs.reading.default_category_id.clone();
        } else if kind == ActivityKind::Focus
            && self.category_id == prefs.reading.default_category_id
        {
            self.category_id = prefs.focus.default_category_id.clone();
        }
        self.reset_phase();
    }

    /// Switches cadence preset and loads its focus schedule.
    pub fn select_cadence(&mut self, cadence: CadencePreset, prefs: &FocusPrefs) {
        let (focus_m, short_m, long_m, intervals) = cadence.schedule_minutes(prefs);
        self.cadence = cadence;
        self.required_focus_intervals = intervals;
        self.completed_focus_intervals = 0;
        let dur_secs = match self.activity_kind {
            ActivityKind::Focus => (focus_m as u64) * 60,
            ActivityKind::ShortBreak => (short_m as u64) * 60,
            ActivityKind::LongBreak => (long_m as u64) * 60,
            ActivityKind::Reading => self.configured_seconds,
        };
        self.configured_seconds = dur_secs;
        self.target_seconds = dur_secs;
        self.reset_phase();
    }

    /// Finalizes the current session (either as `Completed` or `Interrupted`),
    /// advances the cadence state machine, and returns `(Option<SessionRecord>, completed_full_cycle)`.
    pub fn finalize_session(
        &mut self,
        force_completed: bool,
        prefs: &Preferences,
        now_utc: DateTime<Utc>,
        fixed_offset_minutes: Option<i32>,
    ) -> (Option<SessionRecord>, bool) {
        let snap = self.snapshot(now_utc);
        let is_completed = force_completed
            || snap.is_complete
            || snap.elapsed_seconds >= self.target_seconds;

        let completion_state = if is_completed {
            CompletionState::Completed
        } else {
            CompletionState::Interrupted
        };

        let elapsed_seconds = if is_completed {
            if force_completed && snap.elapsed_seconds > 0 && !snap.is_complete {
                snap.elapsed_seconds
            } else {
                self.target_seconds
            }
        } else {
            snap.elapsed_seconds
        };

        let end_utc = self.completed_at_utc.unwrap_or(now_utc);
        let start_utc = self
            .session_started_at_utc
            .unwrap_or_else(|| end_utc - ChronoDuration::seconds(elapsed_seconds as i64));

        let record = if is_completed || elapsed_seconds >= MIN_RECORDABLE_INTERRUPTED_SECS {
            let slices = split_across_midnights(
                start_utc,
                end_utc,
                elapsed_seconds,
                fixed_offset_minutes,
            );
            let primary_date_key = slices
                .first()
                .map(|s| s.date_key.clone())
                .unwrap_or_else(|| to_local_date_key(start_utc, fixed_offset_minutes));

            let reading_meta = if self.activity_kind == ActivityKind::Reading {
                let pages_read = match (self.start_page, self.end_page) {
                    (Some(s), Some(e)) if e >= s => e - s,
                    _ => 0,
                };
                Some(ReadingSessionMeta {
                    material_id: self.reading_material_id.clone(),
                    title: if self.reading_title.trim().is_empty() {
                        "Independent Reading".to_string()
                    } else {
                        self.reading_title.trim().to_string()
                    },
                    author: self.reading_author.trim().to_string(),
                    source_kind: self.reading_kind,
                    start_page: self.start_page,
                    end_page: self.end_page,
                    pages_read,
                })
            } else {
                None
            };

            Some(SessionRecord {
                id: self
                    .active_session_id
                    .clone()
                    .unwrap_or_else(|| generate_entity_id("ses")),
                activity_kind: self.activity_kind,
                category_id: self.category_id.clone(),
                cadence: self.cadence,
                completion_state,
                started_at_utc: start_utc,
                ended_at_utc: end_utc,
                primary_date_key,
                configured_seconds: self.target_seconds,
                elapsed_seconds,
                day_slices: slices,
                objective: self.objective.trim().to_string(),
                completion_notes: self.completion_notes.trim().to_string(),
                planned_block_id: self.planned_block_id.clone(),
                reading_meta,
                reassigned_from_category: None,
            })
        } else {
            None
        };

        // Advance cadence phase
        let (focus_m, short_m, long_m, intervals) =
            self.cadence.schedule_minutes(&prefs.focus);
        let mut completed_full_cycle = false;

        match self.activity_kind {
            ActivityKind::Focus => {
                if is_completed {
                    self.completed_focus_intervals =
                        self.completed_focus_intervals.saturating_add(1);
                    if self.completed_focus_intervals >= intervals {
                        self.activity_kind = ActivityKind::LongBreak;
                        self.configured_seconds = (long_m as u64) * 60;
                        completed_full_cycle = true;
                    } else {
                        self.activity_kind = ActivityKind::ShortBreak;
                        self.configured_seconds = (short_m as u64) * 60;
                    }
                } else {
                    self.configured_seconds = (focus_m as u64) * 60;
                }
            }
            ActivityKind::ShortBreak | ActivityKind::LongBreak => {
                if self.activity_kind == ActivityKind::LongBreak {
                    self.completed_focus_intervals = 0;
                }
                self.activity_kind = ActivityKind::Focus;
                self.configured_seconds = (focus_m as u64) * 60;
            }
            ActivityKind::Reading => {
                self.configured_seconds =
                    (prefs.reading.default_duration_minutes as u64) * 60;
                if let Some(end_p) = self.end_page {
                    self.start_page = Some(end_p);
                    self.end_page = None;
                }
            }
        }

        self.target_seconds = self.configured_seconds;
        self.completion_notes.clear();
        self.planned_block_id = None;
        self.reset_phase();

        // Optional auto-transition into break or next focus
        if is_completed {
            let auto_start = match self.activity_kind {
                ActivityKind::ShortBreak | ActivityKind::LongBreak => {
                    prefs.focus.auto_start_breaks
                }
                ActivityKind::Focus => prefs.focus.auto_start_focus,
                ActivityKind::Reading => false,
            };
            if auto_start {
                self.start_or_resume(now_utc);
            }
        }

        (record, completed_full_cycle)
    }

    /// Recovers a persisted `ClockEngine` after an application restart or OS crash.
    /// Ensures no duplicate elapsed time is created.
    pub fn recover_after_restart(&mut self, now_utc: DateTime<Utc>) -> bool {
        match self.status {
            ClockStatus::Idle => false,
            ClockStatus::Running => {
                let snap = self.snapshot(now_utc);
                if snap.is_complete {
                    self.mark_awaiting_completion(now_utc);
                } else {
                    // Pause safely at the recovered wall-clock elapsed time so the user can review and resume
                    self.status = ClockStatus::Paused;
                    self.accumulated_elapsed_seconds = snap.elapsed_seconds;
                    self.segment_started_at_utc = None;
                }
                true
            }
            ClockStatus::Paused | ClockStatus::AwaitingCompletion => true,
        }
    }
}

/// Splits a session interval `[start_utc, end_utc]` across local midnight boundaries.
/// Guarantees that the sum of returned `DaySlice::seconds` equals `total_elapsed_seconds` exactly.
pub fn split_across_midnights(
    start_utc: DateTime<Utc>,
    end_utc: DateTime<Utc>,
    total_elapsed_seconds: u64,
    fixed_offset_minutes: Option<i32>,
) -> Vec<DaySlice> {
    if total_elapsed_seconds == 0 {
        return Vec::new();
    }

    let safe_end = if end_utc <= start_utc {
        start_utc + ChronoDuration::seconds(total_elapsed_seconds as i64)
    } else {
        end_utc
    };

    let start_date_key = to_local_date_key(start_utc, fixed_offset_minutes);
    let end_probe = safe_end - ChronoDuration::milliseconds(1);
    let end_date_key = to_local_date_key(end_probe.max(start_utc), fixed_offset_minutes);

    if start_date_key == end_date_key {
        return vec![DaySlice {
            date_key: start_date_key,
            seconds: total_elapsed_seconds,
        }];
    }

    let Some(mut cursor_date) = parse_date_key(&start_date_key) else {
        return vec![DaySlice {
            date_key: start_date_key,
            seconds: total_elapsed_seconds,
        }];
    };
    let Some(end_date) = parse_date_key(&end_date_key) else {
        return vec![DaySlice {
            date_key: start_date_key,
            seconds: total_elapsed_seconds,
        }];
    };

    let mut wall_spans: Vec<(String, i64)> = Vec::new();
    let mut guard = 0;

    while cursor_date <= end_date && guard < 14 {
        guard += 1;
        let next_date = cursor_date + ChronoDuration::days(1);
        let day_start_utc = start_of_local_date_utc(cursor_date, fixed_offset_minutes);
        let day_end_utc = start_of_local_date_utc(next_date, fixed_offset_minutes);

        let overlap_start = start_utc.max(day_start_utc);
        let overlap_end = safe_end.min(day_end_utc);
        let wall_ms = (overlap_end - overlap_start).num_milliseconds();
        if wall_ms > 0 {
            wall_spans.push((cursor_date.format("%Y-%m-%d").to_string(), wall_ms));
        }
        cursor_date = next_date;
    }

    let total_wall_ms: i64 = wall_spans.iter().map(|(_, ms)| *ms).sum();
    if total_wall_ms <= 0 || wall_spans.is_empty() {
        return vec![DaySlice {
            date_key: start_date_key,
            seconds: total_elapsed_seconds,
        }];
    }

    let mut allocated: u64 = 0;
    let mut slices = Vec::with_capacity(wall_spans.len());
    let span_count = wall_spans.len();

    for (idx, (d_key, ms)) in wall_spans.into_iter().enumerate() {
        let secs = if idx + 1 == span_count {
            total_elapsed_seconds.saturating_sub(allocated)
        } else {
            let ratio = (ms as f64) / (total_wall_ms as f64);
            let share = ((total_elapsed_seconds as f64) * ratio).round() as u64;
            let clamped = share.min(total_elapsed_seconds.saturating_sub(allocated));
            allocated += clamped;
            clamped
        };
        if secs > 0 {
            slices.push(DaySlice {
                date_key: d_key,
                seconds: secs,
            });
        }
    }

    slices
}

/// Ensures a candidate productive session does not double-count overlapping wall-clock time
/// with existing productive sessions. Trims or rejects overlapping intervals cleanly.
pub fn resolve_session_overlap(
    existing: &[SessionRecord],
    mut candidate: SessionRecord,
    fixed_offset_minutes: Option<i32>,
) -> Option<SessionRecord> {
    if !candidate.activity_kind.is_productive() {
        return Some(candidate);
    }

    let mut eff_start = candidate.started_at_utc;
    let mut eff_end = candidate.ended_at_utc;
    if eff_end <= eff_start {
        return None;
    }

    for prev in existing {
        if prev.id == candidate.id || !prev.activity_kind.is_productive() {
            continue;
        }
        let p_start = prev.started_at_utc;
        let p_end = prev.ended_at_utc;

        if eff_start < p_end && eff_end > p_start {
            // Candidate completely covered by existing session
            if eff_start >= p_start && eff_end <= p_end {
                return None;
            }
            if eff_start < p_start && eff_end <= p_end {
                eff_end = p_start;
            } else if eff_start >= p_start && eff_end > p_end {
                eff_start = p_end;
            } else {
                // Existing session is strictly inside candidate window
                let overlap_secs = (p_end - p_start).num_seconds().max(0) as u64;
                candidate.elapsed_seconds =
                    candidate.elapsed_seconds.saturating_sub(overlap_secs);
            }
        }
    }

    let max_wall_secs = (eff_end - eff_start).num_seconds().max(0) as u64;
    let final_elapsed = candidate.elapsed_seconds.min(max_wall_secs);
    if final_elapsed == 0 {
        return None;
    }

    let slices = split_across_midnights(eff_start, eff_end, final_elapsed, fixed_offset_minutes);
    candidate.started_at_utc = eff_start;
    candidate.ended_at_utc = eff_end;
    candidate.elapsed_seconds = final_elapsed;
    if let Some(first) = slices.first() {
        candidate.primary_date_key = first.date_key.clone();
    }
    candidate.day_slices = slices;

    Some(candidate)
}

//! Chronos Dedicated Reading Sanctuary & Session Ledger Domain (`sanctuary.rs`).
//!
//! Manages:
//! - Reading Sanctuary shelf items (`ReadingMaterial`: Books, Academic Papers, Manuals, Articles)
//! - Page checkpoints, pages-per-hour velocity, and reading status (`Active`, `Completed`, `Paused`)
//! - Session recording, editing, deletion, and automatic reconciliation of reading metrics.

use crate::domain::clock::{resolve_session_overlap, split_across_midnights};
use crate::domain::models::{
    generate_entity_id, to_local_date_key, ActivityKind, CompletionState, MaterialKind,
    ReadingSessionMeta, SessionRecord,
};
use crate::domain::planner::{PlanStatus, PlannedBlock};
use crate::error::{ChronosError, ChronosResult};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde::{Deserialize, Serialize};

/// Where a book is in your life. Old save files used active/completed/paused; those still load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingStatus {
    /// Owned or wishlisted, not started yet.
    #[serde(alias = "paused")]
    Have,
    /// Currently being read.
    #[serde(alias = "active")]
    Reading,
    /// Finished.
    #[serde(alias = "completed")]
    Read,
    /// Started, then dropped.
    Abandoned,
}

impl ReadingStatus {
    pub const ALL: [ReadingStatus; 4] = [ReadingStatus::Have, ReadingStatus::Reading, ReadingStatus::Read, ReadingStatus::Abandoned];

    pub fn label(self) -> &'static str {
        match self {
            ReadingStatus::Have => "Have",
            ReadingStatus::Reading => "Reading",
            ReadingStatus::Read => "Read",
            ReadingStatus::Abandoned => "Abandoned",
        }
    }

    /// Cycle order used by the shelf's `S` key: Have → Reading → Read → Abandoned → Have.
    pub fn next(self) -> Self {
        match self {
            ReadingStatus::Have => ReadingStatus::Reading,
            ReadingStatus::Reading => ReadingStatus::Read,
            ReadingStatus::Read => ReadingStatus::Abandoned,
            ReadingStatus::Abandoned => ReadingStatus::Have,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingMaterial {
    pub id: String,
    pub title: String,
    pub author: String,
    pub kind: MaterialKind,
    pub category_id: String,
    pub total_pages: u32,
    pub current_page: u32,
    pub total_reading_seconds: u64,
    /// Pages actually read across recorded sessions (drives velocity); derived by reconciliation.
    #[serde(default)]
    pub total_pages_read: u32,
    pub completed_sessions_count: u32,
    pub status: ReadingStatus,
    pub notes: String,
    pub created_at_utc: DateTime<Utc>,
    pub last_read_at_utc: Option<DateTime<Utc>>,
}

impl ReadingMaterial {
    pub fn progress_percent(&self) -> u8 {
        if self.total_pages == 0 {
            0
        } else {
            ((self.current_page as u64 * 100) / (self.total_pages as u64)).min(100) as u8
        }
    }

    pub fn pages_per_hour(&self) -> f64 {
        if self.total_reading_seconds < 60 || self.total_pages_read == 0 {
            0.0
        } else {
            let hours = (self.total_reading_seconds as f64) / 3600.0;
            ((self.total_pages_read as f64) / hours * 10.0).round() / 10.0
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReadingMaterialDraft {
    pub title: String,
    pub author: String,
    pub kind: MaterialKind,
    pub category_id: String,
    pub total_pages: u32,
    pub current_page: u32,
    pub notes: String,
}

#[derive(Debug, Clone)]
pub struct SessionEditPatch {
    pub category_id: String,
    pub objective: String,
    pub completion_notes: String,
    pub elapsed_seconds: u64,
    pub completion_state: CompletionState,
    pub reading_title: Option<String>,
    pub reading_author: Option<String>,
    pub pages_read: Option<u32>,
}

pub fn default_reading_shelf(now_utc: DateTime<Utc>) -> Vec<ReadingMaterial> {
    vec![
        ReadingMaterial {
            id: "mat_ddia".to_string(),
            title: "Designing Data-Intensive Applications".to_string(),
            author: "Martin Kleppmann".to_string(),
            kind: MaterialKind::Book,
            category_id: "cat_tech_reading".to_string(),
            total_pages: 616,
            current_page: 210,
            total_reading_seconds: 0,
            total_pages_read: 0,
            completed_sessions_count: 0,
            status: ReadingStatus::Reading,
            notes: "Consensus, linearizability, and distributed storage engines".to_string(),
            created_at_utc: now_utc,
            last_read_at_utc: None,
        },
        ReadingMaterial {
            id: "mat_ous_design".to_string(),
            title: "A Philosophy of Software Design".to_string(),
            author: "John Ousterhout".to_string(),
            kind: MaterialKind::Book,
            category_id: "cat_tech_reading".to_string(),
            total_pages: 190,
            current_page: 95,
            total_reading_seconds: 0,
            total_pages_read: 0,
            completed_sessions_count: 0,
            status: ReadingStatus::Reading,
            notes: "Deep modules vs tactical complexity".to_string(),
            created_at_utc: now_utc,
            last_read_at_utc: None,
        },
    ]
}

pub fn add_reading_material(
    shelf: &mut Vec<ReadingMaterial>,
    draft: ReadingMaterialDraft,
    now_utc: DateTime<Utc>,
) -> ChronosResult<ReadingMaterial> {
    let title = draft.title.trim();
    if title.is_empty() {
        return Err(ChronosError::Validation(
            "Reading material title cannot be empty.".to_string(),
        ));
    }

    let total_pages = draft.total_pages.min(20_000);
    let current_page = if total_pages > 0 {
        draft.current_page.min(total_pages)
    } else {
        draft.current_page
    };
    let status = if total_pages > 0 && current_page >= total_pages {
        ReadingStatus::Read
    } else if current_page > 0 {
        ReadingStatus::Reading
    } else {
        ReadingStatus::Have
    };

    let item = ReadingMaterial {
        id: generate_entity_id("mat"),
        title: title.to_string(),
        author: if draft.author.trim().is_empty() {
            "Independent Source".to_string()
        } else {
            draft.author.trim().to_string()
        },
        kind: draft.kind,
        category_id: draft.category_id,
        total_pages,
        current_page,
        total_reading_seconds: 0,
        total_pages_read: 0,
        completed_sessions_count: 0,
        status,
        notes: draft.notes.trim().to_string(),
        created_at_utc: now_utc,
        last_read_at_utc: None,
    };

    shelf.insert(0, item.clone());
    Ok(item)
}

pub fn update_reading_material(
    shelf: &mut [ReadingMaterial],
    material_id: &str,
    draft: ReadingMaterialDraft,
) -> ChronosResult<ReadingMaterial> {
    let title = draft.title.trim();
    if title.is_empty() {
        return Err(ChronosError::Validation(
            "Reading material title cannot be empty.".to_string(),
        ));
    }

    let item = shelf
        .iter_mut()
        .find(|m| m.id == material_id)
        .ok_or_else(|| {
            ChronosError::Validation(format!("Reading material not found: {}", material_id))
        })?;

    item.title = title.to_string();
    item.author = draft.author.trim().to_string();
    item.kind = draft.kind;
    item.category_id = draft.category_id;
    item.total_pages = draft.total_pages.min(20_000);
    item.current_page = if item.total_pages > 0 {
        draft.current_page.min(item.total_pages)
    } else {
        draft.current_page
    };
    if item.total_pages > 0 && item.current_page >= item.total_pages {
        item.status = ReadingStatus::Read;
    } else if item.status == ReadingStatus::Read {
        item.status = ReadingStatus::Reading;
    }
    item.notes = draft.notes.trim().to_string();

    Ok(item.clone())
}

/// Commits a newly finished or interrupted session into the session ledger,
/// enforcing overlap prevention and updating reading shelf items and planned blocks.
pub fn commit_session_record(
    sessions: &mut Vec<SessionRecord>,
    reading_shelf: &mut Vec<ReadingMaterial>,
    planned_blocks: &mut [PlannedBlock],
    candidate: SessionRecord,
    fixed_offset_minutes: Option<i32>,
) -> Option<SessionRecord> {
    let mut resolved = resolve_session_overlap(sessions, candidate, fixed_offset_minutes)?;

    if resolved.activity_kind == ActivityKind::Reading {
        if let Some(ref mut meta) = resolved.reading_meta {
            let match_idx = reading_shelf.iter().position(|m| {
                meta.material_id.as_deref() == Some(m.id.as_str())
                    || (!meta.title.is_empty()
                        && m.title.trim().eq_ignore_ascii_case(meta.title.trim()))
            });

            if let Some(idx) = match_idx {
                let mat = &mut reading_shelf[idx];
                meta.material_id = Some(mat.id.clone());
                if let Some(end_p) = meta.end_page {
                    if end_p > mat.current_page {
                        mat.current_page = if mat.total_pages > 0 {
                            end_p.min(mat.total_pages)
                        } else {
                            end_p
                        };
                    }
                } else if meta.pages_read > 0 {
                    let next_p = mat.current_page.saturating_add(meta.pages_read);
                    mat.current_page = if mat.total_pages > 0 {
                        next_p.min(mat.total_pages)
                    } else {
                        next_p
                    };
                }
                if matches!(mat.status, ReadingStatus::Have | ReadingStatus::Abandoned) {
                    mat.status = ReadingStatus::Reading;
                }
                if mat.total_pages > 0 && mat.current_page >= mat.total_pages {
                    mat.status = ReadingStatus::Read;
                }
                mat.last_read_at_utc = Some(resolved.ended_at_utc);
            } else if !meta.title.trim().is_empty() && meta.title != "Independent Reading" {
                let new_id = generate_entity_id("mat");
                meta.material_id = Some(new_id.clone());
                reading_shelf.insert(
                    0,
                    ReadingMaterial {
                        id: new_id,
                        title: meta.title.trim().to_string(),
                        author: if meta.author.trim().is_empty() {
                            "Independent Source".to_string()
                        } else {
                            meta.author.trim().to_string()
                        },
                        kind: meta.source_kind,
                        category_id: resolved.category_id.clone(),
                        total_pages: meta.end_page.unwrap_or(250).max(100),
                        current_page: meta.end_page.unwrap_or(meta.pages_read),
                        total_reading_seconds: resolved.elapsed_seconds,
                        total_pages_read: 0,
                        completed_sessions_count: 1,
                        status: ReadingStatus::Reading,
                        notes: resolved.completion_notes.clone(),
                        created_at_utc: resolved.started_at_utc,
                        last_read_at_utc: Some(resolved.ended_at_utc),
                    },
                );
            }
        }
    }

    if resolved.completion_state == CompletionState::Completed {
        if let Some(ref plan_id) = resolved.planned_block_id {
            if let Some(plan) = planned_blocks.iter_mut().find(|p| &p.id == plan_id) {
                plan.status = PlanStatus::Completed;
                plan.linked_session_id = Some(resolved.id.clone());
            }
        }
    }

    sessions.push(resolved.clone());
    sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at_utc));
    reconcile_reading_shelf_totals(reading_shelf, sessions);

    Some(resolved)
}

/// Edits an existing session's metadata and deterministically recalculates day slices and reading totals.
pub fn edit_session_record(
    sessions: &mut [SessionRecord],
    reading_shelf: &mut [ReadingMaterial],
    session_id: &str,
    patch: SessionEditPatch,
    fixed_offset_minutes: Option<i32>,
) -> ChronosResult<SessionRecord> {
    // Lengthening a session must never push it into the next productive session (double counting).
    let (own_start, own_productive) = sessions
        .iter()
        .find(|s| s.id == session_id)
        .map(|s| (s.started_at_utc, s.activity_kind.is_productive()))
        .ok_or_else(|| ChronosError::Validation(format!("Session not found: {}", session_id)))?;
    let requested_elapsed = patch.elapsed_seconds.clamp(60, 4 * 3600);
    let requested_end = own_start + ChronoDuration::seconds(requested_elapsed as i64);
    let next_start = if own_productive {
        sessions
            .iter()
            .filter(|o| {
                o.id != session_id
                    && o.activity_kind.is_productive()
                    && o.started_at_utc > own_start
                    && o.started_at_utc < requested_end
            })
            .map(|o| o.started_at_utc)
            .min()
    } else {
        None
    };
    let session = sessions
        .iter_mut()
        .find(|s| s.id == session_id)
        .ok_or_else(|| ChronosError::Validation(format!("Session not found: {}", session_id)))?;

    let (clamped_elapsed, new_end_utc) = match next_start {
        Some(limit) => {
            let wall = (limit - own_start).num_seconds().max(1) as u64;
            (requested_elapsed.min(wall), limit)
        }
        None => (requested_elapsed, requested_end),
    };
    let slices = split_across_midnights(
        session.started_at_utc,
        new_end_utc,
        clamped_elapsed,
        fixed_offset_minutes,
    );

    session.category_id = patch.category_id;
    session.objective = patch.objective.trim().to_string();
    session.completion_notes = patch.completion_notes.trim().to_string();
    session.elapsed_seconds = clamped_elapsed;
    session.ended_at_utc = new_end_utc;
    session.completion_state = patch.completion_state;
    session.primary_date_key = slices
        .first()
        .map(|s| s.date_key.clone())
        .unwrap_or_else(|| to_local_date_key(session.started_at_utc, fixed_offset_minutes));
    session.day_slices = slices;

    if session.activity_kind == ActivityKind::Reading {
        let mut meta = session.reading_meta.clone().unwrap_or(ReadingSessionMeta {
            material_id: None,
            title: "Independent Reading".to_string(),
            author: String::new(),
            source_kind: MaterialKind::Book,
            start_page: None,
            end_page: None,
            pages_read: 0,
        });
        if let Some(t) = patch.reading_title {
            if !t.trim().is_empty() {
                meta.title = t.trim().to_string();
            }
        }
        if let Some(a) = patch.reading_author {
            meta.author = a.trim().to_string();
        }
        if let Some(p) = patch.pages_read {
            meta.pages_read = p;
        }
        session.reading_meta = Some(meta);
    }

    let updated = session.clone();
    reconcile_reading_shelf_totals(reading_shelf, sessions);
    Ok(updated)
}

/// Deletes an individual session by ID and recalculates reading shelf totals and planned block links.
pub fn delete_session_record(
    sessions: &mut Vec<SessionRecord>,
    reading_shelf: &mut [ReadingMaterial],
    planned_blocks: &mut [PlannedBlock],
    session_id: &str,
) -> ChronosResult<SessionRecord> {
    let idx = sessions
        .iter()
        .position(|s| s.id == session_id)
        .ok_or_else(|| ChronosError::Validation(format!("Session not found: {}", session_id)))?;

    let removed = sessions.remove(idx);

    for plan in planned_blocks.iter_mut() {
        if plan.linked_session_id.as_deref() == Some(session_id) {
            plan.status = PlanStatus::Pending;
            plan.linked_session_id = None;
        }
    }

    reconcile_reading_shelf_totals(reading_shelf, sessions);
    Ok(removed)
}

/// Deterministically recomputes `total_reading_seconds` and `completed_sessions_count` for each reading item.
pub fn reconcile_reading_shelf_totals(
    reading_shelf: &mut [ReadingMaterial],
    sessions: &[SessionRecord],
) {
    for item in reading_shelf.iter_mut() {
        let mut total_secs = 0u64;
        let mut total_pages = 0u32;
        let mut count = 0u32;
        let mut latest_utc: Option<DateTime<Utc>> = None;

        for s in sessions {
            if s.activity_kind != ActivityKind::Reading {
                continue;
            }
            if let Some(ref meta) = s.reading_meta {
                let matches = meta.material_id.as_deref() == Some(item.id.as_str())
                    || (!meta.title.is_empty()
                        && meta.title.trim().eq_ignore_ascii_case(item.title.trim()));
                if matches {
                    total_secs = total_secs.saturating_add(s.elapsed_seconds);
                    count = count.saturating_add(1);
                    total_pages = total_pages.saturating_add(meta.pages_read);
                    if latest_utc.is_none_or(|prev| s.ended_at_utc > prev) {
                        latest_utc = Some(s.ended_at_utc);
                    }
                }
            }
        }

        item.total_reading_seconds = total_secs;
        item.total_pages_read = total_pages;
        item.completed_sessions_count = count;
        item.last_read_at_utc = latest_utc;
    }
}

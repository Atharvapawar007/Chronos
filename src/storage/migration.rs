//! Versioned Data Migration Engine (`migration.rs`).
//!
//! Upgrades earlier Chronos vault schemas (`v1` and `v2`) deterministically to `v3`:
//! - Repairs missing or empty `day_slices` using `split_across_midnights`.
//! - Ensures all 5 default categories exist unless customized, and maps orphaned category IDs safely.
//! - Reconciles Reading Sanctuary shelf items against recorded reading sessions.
//! - Silently seeds achievement unlocks earned by historical sessions so upgrading never floods notifications.

use crate::domain::{
    default_categories, default_reading_shelf, reconcile_reading_shelf_totals,
    split_across_midnights, to_local_date_key, ActivityKind, CadencePreset, CategoryRecord,
    CategoryScope, ClockEngine, CompletionState, MaterialKind, PlannedBlock, Preferences,
    ProgressionLedger, ReadingMaterial, ReadingSessionMeta, SessionRecord, APP_VERSION,
    VAULT_SCHEMA_VERSION,
};
use crate::error::{ChronosError, ChronosResult};
use crate::storage::repository::ChronosVault;
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use serde_json::Value;
use std::collections::HashSet;

pub fn migrate_raw_vault_json(
    raw_json: &str,
    prefs: &Preferences,
    now_utc: DateTime<Utc>,
    fixed_offset_minutes: Option<i32>,
) -> ChronosResult<(ChronosVault, Option<u32>)> {
    let root: Value = serde_json::from_str(raw_json)?;
    let obj = root
        .as_object()
        .ok_or_else(|| ChronosError::Validation("Vault root must be a JSON object.".to_string()))?;

    let found_version = obj
        .get("schema_version")
        .or_else(|| obj.get("version"))
        .and_then(|v| v.as_u64())
        .unwrap_or(1) as u32;

    if found_version > VAULT_SCHEMA_VERSION {
        return Err(ChronosError::SchemaMismatch {
            found: found_version,
            supported: VAULT_SCHEMA_VERSION,
        });
    }

    // Fast path for native v3 schema, while still reconciling slices and reading totals
    if found_version == VAULT_SCHEMA_VERSION {
        if let Ok(mut vault) = serde_json::from_value::<ChronosVault>(root.clone()) {
            if vault.categories.is_empty() {
                vault.categories = default_categories(now_utc);
            }
            for cat in &mut vault.categories {
                cat.icon_glyph = crate::domain::taxonomy::sanitize_icon_glyph(&cat.icon_glyph);
            }
            for s in &mut vault.sessions {
                if s.day_slices.is_empty() {
                    s.day_slices = split_across_midnights(
                        s.started_at_utc,
                        s.ended_at_utc,
                        s.elapsed_seconds,
                        fixed_offset_minutes,
                    );
                }
            }
            reconcile_reading_shelf_totals(&mut vault.reading_shelf, &vault.sessions);
            vault.recompute_progression(prefs, true, now_utc);
            return Ok((vault, None));
        }
    }

    // Tolerant field-by-field migration from v1 / v2
    let mut categories: Vec<CategoryRecord> = obj
        .get("categories")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    if categories.is_empty() {
        categories = default_categories(now_utc);
    }

    let known_cat_ids: HashSet<String> = categories.iter().map(|c| c.id.clone()).collect();
    let fallback_focus_cat = categories
        .iter()
        .find(|c| c.scope.supports_activity(ActivityKind::Focus))
        .map(|c| c.id.clone())
        .unwrap_or_else(|| "cat_software_eng".to_string());
    let fallback_reading_cat = categories
        .iter()
        .find(|c| c.scope.supports_activity(ActivityKind::Reading))
        .map(|c| c.id.clone())
        .unwrap_or_else(|| "cat_tech_reading".to_string());

    let mut sessions = Vec::new();
    if let Some(raw_sessions) = obj.get("sessions").and_then(|v| v.as_array()) {
        for (idx, item) in raw_sessions.iter().enumerate() {
            let Some(s_obj) = item.as_object() else {
                continue;
            };
            let elapsed_seconds = s_obj
                .get("elapsed_seconds")
                .or_else(|| s_obj.get("duration_seconds"))
                .or_else(|| s_obj.get("duration"))
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            if elapsed_seconds == 0 {
                continue;
            }

            let activity_str = s_obj
                .get("activity_kind")
                .or_else(|| s_obj.get("activity"))
                .or_else(|| s_obj.get("type"))
                .and_then(|v| v.as_str())
                .unwrap_or("focus");
            let activity_kind = match activity_str {
                "reading" => ActivityKind::Reading,
                "short_break" => ActivityKind::ShortBreak,
                "long_break" => ActivityKind::LongBreak,
                _ => ActivityKind::Focus,
            };

            let raw_cat_id = s_obj
                .get("category_id")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let category_id = if known_cat_ids.contains(raw_cat_id) {
                raw_cat_id.to_string()
            } else if activity_kind == ActivityKind::Reading {
                fallback_reading_cat.clone()
            } else {
                fallback_focus_cat.clone()
            };

            let started_at_utc = s_obj
                .get("started_at_utc")
                .or_else(|| s_obj.get("started_at"))
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| now_utc - ChronoDuration::seconds(elapsed_seconds as i64));

            let ended_at_utc = s_obj
                .get("ended_at_utc")
                .or_else(|| s_obj.get("ended_at"))
                .and_then(|v| v.as_str())
                .and_then(|s| DateTime::parse_from_rfc3339(s).ok())
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_else(|| started_at_utc + ChronoDuration::seconds(elapsed_seconds as i64));

            let slices = split_across_midnights(
                started_at_utc,
                ended_at_utc,
                elapsed_seconds,
                fixed_offset_minutes,
            );
            let primary_date_key = slices
                .first()
                .map(|sl| sl.date_key.clone())
                .unwrap_or_else(|| to_local_date_key(started_at_utc, fixed_offset_minutes));

            let completion_state = match s_obj
                .get("completion_state")
                .or_else(|| s_obj.get("status"))
                .and_then(|v| v.as_str())
            {
                Some("interrupted") => CompletionState::Interrupted,
                _ => CompletionState::Completed,
            };

            let objective = s_obj
                .get("objective")
                .or_else(|| s_obj.get("note"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let completion_notes = s_obj
                .get("completion_notes")
                .or_else(|| s_obj.get("summary"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let reading_meta = if activity_kind == ActivityKind::Reading {
                if let Some(rm) = s_obj.get("reading_meta") {
                    serde_json::from_value::<ReadingSessionMeta>(rm.clone()).ok()
                } else {
                    let title = s_obj
                        .get("book_title")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Independent Reading")
                        .to_string();
                    let pages = s_obj
                        .get("pages_read")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0) as u32;
                    Some(ReadingSessionMeta {
                        material_id: None,
                        title,
                        author: String::new(),
                        source_kind: MaterialKind::Book,
                        start_page: None,
                        end_page: None,
                        pages_read: pages,
                    })
                }
            } else {
                None
            };

            sessions.push(SessionRecord {
                id: s_obj
                    .get("id")
                    .and_then(|v| v.as_str())
                    .map(String::from)
                    .unwrap_or_else(|| format!("ses_mig_{}", idx)),
                activity_kind,
                category_id,
                cadence: CadencePreset::Harmonic,
                completion_state,
                started_at_utc,
                ended_at_utc,
                primary_date_key,
                configured_seconds: s_obj
                    .get("configured_seconds")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(elapsed_seconds),
                elapsed_seconds,
                day_slices: slices,
                objective,
                completion_notes,
                planned_block_id: None,
                reading_meta,
                reassigned_from_category: None,
            });
        }
    }

    sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at_utc));

    let mut reading_shelf: Vec<ReadingMaterial> = obj
        .get("reading_shelf")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_else(|| default_reading_shelf(now_utc));
    reconcile_reading_shelf_totals(&mut reading_shelf, &sessions);

    let planned_blocks: Vec<PlannedBlock> = obj
        .get("planned_blocks")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let progression: ProgressionLedger = obj
        .get("progression")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let active_clock: Option<ClockEngine> = obj
        .get("active_clock")
        .and_then(|v| serde_json::from_value(v.clone()).ok());

    let _ = CategoryScope::Universal; // Ensure import used

    let mut vault = ChronosVault {
        schema_version: VAULT_SCHEMA_VERSION,
        app_version: APP_VERSION.to_string(),
        created_at_utc: now_utc,
        updated_at_utc: now_utc,
        categories,
        sessions,
        reading_shelf,
        planned_blocks,
        progression,
        active_clock,
    };

    vault.recompute_progression(prefs, true, now_utc);
    let upgraded_from = if found_version < VAULT_SCHEMA_VERSION {
        Some(found_version)
    } else {
        None
    };

    Ok((vault, upgraded_from))
}

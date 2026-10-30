//! Versioned Backup, Validation & Conflict-Aware Restore Engine (`backup.rs`).

use crate::domain::{
    reconcile_reading_shelf_totals, CategoryRecord, PlannedBlock, Preferences, ReadingMaterial,
    SessionRecord, APP_VERSION, VAULT_SCHEMA_VERSION,
};
use crate::error::{ChronosError, ChronosResult};
use crate::storage::migration::migrate_raw_vault_json;
use crate::storage::repository::{save_vault, write_atomically, ChronosPaths, ChronosVault};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RestoreStrategy {
    MergeKeepExisting,
    MergePreferImported,
    ReplaceAll,
}

impl RestoreStrategy {
    pub const ALL: [RestoreStrategy; 3] = [
        RestoreStrategy::MergeKeepExisting,
        RestoreStrategy::MergePreferImported,
        RestoreStrategy::ReplaceAll,
    ];

    pub fn label(self) -> &'static str {
        match self {
            RestoreStrategy::MergeKeepExisting => {
                "Merge (Retain current records when IDs conflict)"
            }
            RestoreStrategy::MergePreferImported => {
                "Merge (Overwrite conflicting IDs with backup records)"
            }
            RestoreStrategy::ReplaceAll => {
                "Replace Entire Vault (Pre-restore backup created automatically)"
            }
        }
    }

    pub fn next(self) -> Self {
        match self {
            RestoreStrategy::MergeKeepExisting => RestoreStrategy::MergePreferImported,
            RestoreStrategy::MergePreferImported => RestoreStrategy::ReplaceAll,
            RestoreStrategy::ReplaceAll => RestoreStrategy::MergeKeepExisting,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupEnvelope {
    pub backup_format: String,
    pub schema_version: u32,
    pub app_version: String,
    pub created_at_utc: DateTime<Utc>,
    pub reason: String,
    pub vault: ChronosVault,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackupConflictPreview {
    pub source_schema_version: u32,
    pub incoming_sessions: usize,
    pub new_sessions: usize,
    pub conflicting_sessions: usize,
    pub incoming_categories: usize,
    pub conflicting_categories: usize,
    pub incoming_reading_items: usize,
    pub incoming_achievements: usize,
}

/// Creates a full versioned backup file in `paths.backups_dir` (or a custom destination path)
/// and verifies that the written file is non-empty on disk.
pub fn create_verified_backup(
    paths: &ChronosPaths,
    vault: &ChronosVault,
    reason: &str,
    custom_dest: Option<&Path>,
    now_utc: DateTime<Utc>,
) -> ChronosResult<PathBuf> {
    let dest_path = if let Some(p) = custom_dest {
        p.to_path_buf()
    } else {
        fs::create_dir_all(&paths.backups_dir)?;
        let stamp = now_utc.format("%Y%m%d-%H%M%S");
        paths
            .backups_dir
            .join(format!("chronos-backup-{}-{}.json", reason, stamp))
    };

    let envelope = BackupEnvelope {
        backup_format: "chronos-backup-v3".to_string(),
        schema_version: VAULT_SCHEMA_VERSION,
        app_version: APP_VERSION.to_string(),
        created_at_utc: now_utc,
        reason: reason.to_string(),
        vault: vault.clone(),
    };

    let bytes = serde_json::to_vec_pretty(&envelope)?;
    write_atomically(&dest_path, &bytes)?;

    let meta = fs::metadata(&dest_path)?;
    if meta.len() < 32 {
        return Err(ChronosError::DataSafety(
            "Backup verification failed: written backup file was unexpectedly small.".to_string(),
        ));
    }

    Ok(dest_path)
}

/// Lists available `.json` backup files in `paths.backups_dir`, newest first.
pub fn list_available_backups(paths: &ChronosPaths) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(&paths.backups_dir) else {
        return Vec::new();
    };
    let mut files: Vec<PathBuf> = entries
        .filter_map(|e| e.ok().map(|de| de.path()))
        .filter(|p| p.extension().and_then(|s| s.to_str()) == Some("json"))
        .collect();
    files.sort_by(|a, b| b.file_name().cmp(&a.file_name()));
    files
}

/// Validates a raw backup JSON string (either wrapped in `BackupEnvelope` or a raw `ChronosVault`)
/// and returns `(ChronosVault, BackupConflictPreview)`.
pub fn inspect_and_validate_backup(
    raw_json: &str,
    current_vault: &ChronosVault,
    prefs: &Preferences,
    now_utc: DateTime<Utc>,
) -> ChronosResult<(ChronosVault, BackupConflictPreview)> {
    let root: Value = serde_json::from_str(raw_json)?;
    let obj = root
        .as_object()
        .ok_or_else(|| ChronosError::Validation("Backup file must be a JSON object.".to_string()))?;

    let inner_vault_str = if let Some(vault_val) = obj.get("vault") {
        serde_json::to_string(vault_val)?
    } else if obj.contains_key("sessions") || obj.contains_key("categories") {
        raw_json.to_string()
    } else {
        return Err(ChronosError::Validation(
            "Incompatible backup file: missing Chronos 'vault' or 'sessions' structure.".to_string(),
        ));
    };

    let (imported_vault, upgraded_from) =
        migrate_raw_vault_json(&inner_vault_str, prefs, now_utc, None)?;

    let existing_session_ids: HashSet<&str> =
        current_vault.sessions.iter().map(|s| s.id.as_str()).collect();
    let conflicting_sessions = imported_vault
        .sessions
        .iter()
        .filter(|s| existing_session_ids.contains(s.id.as_str()))
        .count();
    let incoming_sessions = imported_vault.sessions.len();
    let new_sessions = incoming_sessions.saturating_sub(conflicting_sessions);

    let existing_cat_ids: HashSet<&str> = current_vault
        .categories
        .iter()
        .map(|c| c.id.as_str())
        .collect();
    let conflicting_categories = imported_vault
        .categories
        .iter()
        .filter(|c| existing_cat_ids.contains(c.id.as_str()))
        .count();

    let preview = BackupConflictPreview {
        source_schema_version: upgraded_from.unwrap_or(imported_vault.schema_version),
        incoming_sessions,
        new_sessions,
        conflicting_sessions,
        incoming_categories: imported_vault.categories.len(),
        conflicting_categories,
        incoming_reading_items: imported_vault.reading_shelf.len(),
        incoming_achievements: imported_vault.progression.unlocked.len(),
    };

    Ok((imported_vault, preview))
}

/// Restores a validated backup into `current_vault` using the chosen `RestoreStrategy`.
/// Automatically writes a pre-restore safety snapshot before modifying disk state.
pub fn apply_backup_restore(
    paths: &ChronosPaths,
    current_vault: &mut ChronosVault,
    raw_backup_json: &str,
    strategy: RestoreStrategy,
    prefs: &Preferences,
    now_utc: DateTime<Utc>,
) -> ChronosResult<BackupConflictPreview> {
    let (imported, preview) =
        inspect_and_validate_backup(raw_backup_json, current_vault, prefs, now_utc)?;

    // Always create a verified pre-restore snapshot before mutating user data
    let _ = create_verified_backup(paths, current_vault, "pre-restore", None, now_utc)?;

    if strategy == RestoreStrategy::ReplaceAll {
        let mut replaced = imported;
        replaced.recompute_progression(prefs, true, now_utc);
        save_vault(paths, &replaced)?;
        *current_vault = replaced;
        return Ok(preview);
    }

    let prefer_imported = strategy == RestoreStrategy::MergePreferImported;

    // Merge Categories
    let mut cat_map: BTreeMap<String, CategoryRecord> = current_vault
        .categories
        .iter()
        .map(|c| (c.id.clone(), c.clone()))
        .collect();
    for imp_cat in imported.categories {
        if !cat_map.contains_key(&imp_cat.id) || prefer_imported {
            cat_map.insert(imp_cat.id.clone(), imp_cat);
        }
    }
    current_vault.categories = cat_map.into_values().collect();

    // Merge Sessions
    let mut ses_map: BTreeMap<String, SessionRecord> = current_vault
        .sessions
        .iter()
        .map(|s| (s.id.clone(), s.clone()))
        .collect();
    for imp_ses in imported.sessions {
        if !ses_map.contains_key(&imp_ses.id) || prefer_imported {
            ses_map.insert(imp_ses.id.clone(), imp_ses);
        }
    }
    let mut merged_sessions: Vec<SessionRecord> = ses_map.into_values().collect();
    merged_sessions.sort_by_key(|s| std::cmp::Reverse(s.started_at_utc));
    current_vault.sessions = merged_sessions;

    // Merge Reading Shelf
    let mut mat_map: BTreeMap<String, ReadingMaterial> = current_vault
        .reading_shelf
        .iter()
        .map(|m| (m.id.clone(), m.clone()))
        .collect();
    for imp_mat in imported.reading_shelf {
        if !mat_map.contains_key(&imp_mat.id) || prefer_imported {
            mat_map.insert(imp_mat.id.clone(), imp_mat);
        }
    }
    let mut merged_shelf: Vec<ReadingMaterial> = mat_map.into_values().collect();
    reconcile_reading_shelf_totals(&mut merged_shelf, &current_vault.sessions);
    current_vault.reading_shelf = merged_shelf;

    // Merge Planned Blocks
    let mut plan_map: BTreeMap<String, PlannedBlock> = current_vault
        .planned_blocks
        .iter()
        .map(|p| (p.id.clone(), p.clone()))
        .collect();
    for imp_plan in imported.planned_blocks {
        if !plan_map.contains_key(&imp_plan.id) || prefer_imported {
            plan_map.insert(imp_plan.id.clone(), imp_plan);
        }
    }
    current_vault.planned_blocks = plan_map.into_values().collect();

    // Merge Achievement Unlocks (keep earliest unlock timestamp)
    for (ach_id, imp_rec) in imported.progression.unlocked {
        match current_vault.progression.unlocked.get(&ach_id) {
            None => {
                current_vault.progression.unlocked.insert(ach_id, imp_rec);
            }
            Some(existing) if imp_rec.unlocked_at_utc < existing.unlocked_at_utc => {
                current_vault.progression.unlocked.insert(ach_id, imp_rec);
            }
            _ => {}
        }
    }

    current_vault.recompute_progression(prefs, true, now_utc);
    save_vault(paths, current_vault)?;

    Ok(preview)
}

//! Atomic file persistence for `ChronosVault`, `Preferences`, and status-bar telemetry.

use crate::domain::{
    default_categories, default_reading_shelf, evaluate_goals, evaluate_progression,
    to_local_date_key, CategoryRecord, ClockEngine, ClockStatus, PlannedBlock, Preferences,
    ProgressionLedger, ReadingMaterial, SessionRecord, APP_VERSION, VAULT_SCHEMA_VERSION,
};
use crate::error::ChronosResult;
use crate::storage::migration::migrate_raw_vault_json;
use chrono::{DateTime, Utc};
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

/// Authoritative persistent data store for Chronos (`chronos_vault.json`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChronosVault {
    pub schema_version: u32,
    pub app_version: String,
    pub created_at_utc: DateTime<Utc>,
    pub updated_at_utc: DateTime<Utc>,
    pub categories: Vec<CategoryRecord>,
    pub sessions: Vec<SessionRecord>,
    pub reading_shelf: Vec<ReadingMaterial>,
    pub planned_blocks: Vec<PlannedBlock>,
    pub progression: ProgressionLedger,
    pub active_clock: Option<ClockEngine>,
}

impl ChronosVault {
    pub fn new_empty(now_utc: DateTime<Utc>) -> Self {
        Self {
            schema_version: VAULT_SCHEMA_VERSION,
            app_version: APP_VERSION.to_string(),
            created_at_utc: now_utc,
            updated_at_utc: now_utc,
            categories: default_categories(now_utc),
            sessions: Vec::new(),
            reading_shelf: default_reading_shelf(now_utc),
            planned_blocks: Vec::new(),
            progression: ProgressionLedger {
                unlocked: Default::default(),
                total_xp: 0,
                level: 1,
                level_title: "Novice Timekeeper".to_string(),
            },
            active_clock: None,
        }
    }

    pub fn recompute_progression(&mut self, prefs: &Preferences, silent_seed: bool, now_utc: DateTime<Utc>) {
        let ref_date = to_local_date_key(now_utc, None);
        let _ = evaluate_progression(
            &mut self.progression,
            &self.sessions,
            &self.categories,
            &self.planned_blocks,
            prefs,
            &ref_date,
            silent_seed,
            now_utc,
        );
        self.updated_at_utc = now_utc;
    }
}

#[derive(Debug, Clone)]
pub struct ChronosPaths {
    pub config_dir: PathBuf,
    pub data_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub preferences_file: PathBuf,
    pub vault_file: PathBuf,
    pub backups_dir: PathBuf,
    pub exports_dir: PathBuf,
    pub status_bar_file: PathBuf,
}

impl ChronosPaths {
    pub fn resolve() -> Self {
        let proj = ProjectDirs::from("io", "chronos", "chronos");
        let config_dir = proj
            .as_ref()
            .map(|p| p.config_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".chronos/config"));
        let data_dir = proj
            .as_ref()
            .map(|p| p.data_local_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".chronos/data"));
        let cache_dir = proj
            .as_ref()
            .map(|p| p.cache_dir().to_path_buf())
            .unwrap_or_else(|| PathBuf::from(".chronos/cache"));

        let vault_file = env::var("CHRONOS_VAULT_PATH")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| data_dir.join("chronos_vault.json"));

        let preferences_file = env::var("CHRONOS_CONFIG_PATH")
            .ok()
            .filter(|s| !s.trim().is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(|| config_dir.join("preferences.toml"));

        let backups_dir = vault_file
            .parent()
            .unwrap_or(&data_dir)
            .join("backups");
        let exports_dir = vault_file
            .parent()
            .unwrap_or(&data_dir)
            .join("exports");
        let status_bar_file = cache_dir.join("status.json");

        Self {
            config_dir,
            data_dir,
            cache_dir,
            preferences_file,
            vault_file,
            backups_dir,
            exports_dir,
            status_bar_file,
        }
    }
}

/// Writes `bytes` to `target_path` atomically by writing + flushing a temporary sibling file
/// and renaming it into place. Never leaves a truncated file on power loss or crash.
pub fn write_atomically(target_path: &Path, bytes: &[u8]) -> ChronosResult<()> {
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file_name = target_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("chronos.tmp");
    let tmp_path = target_path.with_file_name(format!(".{}.tmp.{}", file_name, std::process::id()));

    {
        let mut f = fs::File::create(&tmp_path)?;
        if let Err(e) = f.write_all(bytes).and_then(|_| f.sync_all()) {
            drop(f);
            let _ = fs::remove_file(&tmp_path);
            return Err(e.into());
        }
    }

    if let Err(e) = fs::rename(&tmp_path, target_path) {
        let _ = fs::remove_file(&tmp_path);
        return Err(e.into());
    }
    Ok(())
}

/// Loads `Preferences` from TOML, creating default `preferences.toml` on first launch,
/// and falling back safely to defaults with a warning if a hand-edited file has syntax errors.
pub fn load_preferences(paths: &ChronosPaths) -> (Preferences, Option<String>) {
    if !paths.preferences_file.exists() {
        let default_prefs = Preferences::default();
        if let Ok(serialized) = toml::to_string_pretty(&default_prefs) {
            let _ = write_atomically(&paths.preferences_file, serialized.as_bytes());
        }
        return (default_prefs, None);
    }

    match fs::read_to_string(&paths.preferences_file) {
        Ok(raw) => match toml::from_str::<Preferences>(&raw) {
            Ok(parsed) => (parsed.sanitized(), None),
            Err(err) => {
                let warn = format!(
                    "Preferences file had invalid syntax ({}); loaded safe defaults without overwriting your file.",
                    err
                );
                (Preferences::default(), Some(warn))
            }
        },
        Err(err) => (
            Preferences::default(),
            Some(format!("Could not read preferences ({}); using defaults.", err)),
        ),
    }
}

/// Saves `Preferences` atomically and preserves a `.bak` copy of the prior configuration.
pub fn save_preferences(paths: &ChronosPaths, prefs: &Preferences) -> ChronosResult<()> {
    let clean = prefs.clone().sanitized();
    let serialized = toml::to_string_pretty(&clean)?;
    if paths.preferences_file.exists() {
        let bak_path = paths.preferences_file.with_extension("toml.bak");
        let _ = fs::copy(&paths.preferences_file, bak_path);
    }
    write_atomically(&paths.preferences_file, serialized.as_bytes())
}

/// Loads `ChronosVault` from disk, automatically migrating v1/v2 schemas, recovering from `.bak`
/// if the primary file was corrupted, and quarantining unreadable files instead of overwriting them.
pub fn load_vault(
    paths: &ChronosPaths,
    prefs: &Preferences,
    now_utc: DateTime<Utc>,
) -> ChronosResult<(ChronosVault, Vec<String>)> {
    let mut notices = Vec::new();
    if !paths.vault_file.exists() {
        let vault = ChronosVault::new_empty(now_utc);
        return Ok((vault, notices));
    }

    let raw = fs::read_to_string(&paths.vault_file)?;
    match migrate_raw_vault_json(&raw, prefs, now_utc, None) {
        Ok((vault, migrated_from)) => {
            if let Some(old_ver) = migrated_from {
                let v_bak = paths
                    .vault_file
                    .with_extension(format!("json.v{}.bak", old_ver));
                let _ = fs::copy(&paths.vault_file, &v_bak);
                notices.push(format!(
                    "Upgraded Chronos vault from schema v{} to v{} (backup saved to {}).",
                    old_ver,
                    VAULT_SCHEMA_VERSION,
                    v_bak.display()
                ));
            }
            let rolling_bak = paths.vault_file.with_extension("json.bak");
            let _ = fs::copy(&paths.vault_file, rolling_bak);
            Ok((vault, notices))
        }
        Err(primary_err) => {
            // Quarantine the corrupt file so user data is never silently destroyed
            let stamp = now_utc.format("%Y%m%d-%H%M%S");
            let corrupt_path = paths
                .vault_file
                .with_extension(format!("json.corrupt-{}", stamp));
            let _ = fs::copy(&paths.vault_file, &corrupt_path);
            notices.push(format!(
                "Primary vault file was unreadable ({}); quarantined copy at {}.",
                primary_err,
                corrupt_path.display()
            ));

            // Attempt recovery from rolling `.bak`
            let rolling_bak = paths.vault_file.with_extension("json.bak");
            if rolling_bak.exists() {
                if let Ok(bak_raw) = fs::read_to_string(&rolling_bak) {
                    if let Ok((recovered, _)) =
                        migrate_raw_vault_json(&bak_raw, prefs, now_utc, None)
                    {
                        notices.push("Recovered vault state from rolling .bak file.".to_string());
                        return Ok((recovered, notices));
                    }
                }
            }

            Ok((ChronosVault::new_empty(now_utc), notices))
        }
    }
}

/// Saves `ChronosVault` atomically to disk.
pub fn save_vault(paths: &ChronosPaths, vault: &ChronosVault) -> ChronosResult<()> {
    let serialized = serde_json::to_vec_pretty(vault)?;
    write_atomically(&paths.vault_file, &serialized)
}

/// Writes live status telemetry to `~/.cache/chronos/status.json` for desktop status bars (Waybar/Polybar).
#[derive(Debug, Serialize)]
struct StatusBarPayload<'a> {
    status: &'static str,
    activity: &'static str,
    category: &'a str,
    remaining_hms: String,
    progress_percent: u8,
    today_focus_seconds: u64,
    today_reading_seconds: u64,
    daily_focus_goal_percent: u16,
    daily_reading_goal_percent: u16,
    level: u32,
    total_xp: u64,
}

pub fn write_status_bar_snapshot(
    paths: &ChronosPaths,
    vault: &ChronosVault,
    clock: &ClockEngine,
    prefs: &Preferences,
    now_utc: DateTime<Utc>,
) {
    if !prefs.notifications.status_bar_export {
        return;
    }
    let snap = clock.snapshot(now_utc);
    let today_key = to_local_date_key(now_utc, None);
    let goals = evaluate_goals(&vault.sessions, &vault.categories, prefs, &today_key);
    let cat_name = vault
        .categories
        .iter()
        .find(|c| c.id == clock.category_id)
        .map(|c| c.name.as_str())
        .unwrap_or("General");

    let payload = StatusBarPayload {
        status: match snap.status {
            ClockStatus::Idle => "idle",
            ClockStatus::Running => "running",
            ClockStatus::Paused => "paused",
            ClockStatus::AwaitingCompletion => "awaiting_completion",
        },
        activity: clock.activity_kind.short_label(),
        category: cat_name,
        remaining_hms: crate::domain::format_duration_hms(snap.remaining_seconds),
        progress_percent: (snap.progress_permille / 10).min(100) as u8,
        today_focus_seconds: goals.daily_focus.done_seconds,
        today_reading_seconds: goals.daily_reading.done_seconds,
        daily_focus_goal_percent: goals.daily_focus.percent,
        daily_reading_goal_percent: goals.daily_reading.percent,
        level: vault.progression.level,
        total_xp: vault.progression.total_xp,
    };

    if let Ok(bytes) = serde_json::to_vec_pretty(&payload) {
        let _ = write_atomically(&paths.status_bar_file, &bytes);
    }
}

pub fn remove_status_bar_snapshot(paths: &ChronosPaths) {
    let _ = fs::remove_file(&paths.status_bar_file);
}

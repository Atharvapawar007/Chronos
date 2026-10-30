//! Safe Reset & Destructive Action Engine (`reset.rs`).
//!
//! Provides 4 distinct operations:
//! 1. `StatisticsOnly` — Resets recorded sessions, streaks, and planned blocks; preserves categories,
//!    unlocked achievements, reading shelf titles, and preferences.
//! 2. `ProgressionOnly` — Resets unlocked achievements, XP, and level back to Level 1; preserves
//!    historical sessions, categories, reading shelf, and preferences.
//! 3. `PreferencesOnly` — Resets `preferences.toml` to factory defaults; preserves all historical
//!    sessions, categories, reading shelf, and progression.
//! 4. `FactoryEverything` — Completely resets the vault and preferences to factory state.
//!
//! Safeguards:
//! - Refuses to run while a timer is running or paused.
//! - Requires typing `RESET MY DATA` verbatim for any operation that erases recorded data.
//! - Creates and verifies a backup in `backups/` BEFORE erasing anything; aborts if backup fails.

use crate::domain::{
    ClockEngine, ClockStatus, Preferences, ProgressionLedger, REQUIRED_RESET_CONFIRMATION,
};
use crate::error::{ChronosError, ChronosResult};
use crate::storage::backup::create_verified_backup;
use crate::storage::repository::{save_preferences, save_vault, ChronosPaths, ChronosVault};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResetOperation {
    StatisticsOnly,
    ProgressionOnly,
    PreferencesOnly,
    FactoryEverything,
}

impl ResetOperation {
    pub const ALL: [ResetOperation; 4] = [
        ResetOperation::StatisticsOnly,
        ResetOperation::ProgressionOnly,
        ResetOperation::PreferencesOnly,
        ResetOperation::FactoryEverything,
    ];

    pub fn title(self) -> &'static str {
        match self {
            ResetOperation::StatisticsOnly => "1. Reset Statistical History",
            ResetOperation::ProgressionOnly => "2. Reset Achievements & Progression",
            ResetOperation::PreferencesOnly => "3. Reset Preferences to Defaults",
            ResetOperation::FactoryEverything => "4. Factory Reset All Application Data",
        }
    }

    pub fn deletes_summary(self) -> &'static str {
        match self {
            ResetOperation::StatisticsOnly => {
                "All recorded focus/reading sessions, daily slices, streaks, and planned blocks"
            }
            ResetOperation::ProgressionOnly => {
                "All unlocked achievement badges, XP points, and progression level (resets to Level 1)"
            }
            ResetOperation::PreferencesOnly => {
                "Custom appearance, cadence durations, daily/weekly goal targets, and audio settings"
            }
            ResetOperation::FactoryEverything => {
                "All session history, custom categories, reading shelf items, achievements, XP, and preferences"
            }
        }
    }

    pub fn preserves_summary(self) -> &'static str {
        match self {
            ResetOperation::StatisticsOnly => {
                "Custom categories, Reading Sanctuary book titles, unlocked achievements, and preferences"
            }
            ResetOperation::ProgressionOnly => {
                "All historical sessions, analytics, custom categories, reading shelf, and preferences"
            }
            ResetOperation::PreferencesOnly => {
                "All recorded sessions, streaks, custom categories, reading shelf, and achievements"
            }
            ResetOperation::FactoryEverything => {
                "Nothing in active storage (a verified pre-reset backup is saved to backups/ first)"
            }
        }
    }

    pub fn requires_typed_phrase(self) -> bool {
        matches!(
            self,
            ResetOperation::StatisticsOnly
                | ResetOperation::ProgressionOnly
                | ResetOperation::FactoryEverything
        )
    }
}

pub fn execute_guarded_reset(
    paths: &ChronosPaths,
    vault: &mut ChronosVault,
    prefs: &mut Preferences,
    active_clock: &mut ClockEngine,
    operation: ResetOperation,
    typed_confirmation: &str,
    now_utc: DateTime<Utc>,
) -> ChronosResult<Option<PathBuf>> {
    if active_clock.status != ClockStatus::Idle {
        return Err(ChronosError::ActiveSessionGuard(
            "Cannot execute a data reset while a timer session is active or paused. Complete or cancel the session first."
                .to_string(),
        ));
    }

    if operation.requires_typed_phrase() && typed_confirmation != REQUIRED_RESET_CONFIRMATION {
        return Err(ChronosError::DataSafety(format!(
            "Confirmation phrase mismatch. Type '{}' exactly to proceed.",
            REQUIRED_RESET_CONFIRMATION
        )));
    }

    // Create and verify pre-reset backup before mutating any state
    let backup_path = if operation != ResetOperation::PreferencesOnly {
        let reason = match operation {
            ResetOperation::StatisticsOnly => "before-reset-statistics",
            ResetOperation::ProgressionOnly => "before-reset-progression",
            ResetOperation::FactoryEverything => "before-reset-everything",
            ResetOperation::PreferencesOnly => "before-reset-prefs",
        };
        Some(create_verified_backup(paths, vault, reason, None, now_utc)?)
    } else {
        None
    };

    match operation {
        ResetOperation::StatisticsOnly => {
            vault.sessions.clear();
            vault.planned_blocks.clear();
            for item in &mut vault.reading_shelf {
                item.total_reading_seconds = 0;
                item.completed_sessions_count = 0;
                item.last_read_at_utc = None;
            }
            vault.active_clock = None;
            vault.recompute_progression(prefs, true, now_utc);
            save_vault(paths, vault)?;
        }
        ResetOperation::ProgressionOnly => {
            vault.progression = ProgressionLedger {
                unlocked: Default::default(),
                total_xp: 0,
                level: 1,
                level_title: "Novice Timekeeper".to_string(),
            };
            vault.updated_at_utc = now_utc;
            save_vault(paths, vault)?;
        }
        ResetOperation::PreferencesOnly => {
            let defaults = Preferences::default();
            save_preferences(paths, &defaults)?;
            *prefs = defaults;
            *active_clock = ClockEngine::new_from_prefs(prefs);
        }
        ResetOperation::FactoryEverything => {
            let fresh_vault = ChronosVault::new_empty(now_utc);
            let fresh_prefs = Preferences::default();
            save_vault(paths, &fresh_vault)?;
            save_preferences(paths, &fresh_prefs)?;
            *vault = fresh_vault;
            *prefs = fresh_prefs;
            *active_clock = ClockEngine::new_from_prefs(prefs);
        }
    }

    Ok(backup_path)
}

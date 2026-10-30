//! Command-Line Interface & Headless Operations (`cli.rs`).
//!
//! Supports non-interactive subcommands alongside the default interactive TUI:
//! - `chronos report [-r today|7d|30d|90d|1y|all] [--json]`
//! - `chronos mastery`
//! - `chronos backup [-o PATH]`
//! - `chronos restore <FILE> [--mode merge|overwrite|replace]`
//! - `chronos export [-f csv|json] [-o PATH]`
//! - `chronos doctor`

use crate::domain::{
    build_observatory_report, evaluate_progression, format_hours_minutes, to_local_date_key,
    HorizonFilter,
};
use crate::error::ChronosResult;
use crate::storage::{
    apply_backup_restore, create_verified_backup, export_sessions_csv_file,
    export_structured_json_file, inspect_and_validate_backup, load_preferences, load_vault,
    ChronosPaths, RestoreStrategy,
};
use chrono::Utc;
use clap::{Parser, Subcommand};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Parser)]
#[command(
    name = "chronos",
    version,
    about = "Chronos — Material-inspired terminal deep-work studio, reading sanctuary, analytics & progression suite"
)]
pub struct CliArgs {
    #[command(subcommand)]
    pub command: Option<CliCommand>,
}

#[derive(Debug, Subcommand)]
pub enum CliCommand {
    /// Display an offline analytics summary report in the terminal
    Report {
        /// Time horizon: today, 7d, 30d, 90d, 1y, or all
        #[arg(short = 'r', long = "range", default_value = "7d")]
        range: String,
        /// Optional category ID filter
        #[arg(short = 'c', long = "category")]
        category: Option<String>,
        /// Output machine-readable JSON
        #[arg(long = "json")]
        json: bool,
    },
    /// Display current level, XP, and all 24 achievements across the 6 families
    Mastery,
    /// Create a verified full backup of the Chronos vault
    Backup {
        /// Optional output path for the backup file
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },
    /// Validate and restore a Chronos backup file with explicit conflict handling
    Restore {
        /// Path to the backup JSON file
        file: PathBuf,
        /// Conflict mode: merge (keep existing), overwrite (prefer imported), or replace
        #[arg(short = 'm', long = "mode", default_value = "merge")]
        mode: String,
    },
    /// Export session history to RFC-4180 CSV or structured JSON
    Export {
        /// Format: csv or json
        #[arg(short = 'f', long = "format", default_value = "csv")]
        format: String,
        /// Optional output file path
        #[arg(short = 'o', long = "output")]
        output: Option<PathBuf>,
    },
    /// Verify vault integrity, schema version, and local storage paths
    Doctor,
}

pub fn execute_cli_command(cmd: CliCommand) -> ChronosResult<()> {
    let now_utc = Utc::now();
    let paths = ChronosPaths::resolve();
    let (prefs, warn_opt) = load_preferences(&paths);
    if let Some(w) = warn_opt {
        eprintln!("Warning: {}", w);
    }
    let (mut vault, notices) = load_vault(&paths, &prefs, now_utc)?;
    for n in notices {
        eprintln!("Notice: {}", n);
    }

    match cmd {
        CliCommand::Report {
            range,
            category,
            json,
        } => {
            let horizon = HorizonFilter::from_key(&range);
            let today_key = to_local_date_key(now_utc, None);
            let rep = build_observatory_report(
                &vault.sessions,
                &vault.categories,
                &prefs,
                horizon,
                category.as_deref(),
                &today_key,
                None,
            );

            if json {
                let payload = serde_json::json!({
                    "horizon": rep.horizon.short_key(),
                    "start_date": rep.start_date_key,
                    "end_date": rep.end_date_key,
                    "total_productive_seconds": rep.total_productive_seconds,
                    "total_focus_seconds": rep.total_focus_seconds,
                    "total_reading_seconds": rep.total_reading_seconds,
                    "total_pages_read": rep.total_pages_read,
                    "completed_sessions": rep.completed_sessions_count,
                    "interrupted_sessions": rep.interrupted_sessions_count,
                    "completion_rate_percent": rep.completion_rate_percent,
                    "period_delta_percent": rep.period_delta_percent,
                    "current_streak_days": rep.streaks.combined.current_days,
                    "best_streak_days": rep.streaks.combined.best_days,
                });
                println!("{}", serde_json::to_string_pretty(&payload)?);
            } else {
                println!(
                    "CHRONOS ANALYTICS REPORT — {} ({} → {})",
                    rep.horizon.label(),
                    rep.start_date_key,
                    rep.end_date_key
                );
                println!("────────────────────────────────────────────────────────────");
                println!(
                    "Total Productive : {}",
                    format_hours_minutes(rep.total_productive_seconds)
                );
                println!(
                    "Focus / Reading  : {} Focus · {} Reading ({} pages)",
                    format_hours_minutes(rep.total_focus_seconds),
                    format_hours_minutes(rep.total_reading_seconds),
                    rep.total_pages_read
                );
                println!(
                    "Sessions         : {} completed · {} interrupted",
                    rep.completed_sessions_count, rep.interrupted_sessions_count
                );
                println!(
                    "Active Streak    : {} day(s) (All-time best: {}d)",
                    rep.streaks.combined.current_days, rep.streaks.combined.best_days
                );
                for ins in &rep.insights {
                    println!("• {}", ins);
                }
            }
        }
        CliCommand::Mastery => {
            let today_key = to_local_date_key(now_utc, None);
            let (lvl, items, _) = evaluate_progression(
                &mut vault.progression,
                &vault.sessions,
                &vault.categories,
                &vault.planned_blocks,
                &prefs,
                &today_key,
                true,
                now_utc,
            );
            println!(
                "CHRONOS PROGRESSION — Level {} · {} ({} XP)",
                lvl.level, lvl.title, lvl.total_xp
            );
            println!("────────────────────────────────────────────────────────────");
            for item in items {
                let mark = if item.is_unlocked { "✓" } else { "·" };
                println!(
                    "[{}] {:<28} ({:<8} +{:>3} XP) {:>3}% — {}",
                    mark,
                    item.spec.title,
                    item.spec.rarity.label(),
                    item.spec.rarity.xp_bonus(),
                    item.progress_percent,
                    item.spec.description
                );
            }
        }
        CliCommand::Backup { output } => {
            let path =
                create_verified_backup(&paths, &vault, "cli", output.as_deref(), now_utc)?;
            println!("Created verified backup at {}", path.display());
        }
        CliCommand::Restore { file, mode } => {
            let raw = fs::read_to_string(&file)?;
            let strategy = match mode.trim().to_lowercase().as_str() {
                "merge" | "keep_existing" => RestoreStrategy::MergeKeepExisting,
                "overwrite" | "prefer_imported" => RestoreStrategy::MergePreferImported,
                "replace" | "replace_all" => RestoreStrategy::ReplaceAll,
                other => {
                    eprintln!(
                        "Unknown restore mode '{}'. Use: merge | overwrite | replace.",
                        other
                    );
                    std::process::exit(2);
                }
            };
            let preview =
                apply_backup_restore(&paths, &mut vault, &raw, strategy, &prefs, now_utc)?;
            println!(
                "Restored backup ({}) — {} incoming session(s) ({} new, {} conflicts).",
                strategy.label(),
                preview.incoming_sessions,
                preview.new_sessions,
                preview.conflicting_sessions
            );
        }
        CliCommand::Export { format, output } => {
            if format.trim().eq_ignore_ascii_case("json") {
                let p = export_structured_json_file(&paths, &vault, output.as_deref(), now_utc)?;
                println!("Exported structured JSON to {}", p.display());
            } else {
                let p = export_sessions_csv_file(&paths, &vault, output.as_deref(), now_utc)?;
                println!("Exported sessions CSV to {}", p.display());
            }
        }
        CliCommand::Doctor => {
            let raw = serde_json::to_string(&vault)?;
            let (_, preview) = inspect_and_validate_backup(&raw, &vault, &prefs, now_utc)?;
            let problems = vault_integrity_problems(&vault);
            if problems.is_empty() {
                println!("CHRONOS DATA DOCTOR — HEALTHY ✓");
            } else {
                println!("CHRONOS DATA DOCTOR — {} ISSUE(S) FOUND ✗", problems.len());
                for p in problems.iter().take(25) {
                    println!("  ! {}", p);
                }
                if problems.len() > 25 {
                    println!("  … and {} more", problems.len() - 25);
                }
            }
            println!("  Schema Version  : v{}", preview.source_schema_version);
            println!("  Vault Path      : {}", paths.vault_file.display());
            println!("  Preferences Path: {}", paths.preferences_file.display());
            println!("  Backups Folder  : {}", paths.backups_dir.display());
            println!("  Sessions Stored : {}", vault.sessions.len());
            println!("  Categories      : {}", vault.categories.len());
            println!("  Reading Shelf   : {}", vault.reading_shelf.len());
        }
    }

    Ok(())
}

/// Cross-checks vault invariants that a plain JSON round-trip cannot catch.
pub fn vault_integrity_problems(vault: &crate::storage::ChronosVault) -> Vec<String> {
    use std::collections::HashSet;
    let mut problems = Vec::new();
    let cat_ids: HashSet<&str> = vault.categories.iter().map(|c| c.id.as_str()).collect();

    let mut seen = HashSet::new();
    for s in &vault.sessions {
        if !seen.insert(s.id.as_str()) {
            problems.push(format!("duplicate session id {}", s.id));
        }
        if !cat_ids.contains(s.category_id.as_str()) {
            problems.push(format!("session {} references missing category {}", s.id, s.category_id));
        }
        let slice_sum: u64 = s.day_slices.iter().map(|d| d.seconds).sum();
        if slice_sum != s.elapsed_seconds {
            problems.push(format!(
                "session {} day slices sum to {}s but elapsed is {}s",
                s.id, slice_sum, s.elapsed_seconds
            ));
        }
    }
    for m in &vault.reading_shelf {
        if !cat_ids.contains(m.category_id.as_str()) {
            problems.push(format!("reading item '{}' references missing category", m.title));
        }
    }
    for p in &vault.planned_blocks {
        if !cat_ids.contains(p.category_id.as_str()) {
            problems.push(format!("planned block '{}' references missing category", p.title));
        }
    }
    problems
}

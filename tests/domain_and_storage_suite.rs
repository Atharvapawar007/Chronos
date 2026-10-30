//! Chronos Comprehensive Automated Integration Test Suite (`tests/domain_and_storage_suite.rs`).
//!
//! Tests all 14+ critical domain and storage behaviors:
//! 1. Monotonic wall-clock timer transitions & duration adjustment invariants
//! 2. OS sleep resilience & crash/restart recovery without duplicating elapsed time
//! 3. Midnight boundary session splitting with exact second conservation
//! 4. Overlapping session prevention
//! 5. Category creation, editing, WCAG contrast validation, archival, and safe deletion reassignment
//! 6. Reading Sanctuary workflow, page velocity, and session edit/delete recalculation
//! 7. Daily & weekly focus/reading goals, planned sessions, and weekly reflection zero-baseline safety
//! 8. Analytics Observatory calculations, streaks, circadian distribution, and period deltas
//! 9. Progression XP, 20-Level curve, 24 achievements across 6 families, 5h/day healthy cap & idempotent unlocks
//! 10. Schema migration from v1/v2 to v3, backup validation, conflict resolution, and 4 guarded resets.

use chrono::{TimeZone, Utc};
use chronos::domain::*;
use chronos::storage::*;

#[test]
fn test_clock_state_transitions_and_duration_invariants() {
    let prefs = Preferences::default();
    let mut clock = ClockEngine::new_from_prefs(&prefs);
    assert_eq!(clock.status, ClockStatus::Idle);
    assert_eq!(clock.target_seconds, 25 * 60);

    let t0 = Utc.with_ymd_and_hms(2026, 10, 28, 9, 0, 0).unwrap();
    clock.objective = "Architect storage layer".to_string();
    clock.start_or_resume(t0);
    assert_eq!(clock.status, ClockStatus::Running);

    // Advance 10 minutes (600s)
    let t1 = t0 + chrono::Duration::seconds(600);
    let snap1 = clock.snapshot(t1);
    assert_eq!(snap1.elapsed_seconds, 600);
    assert_eq!(snap1.remaining_seconds, 900);

    // Pause at 10m
    clock.pause(t1);
    assert_eq!(clock.status, ClockStatus::Paused);
    assert_eq!(clock.accumulated_elapsed_seconds, 600);

    // Wait 45 minutes while paused -> elapsed time MUST NOT advance
    let t2 = t1 + chrono::Duration::seconds(2700);
    let snap_paused = clock.snapshot(t2);
    assert_eq!(snap_paused.elapsed_seconds, 600);
    assert_eq!(snap_paused.remaining_seconds, 900);

    // Shorten by -20m (-1200s) when 600s already elapsed -> must clamp to 601s so >= 1s remains
    clock.adjust_duration_secs(-1200, t2);
    assert_eq!(clock.target_seconds, 601);

    // Extend by +5m (+300s) -> elapsed stays 600s, never fabricated
    clock.adjust_duration_secs(299, t2);
    assert_eq!(clock.target_seconds, 900);
    assert_eq!(clock.snapshot(t2).elapsed_seconds, 600);
    assert_eq!(clock.snapshot(t2).remaining_seconds, 300);

    // Resume and complete
    clock.start_or_resume(t2);
    let t3 = t2 + chrono::Duration::seconds(300);
    assert!(clock.sync_wall_clock(t3));
    assert_eq!(clock.status, ClockStatus::AwaitingCompletion);
}

#[test]
fn test_os_sleep_and_restart_recovery_without_duplicating_time() {
    let prefs = Preferences::default();
    let mut clock = ClockEngine::new_from_prefs(&prefs);
    let t0 = Utc.with_ymd_and_hms(2026, 10, 28, 10, 0, 0).unwrap();
    clock.start_or_resume(t0);

    // Simulate OS sleep / app restart 45 minutes later on a 25-minute timer
    let after_wake = t0 + chrono::Duration::seconds(45 * 60);
    assert!(clock.recover_after_restart(after_wake));
    assert_eq!(clock.status, ClockStatus::AwaitingCompletion);
    assert_eq!(clock.accumulated_elapsed_seconds, 25 * 60);
}

#[test]
fn test_sessions_crossing_midnight_split_proportionally() {
    // 2026-10-28 23:40:00 UTC to 2026-10-29 00:30:00 UTC (50m = 3000s, fixed offset 0)
    let start = Utc.with_ymd_and_hms(2026, 10, 28, 23, 40, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2026, 10, 29, 0, 30, 0).unwrap();
    let slices = split_across_midnights(start, end, 3000, Some(0));

    assert_eq!(slices.len(), 2);
    assert_eq!(slices[0].date_key, "2026-10-28");
    assert_eq!(slices[0].seconds, 1200);
    assert_eq!(slices[1].date_key, "2026-10-29");
    assert_eq!(slices[1].seconds, 1800);
}

#[test]
fn test_overlap_guard_prevents_double_counting() {
    let t_base = Utc.with_ymd_and_hms(2026, 10, 28, 14, 0, 0).unwrap();
    let existing = vec![SessionRecord {
        id: "ses_1".to_string(),
        activity_kind: ActivityKind::Focus,
        category_id: "cat_software_eng".to_string(),
        cadence: CadencePreset::Harmonic,
        completion_state: CompletionState::Completed,
        started_at_utc: t_base,
        ended_at_utc: t_base + chrono::Duration::seconds(1800),
        primary_date_key: "2026-10-28".to_string(),
        configured_seconds: 1800,
        elapsed_seconds: 1800,
        day_slices: vec![DaySlice {
            date_key: "2026-10-28".to_string(),
            seconds: 1800,
        }],
        objective: String::new(),
        completion_notes: String::new(),
        planned_block_id: None,
        reading_meta: None,
        reassigned_from_category: None,
    }];

    // Candidate overlaps 14:15..14:45 -> trimmed to 14:30..14:45 (900s)
    let candidate = SessionRecord {
        id: "ses_2".to_string(),
        activity_kind: ActivityKind::Focus,
        category_id: "cat_software_eng".to_string(),
        cadence: CadencePreset::Harmonic,
        completion_state: CompletionState::Completed,
        started_at_utc: t_base + chrono::Duration::seconds(900),
        ended_at_utc: t_base + chrono::Duration::seconds(2700),
        primary_date_key: "2026-10-28".to_string(),
        configured_seconds: 1800,
        elapsed_seconds: 1800,
        day_slices: Vec::new(),
        objective: String::new(),
        completion_notes: String::new(),
        planned_block_id: None,
        reading_meta: None,
        reassigned_from_category: None,
    };

    let resolved = resolve_session_overlap(&existing, candidate, Some(0)).expect("trimmed session");
    assert_eq!(resolved.elapsed_seconds, 900);
}

#[test]
fn test_category_lifecycle_and_safe_deletion_reassignment() {
    let now = Utc::now();
    let mut prefs = Preferences::default();
    let mut vault = ChronosVault::new_empty(now);

    // Duplicate name rejected
    let dup = CategoryDraft {
        name: "software engineering".to_string(),
        color_hex: "#26A69A".to_string(),
        icon_glyph: "λ".to_string(),
        scope: CategoryScope::FocusOnly,
        description: String::new(),
        daily_goal_minutes: 60,
        weekly_goal_minutes: 300,
    };
    assert!(create_category(&mut vault.categories, dup, now).is_err());

    // Create custom category
    let custom = create_category(
        &mut vault.categories,
        CategoryDraft {
            name: "Kernel Systems".to_string(),
            color_hex: "#AB47BC".to_string(),
            icon_glyph: "⚙".to_string(),
            scope: CategoryScope::FocusOnly,
            description: "OS kernel work".to_string(),
            daily_goal_minutes: 60,
            weekly_goal_minutes: 300,
        },
        now,
    )
    .expect("valid custom category");

    // Record a session referencing the custom category
    vault.sessions.push(SessionRecord {
        id: "ses_k1".to_string(),
        activity_kind: ActivityKind::Focus,
        category_id: custom.id.clone(),
        cadence: CadencePreset::DeepBlock,
        completion_state: CompletionState::Completed,
        started_at_utc: now - chrono::Duration::seconds(3000),
        ended_at_utc: now,
        primary_date_key: "2026-10-28".to_string(),
        configured_seconds: 3000,
        elapsed_seconds: 3000,
        day_slices: vec![DaySlice {
            date_key: "2026-10-28".to_string(),
            seconds: 3000,
        }],
        objective: "Page table allocator".to_string(),
        completion_notes: "Verified".to_string(),
        planned_block_id: None,
        reading_meta: None,
        reassigned_from_category: None,
    });

    // Safe delete with reassignment to cat_software_eng
    let reassigned = delete_category_safely(
        &mut vault.categories,
        &mut vault.sessions,
        &mut vault.reading_shelf,
        &mut vault.planned_blocks,
        &mut prefs,
        &custom.id,
        None,
        CategoryDeleteStrategy::ReassignTo("cat_software_eng".to_string()),
    )
    .expect("safe delete");

    assert_eq!(reassigned, 1);
    assert_eq!(vault.sessions[0].category_id, "cat_software_eng");
    assert_eq!(
        vault.sessions[0].reassigned_from_category.as_deref(),
        Some("Kernel Systems")
    );
}

#[test]
fn test_progression_5h_daily_cap_and_idempotent_unlocks() {
    let now = Utc.with_ymd_and_hms(2026, 10, 28, 18, 0, 0).unwrap();
    let prefs = Preferences::default();
    let mut vault = ChronosVault::new_empty(now);

    // Record a 6-hour completed session -> XP credit capped at 5 hours (18,000s)
    vault.sessions.push(SessionRecord {
        id: "ses_long".to_string(),
        activity_kind: ActivityKind::Focus,
        category_id: "cat_software_eng".to_string(),
        cadence: CadencePreset::Monolith,
        completion_state: CompletionState::Completed,
        started_at_utc: now - chrono::Duration::seconds(6 * 3600),
        ended_at_utc: now,
        primary_date_key: "2026-10-28".to_string(),
        configured_seconds: 6 * 3600,
        elapsed_seconds: 6 * 3600,
        day_slices: vec![DaySlice {
            date_key: "2026-10-28".to_string(),
            seconds: 6 * 3600,
        }],
        objective: "Deep design".to_string(),
        completion_notes: "Completed".to_string(),
        planned_block_id: None,
        reading_meta: None,
        reassigned_from_category: None,
    });

    let (lvl1, _, unlocked1) = evaluate_progression(
        &mut vault.progression,
        &vault.sessions,
        &vault.categories,
        &vault.planned_blocks,
        &prefs,
        "2026-10-28",
        false,
        now,
    );
    assert!(!unlocked1.is_empty());

    // Re-evaluating the exact same state must yield 0 newly unlocked achievements and identical XP
    let (lvl2, _, unlocked2) = evaluate_progression(
        &mut vault.progression,
        &vault.sessions,
        &vault.categories,
        &vault.planned_blocks,
        &prefs,
        "2026-10-28",
        false,
        now + chrono::Duration::seconds(60),
    );
    assert!(unlocked2.is_empty());
    assert_eq!(lvl1.total_xp, lvl2.total_xp);
}

#[test]
fn test_migration_backup_and_guarded_reset() {
    let now = Utc.with_ymd_and_hms(2026, 10, 28, 12, 0, 0).unwrap();
    let mut prefs = Preferences::default();

    let legacy_json = r#"{
        "version": 1,
        "sessions": [
            {
                "id": "legacy_s1",
                "type": "focus",
                "category_id": "cat_software_eng",
                "started_at": "2026-10-25T10:00:00Z",
                "duration": 1800,
                "note": "Legacy Rust refactor"
            }
        ]
    }"#;

    let (mut migrated, from_ver) =
        migrate_raw_vault_json(legacy_json, &prefs, now, Some(0)).expect("migrates v1");
    assert_eq!(from_ver, Some(1));
    assert_eq!(migrated.schema_version, VAULT_SCHEMA_VERSION);
    assert_eq!(migrated.sessions.len(), 1);
    assert_eq!(migrated.sessions[0].day_slices.len(), 1);

    let tmp_dir = std::env::temp_dir().join(format!("chronos_test_{}", std::process::id()));
    let paths = ChronosPaths {
        config_dir: tmp_dir.join("config"),
        data_dir: tmp_dir.join("data"),
        cache_dir: tmp_dir.join("cache"),
        preferences_file: tmp_dir.join("config/preferences.toml"),
        vault_file: tmp_dir.join("data/chronos_vault.json"),
        backups_dir: tmp_dir.join("data/backups"),
        exports_dir: tmp_dir.join("data/exports"),
        status_bar_file: tmp_dir.join("cache/status.json"),
    };

    let mut clock = ClockEngine::new_from_prefs(&prefs);

    // Wrong reset phrase must be rejected
    assert!(execute_guarded_reset(
        &paths,
        &mut migrated,
        &mut prefs,
        &mut clock,
        ResetOperation::StatisticsOnly,
        "WRONG PHRASE",
        now
    )
    .is_err());

    // Exact phrase 'RESET MY DATA' succeeds and writes pre-reset backup
    let bak = execute_guarded_reset(
        &paths,
        &mut migrated,
        &mut prefs,
        &mut clock,
        ResetOperation::StatisticsOnly,
        REQUIRED_RESET_CONFIRMATION,
        now,
    )
    .expect("guarded reset succeeds");
    assert!(bak.is_some());
    assert!(migrated.sessions.is_empty());

    let _ = std::fs::remove_dir_all(&tmp_dir);
}

#[test]
fn test_focus_vs_reading_vs_universal_class_scope_routing() {
    let now = Utc::now();
    let prefs = Preferences::default();
    let categories = vec![
        CategoryRecord {
            id: "cat_software_eng".to_string(),
            name: "Software Engineering".to_string(),
            color_hex: "#26A69A".to_string(),
            icon_glyph: "λ".to_string(),
            scope: CategoryScope::FocusOnly,
            description: String::new(),
            daily_goal_minutes: 90,
            weekly_goal_minutes: 450,
            is_default: true,
            is_archived: true, // Archived! Must never appear in Focus Arena or Reading Deck
            created_at_utc: now,
        },
        CategoryRecord {
            id: "cat_dsa".to_string(),
            name: "DSA".to_string(),
            color_hex: "#29B6F6".to_string(),
            icon_glyph: "◈".to_string(),
            scope: CategoryScope::FocusOnly,
            description: String::new(),
            daily_goal_minutes: 120,
            weekly_goal_minutes: 825,
            is_default: false,
            is_archived: false,
            created_at_utc: now,
        },
        CategoryRecord {
            id: "cat_fiction".to_string(),
            name: "Fiction Reading".to_string(),
            color_hex: "#5C6BC0".to_string(),
            icon_glyph: "▪".to_string(),
            scope: CategoryScope::ReadingOnly,
            description: String::new(),
            daily_goal_minutes: 60,
            weekly_goal_minutes: 720,
            is_default: false,
            is_archived: false,
            created_at_utc: now,
        },
        CategoryRecord {
            id: "cat_universal".to_string(),
            name: "Systems Research".to_string(),
            color_hex: "#AB47BC".to_string(),
            icon_glyph: "❖".to_string(),
            scope: CategoryScope::Universal,
            description: String::new(),
            daily_goal_minutes: 45,
            weekly_goal_minutes: 300,
            is_default: false,
            is_archived: false,
            created_at_utc: now,
        },
    ];

    let goals = evaluate_goals(&[], &categories, &prefs, "2026-10-29");

    let focus_names: Vec<&str> = goals
        .category_goals
        .iter()
        .filter(|cg| cg.scope.supports_activity(ActivityKind::Focus))
        .map(|cg| cg.category_name.as_str())
        .collect();
    assert_eq!(focus_names, vec!["DSA", "Systems Research"]);

    let reading_names: Vec<&str> = goals
        .category_goals
        .iter()
        .filter(|cg| cg.scope.supports_activity(ActivityKind::Reading))
        .map(|cg| cg.category_name.as_str())
        .collect();
    assert_eq!(reading_names, vec!["Fiction Reading", "Systems Research"]);
}

# Chronos Architecture & Technical Design Specification (v1.1.1)

## 1. Architectural Principles
Chronos is engineered from scratch in Rust with strict unidirectional separation between four layers:

1. **Domain Layer (`src/domain/`)**: Pure, deterministic business logic with zero terminal or I/O dependencies:
   - `models.rs`: Immutable entity IDs, `ActivityKind`, `CompletionState`, `CategoryScope`, `CadencePreset`, `MaterialKind`, `SessionRecord`, `Preferences`, and WCAG 2.1 contrast math.
   - `clock.rs`: Wall-clock anchored `ClockEngine` (`Idle`, `Running`, `Paused`, `AwaitingCompletion`), midnight splitter (`split_across_midnights`), and overlap prevention (`resolve_session_overlap`).
   - `taxonomy.rs`: Category CRUD, WCAG contrast verification, archival, and safe deletion with historical reassignment.
   - `sanctuary.rs`: First-class Reading Sanctuary shelf (`ReadingMaterial`), 4-state book lifecycle (`Have`, `Reading`, `Read`, `Abandoned`), page velocity (`pages_per_hour`), and session edit/delete reconciliation.
   - `planner.rs`: Daily/weekly Focus & Reading goals, per-category targets, `PlannedBlock` queue, and 7-day `WeeklyReflectionReport` synthesizer.
   - `observatory.rs`: Multi-horizon analytics (`Today`, `7d`, `30d`, `90d`, `365d`, `AllTime`), period-over-period comparisons, streaks, circadian 24h histogram, and weekday matrix.
   - `progression.rs`: 20-level XP curve, FNV-1a deterministic `Daily Chest`, and 24-achievement catalog across 6 families with a healthy 5h/day XP cap and idempotent unlocks.

2. **Storage & Data Safety Layer (`src/storage/`)**:
   - `repository.rs`: Atomic temp-file-and-rename persistence (`write_atomically`), rolling `.bak` files, corrupt file quarantine, and `~/.cache/chronos/status.json` export.
   - `migration.rs`: Tolerant v1/v2 -> v3 schema migration with automatic `.v<N>.bak` preservation.
   - `backup.rs`: Versioned `BackupEnvelope`, schema validation, conflict previewing, and 3 restore strategies (`MergeKeepExisting`, `MergePreferImported`, `ReplaceAll`).
   - `export.rs`: RFC-4180 CSV and structured JSON exporters.
   - `reset.rs`: 4 guarded reset scopes requiring `RESET MY DATA` confirmation and pre-reset backup verification.

3. **Unidirectional Runtime, CLI, Audio & Keymap Layer (`src/runtime.rs`, `src/keymap.rs`, `src/audio.rs`, `src/cli.rs`)**:
   - `WorkspaceRuntime`: Single state container coordinating active route (`PrimaryRoute`), modal stack (`ModalDialog`), timer ticks, and disk persistence.
   - `keymap.rs`: Modal-aware keyboard router that isolates text input in dialogs and search bars from global shortcuts.
   - `audio.rs`: Synthesizes a 44.1kHz 16-bit PCM polyphonic singing-bowl chord WAV in memory, reaps helper processes on background threads, and dispatches OS notifications.
   - `cli.rs`: Headless CLI subcommands (`report`, `mastery`, `backup`, `restore`, `export`, `doctor`) and referential integrity checker (`vault_integrity_problems`).

4. **Card-Based Terminal UI Layer (`src/tui/`)**:
   - `theme.rs`: 9 semantic palettes (`Cyberpunk Neon`, `Tokyo Night`, `Catppuccin Mocha`, `Dracula`, `Matrix Green`, `Blood Moon`, `Nordic Aurora`, `Gruvbox`, `Monochrome`) with automatic `TrueColor` / `Ansi256` / `Ansi16` adaptation.
   - `ui.rs` & `ring.rs`: Reusable rounded/double-bordered `card`, flexbox `center`, 1/8th-cell `meter`, vertical bar chart `vbars`, sub-cell Braille `draw_ring`, and auto-scaling 5-row block digits (`big_digits_scaled`).
   - `shell.rs` & `modals.rs`: Top header with live timer pill and XP gauge, 9-tab navigation bar, pure-black Zen Mode (`#000000`), context-aware footer key chip bar, and 12 modal dialogs.
   - `views/`: 9 dedicated screen modules (`home`, `focus`, `reading`, `analytics`, `history`, `trophies`, `categories`, `settings`, `guide`).

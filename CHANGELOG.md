# Changelog

## 1.1.1

### Added
- Books now have four states: **Have**, **Reading**, **Read**, **Abandoned**. `S` cycles them; they also update automatically (starting a session makes a book Reading, reaching the last page makes it Read). Old saves load unchanged (active→Reading, completed→Read, paused→Have).
- `D` deletes the selected book after a confirmation. Every book can be deleted, including the last one; past sessions stay in history and stats.

### Fixed
- Zen mode background is now pure black in every theme (it used the theme colour, with a black strip behind the ring).
- The ring no longer draws a black rectangle on the card background in the normal Focus/Reading screens.
- The Reading timer showed the Focus rhythm name (e.g. "Harmonic"), which looked changeable but isn't used for reading. It now shows the session length and how to change it.

## 1.0.1 — Stability & Polish

### Added
- Chronos ASCII-art banner with the tagline *Discipline = Value + Reward - Friction* on the Command Deck (full and compact variants, hidden on short terminals).
- `chronos doctor` now performs real integrity checks (duplicate IDs, missing category references, day-slice/elapsed mismatches).
- Render smoke tests across all views and terminal sizes (1x1 to 200x60) plus regression tests for the fixes below.
- `Cargo.lock` shipped so `scripts/install.sh` (`--locked`) works; `rust-version = "1.85"` declared.

### Fixed
- **Crash**: pressing `+`/`-` on a finished 4-hour phase panicked (`clamp` with min > max). Finished phases are now locked and the clamp is always valid.
- **Data loss**: launching a planned block or shelf item while a session was running silently discarded it; launches are now refused (or resume, for the same material).
- **Wrong reading velocity**: pages/hour used the absolute page number; it now uses pages actually read in recorded sessions (reconciled on startup).
- Editing a session's duration could extend it into the next session and double-count time; it is now capped at the next productive session's start.
- Reading items downgraded below the final page now return from `Completed` to `Reading`.
- Status-bar export (`status.json`) froze between state changes; it now refreshes every second while running.
- Vault save failures were silently ignored; they now surface as an on-screen message. Temp files are cleaned up on failed atomic writes.
- Audio/notification helper processes were never reaped (zombies); the documented terminal-bell fallback is now implemented.
- `chronos restore --mode <typo>` silently merged; unknown modes now error out.
- Removed a hard-coded fallback date in the activity heatmap.
- README/theme documentation now matches the 8 shipped dark themes.
- Resolved all `cargo clippy` warnings.

## 1.0.0 — Initial Production Release

### Added
- **Clean-Slate Rust & Ratatui Architecture**: Layered separation between `domain`, `storage`, `runtime`, and `tui`.
- **Material Design 3 Terminal System**: 6 themes (4 Dark + 2 Light: `Athenaeum Dark`, `Alabaster Light`, `Botanical Sage`, `Terracotta Dusk`, `Monolith Slate`, `Solarized Parchment`) with automatic 24-bit / 256-color / 16-color adaptation.
- **Adaptive Application Shell**: 8-destination Navigation Rail (`Overview`, `Focus Studio`, `Reading Sanctuary`, `Analytics`, `Session History`, `Achievements`, `Categories`, `Settings & Data`), live top-bar active-timer pill, and distraction-free Zen Mode.
- **Monotonic Wall-Clock Timer Engine**: Immune to OS sleep and callback drift, with automatic midnight session splitting (`DaySlice`) and overlap prevention.
- **Dedicated Reading Sanctuary**: Manage books, academic papers, manuals, and articles with page checkpoints and pages-per-hour (`p/h`) velocity tracking.
- **Full Category Studio**: Create, edit, recolor (with live WCAG contrast ratio validation), archive, and safely delete categories with historical session reassignment.
- **Analytics Observatory & Weekly Reflection**: 6 time horizons, 4 analytical lenses, period-over-period comparisons, 24-hour circadian distribution, and automated weekly schedule insights.
- **Sustainable Progression System**: 20 levels and 24 achievements across 6 families with a healthy 5-hour/day XP ceiling and optional visibility toggle.
- **Guarded Data Safety**: Versioned backups, conflict-aware restore (`MergeKeepExisting`, `MergePreferImported`, `ReplaceAll`), CSV/JSON export, and 4 reset scopes guarded by `RESET MY DATA` and automatic pre-reset backups.

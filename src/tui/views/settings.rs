//! Settings: groups on the left, rows on the right, theme previews and local-first info.

use super::Ctx;
use crate::runtime::WorkspaceRuntime;
use crate::tui::theme::{Palette, ThemeId};
use crate::tui::ui;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    Frame,
};

pub const SETTINGS_SECTIONS: [&str; 6] = ["Look & Feel", "Focus Timer", "Reading", "Goals", "Alerts & Sound", "Data & Safety"];

fn onoff(b: bool) -> String { if b { "ON".into() } else { "OFF".into() } }

/// (label, value, explanation). The order must match `apply_settings_adjustment` in the keymap.
pub fn build_section_rows(rt: &WorkspaceRuntime) -> Vec<(String, String, String)> {
    let r = |a: &str, b: String, c: &str| (a.to_string(), b, c.to_string());
    let pr = &rt.prefs;
    match rt.settings_section_idx % SETTINGS_SECTIONS.len() {
        0 => vec![
            r("Colour theme", ThemeId::from_key(&pr.appearance.theme).label().into(), "Nine themes, including a pure black-white-grey one. T cycles quickly."),
            r("Spacing", pr.appearance.density.label().into(), "Comfortable or compact layout."),
            r("Animation", pr.appearance.motion.label().into(), "Turn off to freeze the ring's glowing tip."),
            r("Show XP & levels", onoff(pr.appearance.show_gamification), "Hide the game layer if you prefer a plain timer."),
            r("Show tab names", onoff(pr.appearance.show_navigation_labels), "Full names on the tab strip, or numbers only."),
        ],
        1 => vec![
            r("Rhythm", pr.focus.active_cadence.short_name().into(), "Harmonic 25/5, Deep Block 50/10, Monolith 90/20, or Custom."),
            r("Custom focus (min)", pr.focus.custom_focus_minutes.to_string(), "Length of one focus block in Custom rhythm."),
            r("Custom short break (min)", pr.focus.custom_short_break_minutes.to_string(), "Break between focus blocks."),
            r("Custom long break (min)", pr.focus.custom_long_break_minutes.to_string(), "Longer break after a full cycle."),
            r("Auto-start breaks", onoff(pr.focus.auto_start_breaks), "Start the break timer by itself when focus ends."),
            r("Count cut-short sessions", onoff(pr.focus.count_interrupted_in_totals), "Include unfinished sessions in totals and streaks."),
        ],
        2 => vec![
            r("Default reading time (min)", pr.reading.default_duration_minutes.to_string(), "Length of a new reading session."),
            r("Track pages", onoff(pr.reading.track_pages_read), "Ask for the page you reached when finishing."),
            r("Ask for notes", onoff(pr.reading.prompt_notes_on_complete), "Show the note box when a session ends."),
        ],
        3 => vec![
            r("Daily focus goal (min)", pr.goals.daily_focus_minutes.to_string(), "Reaching it opens the Daily Chest. 60+ minutes earns chests."),
            r("Weekly focus goal (min)", pr.goals.weekly_focus_minutes.to_string(), "Your target for the whole week."),
            r("Daily reading goal (min)", pr.goals.daily_reading_minutes.to_string(), "Your target for each day."),
            r("Weekly reading goal (min)", pr.goals.weekly_reading_minutes.to_string(), "Your target for the whole week."),
        ],
        4 => vec![
            r("Desktop alerts", onoff(pr.notifications.desktop_notifications), "Pop-up when a session ends (needs notify-send)."),
            r("Sound", onoff(pr.notifications.sound_enabled), "Soft chime when a session ends."),
            r("Status-bar file", onoff(pr.notifications.status_bar_export), "Writes status.json for Waybar or Polybar."),
        ],
        _ => vec![
            r("Back up now", "Enter".into(), "Saves a full copy of all your data."),
            r("Restore latest backup", "Enter".into(), "Shows a preview first. Nothing changes until you confirm."),
            r("Export CSV + JSON", "Enter".into(), "Your data as files you own."),
            r("Reset statistics", "Enter".into(), "Deletes sessions only. Asks you to type a phrase."),
            r("Reset XP & trophies", "Enter".into(), "Keeps sessions, restarts the game layer."),
            r("Reset preferences", "Enter".into(), "Back to default settings."),
            r("Factory reset", "Enter".into(), "Everything. A backup is made first."),
        ],
    }
}

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let cols = ui::fit_cols(area, &[2, 5], 30);
    let sec = c.rt.settings_section_idx % SETTINGS_SECTIONS.len();

    if cols.len() > 1 {
        let left = ui::rows(cols[0], &[Constraint::Length(SETTINGS_SECTIONS.len() as u16 + 2), Constraint::Min(5)]);
        let inner = ui::card(f, left[0], p, "☰", "Groups (Tab)", p.accent, false);
        let lines: Vec<Line> = SETTINGS_SECTIONS.iter().enumerate().map(|(i, t)| {
            let on = i == sec;
            Line::from(Span::styled(ui::pad_right(&format!(" {} {}. {}", if on { "▸" } else { " " }, i + 1, t), inner.width as usize), if on { Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD) } else { Style::default().fg(p.text) }))
        }).collect();
        ui::text(f, inner, lines);

        let inner = ui::card(f, left[1], p, "◈", "Your data", p.good, false);
        let mut lines = vec![
            Line::from(Span::styled("100% offline · zero tracking", Style::default().fg(p.good).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(format!("{} sessions saved", c.rt.vault.sessions.len()), Style::default().fg(p.text))),
            Line::from(Span::styled(format!("{} backups on disk", c.rt.available_backups.len()), Style::default().fg(p.text))),
            Line::from(Span::styled(format!("{} skills · {} books", c.rt.vault.categories.len(), c.rt.vault.reading_shelf.len()), Style::default().fg(p.text))),
            Line::from(Span::styled(format!("Level {} · {} XP", c.lvl.level, c.lvl.total_xp), Style::default().fg(p.text))),
            Line::from(Span::styled(format!("Chronos v{}", env!("CARGO_PKG_VERSION")), Style::default().fg(p.dim))),
            Line::from(Span::styled(format!("File: {}", c.rt.paths.vault_file.display()), Style::default().fg(p.dim))),
        ];
        let tips = ["Back up before big changes (B).", "T cycles themes anywhere.", "Goals of 60+ min open the chest.", "Zen mode starts from Z.", "Export CSV with X in Stats.", "Run `chronos doctor` to check data.", "Everything is stored locally.", "Nothing is ever sent online."].map(String::from);
        ui::fill_hints(&mut lines, inner.height, p, &tips);
        ui::text(f, inner, lines);
    }

    let main = *cols.last().unwrap();
    let rows = build_section_rows(c.rt);
    let cur = c.rt.settings_row_idx.min(rows.len().saturating_sub(1));
    let show_themes = sec == 0 && main.height >= 22;
    let parts = ui::rows(main, &[Constraint::Min(8), Constraint::Length(if show_themes { 7 } else { 0 })]);
    let inner = ui::card(f, parts[0], p, "⚙", &format!("{}  (↑↓ pick · ←→/Enter change)", SETTINGS_SECTIONS[sec]), p.accent2, false);
    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let per = (inner.height as usize / rows.len().max(1)).clamp(2, 5);
    for (i, (label, val, hint)) in rows.iter().enumerate() {
        let sel = i == cur;
        let bg = if sel { p.card_hi } else { p.card };
        let val_s = format!(" ‹ {} › ", val);
        lines.push(Line::from(vec![
            Span::styled(if sel { "▸ " } else { "  " }, Style::default().fg(p.accent2).bg(bg).add_modifier(Modifier::BOLD)),
            Span::styled(ui::pad_right(label, w.saturating_sub(val_s.chars().count() + 2)), Style::default().fg(if sel { p.text } else { p.dim }).bg(bg).add_modifier(Modifier::BOLD)),
            Span::styled(val_s, Style::default().fg(if sel { p.on_accent } else { p.accent }).bg(if sel { p.accent2 } else { bg }).add_modifier(Modifier::BOLD)),
        ]));
        lines.push(Line::from(Span::styled(format!("  {}", hint), Style::default().fg(p.dim))));
        for _ in 2..per { lines.push(Line::from("")); }
    }
    ui::text(f, inner, lines);

    if show_themes {
        let inner = ui::card(f, parts[1], p, "◐", "Theme previews", p.accent3, false);
        let mut lines = Vec::new();
        let per_row = if inner.width >= 84 { 3 } else { 2 };
        for chunk in ThemeId::ALL.chunks(per_row) {
            let mut s = Vec::new();
            for t in chunk {
                let pal = Palette::with_depth(*t, p.depth);
                let on = *t == p.id;
                s.push(Span::styled(if on { "▸" } else { " " }, Style::default().fg(p.accent)));
                for col in [pal.accent, pal.accent2, pal.accent3, pal.good] { s.push(Span::styled("██", Style::default().fg(col))); }
                s.push(Span::styled(format!(" {:<17}  ", t.label()), Style::default().fg(if on { p.text } else { p.dim })));
            }
            lines.push(Line::from(s));
        }
        ui::center(f, inner, p, lines);
    }
}

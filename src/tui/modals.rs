//! Pop-up dialogs. Every dialog is a centred card; forms show one labelled input per field.

use super::ui;
use super::views::Ctx;
use crate::domain::{ActivityKind, CategoryScope, APPROVED_CATEGORY_COLORS, CATEGORY_ICON_GLYPHS, REQUIRED_RESET_CONFIRMATION};
use crate::runtime::ModalDialog;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    Frame,
};

struct Field {
    label: &'static str,
    value: String,
    focused: bool,
    /// true = typed text (shows a cursor), false = chosen with ←/→
    typed: bool,
    swatch: Option<ratatui::style::Color>,
}

fn text_field(label: &'static str, value: &str, focused: bool) -> Field {
    Field { label, value: value.to_string(), focused, typed: true, swatch: None }
}
fn choice_field(label: &'static str, value: String, focused: bool) -> Field {
    Field { label, value, focused, typed: false, swatch: None }
}

fn centered(area: Rect, w: u16, h: u16) -> Rect {
    let w = w.min(area.width);
    let h = h.min(area.height);
    Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h)
}

fn draw_form(f: &mut Frame, area: Rect, c: &Ctx, icon: &str, title: &str, color: ratatui::style::Color, intro: Vec<Line<'static>>, fields: Vec<Field>, footer: &str) {
    let p = &c.p;
    let roomy = area.height >= (fields.len() as u16 * 2 + intro.len() as u16 + 6).max(14);
    let per = if roomy { 2 } else { 1 };
    let h = fields.len() as u16 * per + intro.len() as u16 + 4;
    let w = 72u16.min(area.width.saturating_sub(2)).max(30);
    let r = centered(area, w, h);
    let inner = ui::card(f, r, p, icon, title, color, true);
    let mut lines: Vec<Line> = intro;
    let width = inner.width as usize;
    for fd in fields {
        let marker = if fd.focused { "▸ " } else { "  " };
        let label_style = Style::default().fg(if fd.focused { color } else { p.dim }).add_modifier(Modifier::BOLD);
        let mut value_text = fd.value.clone();
        if fd.typed && fd.focused { value_text.push('▏'); }
        let shown = if fd.typed { ui::clamp_str(&value_text, width.saturating_sub(6)) } else { format!("‹ {} ›", ui::clamp_str(&value_text, width.saturating_sub(10))) };
        let box_style = Style::default().fg(if fd.focused { p.on_accent } else { p.text }).bg(if fd.focused { color } else { p.card_hi }).add_modifier(Modifier::BOLD);
        let mut val_spans = vec![Span::raw(if per == 2 { "  " } else { "" })];
        if let Some(sw) = fd.swatch { val_spans.push(Span::styled("██ ", Style::default().fg(sw))); }
        val_spans.push(Span::styled(format!(" {} ", shown), box_style));
        if per == 2 {
            lines.push(Line::from(vec![Span::styled(marker, label_style), Span::styled(fd.label, label_style)]));
            lines.push(Line::from(val_spans));
        } else {
            let mut row = vec![Span::styled(marker, label_style), Span::styled(format!("{:<14}", fd.label), label_style)];
            row.extend(val_spans);
            lines.push(Line::from(row));
        }
    }
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(footer.to_string(), Style::default().fg(p.dim))));
    ui::text(f, inner, lines);
}

fn simple(f: &mut Frame, area: Rect, c: &Ctx, icon: &str, title: &str, color: ratatui::style::Color, lines: Vec<Line<'static>>, w: u16) {
    let h = lines.len() as u16 + 4;
    let r = centered(area, w.min(area.width.saturating_sub(2)).max(30), h);
    let inner = ui::card(f, r, &c.p, icon, title, color, true);
    ui::center(f, inner, &c.p, lines);
}

fn line(c: &Ctx, s: impl Into<String>, color: ratatui::style::Color, bold: bool) -> Line<'static> {
    let _ = c;
    let mut st = Style::default().fg(color);
    if bold { st = st.add_modifier(Modifier::BOLD); }
    Line::from(Span::styled(s.into(), st))
}

pub fn render_modal(f: &mut Frame, area: Rect, c: &Ctx) {
    let Some(modal) = &c.rt.modal else { return };
    let p = &c.p;
    match modal {
        ModalDialog::ShortcutHelp => {
            let rows = [("1-9", "Go to a screen"), ("Space", "Start / pause"), ("Enter", "Finish session"), ("Z", "Zen mode"), ("T", "Theme"), ("+ / -", "±5 minutes"), ("?", "Full guide"), ("Q", "Quit")];
            let lines: Vec<Line> = rows.iter().map(|(k, d)| Line::from(vec![Span::styled(format!(" {:<6}", k), Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD)), Span::styled(format!("  {}", d), Style::default().fg(p.text))])).collect();
            let mut lines = lines;
            lines.push(Line::from(""));
            lines.push(line(c, "Press ? for the full guide · Esc closes", p.dim, false));
            simple(f, area, c, "◈", "Quick keys", p.accent, lines, 44);
        }
        ModalDialog::QuitConfirmation => {
            let lines = vec![
                line(c, "A session is still running.", p.text, true),
                line(c, "", p.text, false),
                Line::from(vec![Span::styled(" Y ", Style::default().fg(p.on_accent).bg(p.good).add_modifier(Modifier::BOLD)), Span::styled(" save it & quit   ", Style::default().fg(p.text)), Span::styled(" S ", Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD)), Span::styled(" pause & quit   ", Style::default().fg(p.text)), Span::styled(" N ", Style::default().fg(p.on_accent).bg(p.warn).add_modifier(Modifier::BOLD)), Span::styled(" stay", Style::default().fg(p.text))]),
            ];
            simple(f, area, c, "◈", "Leave Chronos?", p.warn, lines, 62);
        }
        ModalDialog::ObjectiveAndNotesEditor { field_index, objective_buf, notes_buf, end_page_buf, complete_on_submit } => {
            let reading = c.rt.clock.activity_kind == ActivityKind::Reading;
            let mut fields = vec![text_field("Goal of this session", objective_buf, *field_index == 0), text_field("Notes (what did you get done?)", notes_buf, *field_index == 1)];
            if reading { fields.push(text_field("Page you reached", end_page_buf, *field_index == 2)); }
            let intro = vec![line(c, format!("Time so far: {}  ·  worth about +{} XP", ui::hm(c.snap.elapsed_seconds), c.snap.elapsed_seconds / 60 * 2 + 15), p.good, true), line(c, "", p.text, false)];
            draw_form(f, area, c, "✎", if *complete_on_submit { "Finish & save session" } else { "Edit session info" }, p.accent, intro, fields, "Tab next field · Enter save · Esc cancel");
        }
        ModalDialog::CategoryEditor { editing_id, field_index, name_buf, color_idx, icon_idx, scope, daily_goal_minutes, weekly_goal_minutes, description_buf } => {
            let (hex, cname) = APPROVED_CATEGORY_COLORS[*color_idx % APPROVED_CATEGORY_COLORS.len()];
            let (glyph, gname) = CATEGORY_ICON_GLYPHS[*icon_idx % CATEGORY_ICON_GLYPHS.len()];
            let mut colour = choice_field("Colour", cname.to_string(), *field_index == 1);
            colour.swatch = Some(p.category(hex));
            let scope_s = match scope { CategoryScope::FocusOnly => "Focus only", CategoryScope::ReadingOnly => "Reading only", CategoryScope::Universal => "Focus & reading" };
            let goal = |m: &u32| if *m == 0 { "no goal".to_string() } else { ui::hm(*m as u64 * 60) };
            let fields = vec![
                text_field("Name", name_buf, *field_index == 0),
                colour,
                choice_field("Icon", format!("{}  {}", glyph, gname), *field_index == 2),
                choice_field("Used for", scope_s.to_string(), *field_index == 3),
                choice_field("Daily goal", goal(daily_goal_minutes), *field_index == 4),
                choice_field("Weekly goal", goal(weekly_goal_minutes), *field_index == 5),
                text_field("Description", description_buf, *field_index == 6),
            ];
            draw_form(f, area, c, "◆", if editing_id.is_some() { "Edit skill" } else { "New skill" }, p.good, vec![], fields, "Tab next · ←→ change · Enter save · Esc cancel");
        }
        ModalDialog::CategoryDeleteConfirm { category_name, linked_sessions_count, use_archive_instead, replacement_category_idx, .. } => {
            let others: Vec<_> = c.rt.vault.categories.iter().filter(|x| x.name != *category_name && !x.is_archived).collect();
            let repl = others.get(*replacement_category_idx % others.len().max(1)).map(|x| x.name.clone()).unwrap_or("-".into());
            let lines = vec![
                line(c, format!("'{}' has {} sessions.", category_name, linked_sessions_count), p.text, true),
                line(c, "", p.text, false),
                line(c, if *use_archive_instead { "▸ Archive it (hide, keep history)" } else { "▸ Delete it and move its sessions" }, p.warn, true),
                line(c, if *use_archive_instead { "Tab: switch to delete".to_string() } else { format!("Move sessions to: ‹ {} ›  (←/→)", repl) }, p.dim, false),
                line(c, "Enter confirm · Esc cancel", p.dim, false),
            ];
            simple(f, area, c, "!", "Remove skill", p.warn, lines, 56);
        }
        ModalDialog::ReadingMaterialEditor { editing_id, field_index, title_buf, author_buf, kind, category_id, total_pages_buf, current_page_buf, notes_buf } => {
            let cat = c.rt.vault.categories.iter().find(|x| &x.id == category_id).map(|x| x.name.clone()).unwrap_or("-".into());
            let fields = vec![
                text_field("Title", title_buf, *field_index == 0),
                text_field("Author", author_buf, *field_index == 1),
                choice_field("Type", kind.label().to_string(), *field_index == 2),
                text_field("Total pages", total_pages_buf, *field_index == 3),
                text_field("Page now", current_page_buf, *field_index == 4),
                text_field("Notes", notes_buf, *field_index == 5),
                choice_field("Skill", cat, *field_index == 6),
            ];
            draw_form(f, area, c, "▣", if editing_id.is_some() { "Edit book" } else { "Add a book" }, p.accent2, vec![], fields, "Tab next · ←→ change · Enter save · Esc cancel");
        }
        ModalDialog::PlannedBlockEditor { field_index, title_buf, activity_kind, category_id, duration_minutes, priority } => {
            let cat = c.rt.vault.categories.iter().find(|x| &x.id == category_id).map(|x| x.name.clone()).unwrap_or("-".into());
            let fields = vec![
                text_field("What will you do?", title_buf, *field_index == 0),
                choice_field("Type", activity_kind.short_label().to_string(), *field_index == 1),
                choice_field("Skill", cat, *field_index == 2),
                choice_field("Length", format!("{} min", duration_minutes), *field_index == 3),
                choice_field("Priority", format!("{:?}", priority), *field_index == 4),
            ];
            draw_form(f, area, c, "▸", "Plan a block", p.accent2, vec![line(c, "A planned block is a promise to your future self (+20 XP when done).", p.dim, false), line(c, "", p.text, false)], fields, "Tab next · ←→ change · Enter save · Esc cancel");
        }
        ModalDialog::SessionEditor { field_index, objective_buf, notes_buf, elapsed_mins_buf, category_idx, pages_read_buf, completion_state, .. } => {
            let cat = c.rt.vault.categories.get(*category_idx % c.rt.vault.categories.len().max(1)).map(|x| x.name.clone()).unwrap_or("-".into());
            let fields = vec![
                text_field("Goal", objective_buf, *field_index == 0),
                text_field("Notes", notes_buf, *field_index == 1),
                text_field("Minutes", elapsed_mins_buf, *field_index == 2),
                choice_field("Skill", cat, *field_index == 3),
                text_field("Pages read", pages_read_buf, *field_index == 4),
                choice_field("Result", completion_state.label().to_string(), *field_index == 5),
            ];
            draw_form(f, area, c, "✎", "Edit session", p.accent, vec![], fields, "Tab next · ←→ change · Enter save · Esc cancel");
        }
        ModalDialog::SessionDeleteConfirm { summary_label, .. } => {
            let lines = vec![line(c, "Delete this session?", p.text, true), line(c, summary_label.clone(), p.warn, false), line(c, "", p.text, false), line(c, "Y / Enter = delete · N / Esc = keep", p.dim, false), line(c, "XP and streaks are recalculated.", p.dim, false)];
            simple(f, area, c, "!", "Delete session", p.bad, lines, 60);
        }
        ModalDialog::ReadingDeleteConfirm { title, sessions_count, .. } => {
            let lines = vec![
                line(c, format!("Remove '{}' from your shelf?", ui::clamp_str(title, 44)), p.text, true),
                line(c, if *sessions_count > 0 { format!("Your {} reading sessions stay in your history and stats.", sessions_count) } else { "It has no reading sessions yet.".to_string() }, p.dim, false),
                line(c, "", p.text, false),
                line(c, "Y / Enter = remove · N / Esc = keep it", p.dim, false),
            ];
            simple(f, area, c, "!", "Delete book", p.bad, lines, 64);
        }
        ModalDialog::BackupRestoreConfirm { backup_path, preview, strategy, .. } => {
            let name = backup_path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
            let lines = vec![
                line(c, name, p.accent, true),
                line(c, "", p.text, false),
                line(c, format!("Sessions in backup: {}  ({} new, {} overlap)", preview.incoming_sessions, preview.new_sessions, preview.conflicting_sessions), p.text, false),
                line(c, format!("Skills: {} ({} overlap) · Books: {} · Trophies: {}", preview.incoming_categories, preview.conflicting_categories, preview.incoming_reading_items, preview.incoming_achievements), p.text, false),
                line(c, "", p.text, false),
                line(c, format!("‹ {} ›", strategy.label()), p.warn, true),
                line(c, "←/→ change method · Enter restore · Esc cancel", p.dim, false),
            ];
            simple(f, area, c, "↺", "Restore backup", p.warn, lines, 76);
        }
        ModalDialog::DestructiveResetConfirm { operation, typed_phrase } => {
            let ok = typed_phrase.trim() == REQUIRED_RESET_CONFIRMATION;
            let lines = vec![
                line(c, operation.title(), p.bad, true),
                line(c, format!("This deletes: {}", operation.deletes_summary()), p.text, false),
                line(c, "", p.text, false),
                line(c, format!("Type  {}  to confirm:", REQUIRED_RESET_CONFIRMATION), p.dim, false),
                Line::from(vec![Span::styled(format!(" {}▏ ", typed_phrase), Style::default().fg(if ok { p.on_accent } else { p.text }).bg(if ok { p.good } else { p.card_hi }).add_modifier(Modifier::BOLD))]),
                line(c, if ok { "✔ Press Enter to continue" } else { "Enter does nothing until the phrase matches" }, if ok { p.good } else { p.dim }, false),
            ];
            simple(f, area, c, "✖", "Danger zone", p.bad, lines, 78);
        }
        ModalDialog::CelebrationBanner { headline, subtitle, unlocked_titles, completed_full_cycle, opened_at } => {
            let t = opened_at.elapsed().as_millis() as u64 / 250;
            let spark = ["✦", "✧", "★", "✧"][(t % 4) as usize];
            let col = [p.accent, p.accent2, p.accent3, p.good][(t % 4) as usize];
            let mut lines = vec![
                Line::from(Span::styled(format!("{} {} {}", spark, headline, spark), Style::default().fg(col).add_modifier(Modifier::BOLD))),
                line(c, subtitle.clone(), p.text, false),
                line(c, "", p.text, false),
            ];
            for (title, desc, xp) in unlocked_titles.iter().take(5) {
                lines.push(Line::from(vec![Span::styled("🏆 ".replace("🏆", "✦"), Style::default().fg(p.accent3)), Span::styled(title.clone(), Style::default().fg(p.text).add_modifier(Modifier::BOLD)), Span::styled(format!("  +{} XP", xp), Style::default().fg(p.good).add_modifier(Modifier::BOLD))]));
                lines.push(line(c, format!("   {}", desc), p.dim, false));
            }
            if *completed_full_cycle { lines.push(line(c, "Full cycle complete — take a proper long break!", p.accent2, true)); }
            lines.push(line(c, "", p.text, false));
            lines.push(line(c, "Enter / Space to continue", p.dim, false));
            simple(f, area, c, "★", "Nice work", col, lines, 64);
        }
    }
}

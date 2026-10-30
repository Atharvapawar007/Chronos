//! Log (session history): filter bar, session list and a detail card for the selected session.

use super::{kind_glyph, session_xp, weekday_name, Ctx};
use crate::domain::{ActivityKind, CompletionState};
use crate::tui::ui;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let rt = c.rt;
    let list = rt.filtered_sessions();
    let r = ui::rows(area, &[Constraint::Length(1), Constraint::Length(if area.height >= 20 { 5 } else { 0 }), Constraint::Min(5)]);

    // filter bar
    let chip = |label: &str, val: String, on: bool| -> Vec<Span<'static>> {
        vec![
            Span::styled(format!(" {} ", label), Style::default().fg(p.on_accent).bg(if on { p.accent } else { p.border }).add_modifier(Modifier::BOLD)),
            Span::styled(format!(" {}  ", val), Style::default().fg(if on { p.text } else { p.dim })),
        ]
    };
    let mut bar = Vec::new();
    let search = if rt.ledger_search_editing { format!("{}▏", rt.ledger_search_query) } else if rt.ledger_search_query.is_empty() { "type / to search".to_string() } else { rt.ledger_search_query.clone() };
    bar.extend(chip("/", search, rt.ledger_search_editing || !rt.ledger_search_query.is_empty()));
    bar.extend(chip("F", rt.ledger_activity_filter.map(|a| a.short_label().to_string()).unwrap_or("all types".into()), rt.ledger_activity_filter.is_some()));
    bar.extend(chip("S", rt.ledger_status_filter.map(|s| s.label().to_string()).unwrap_or("any status".into()), rt.ledger_status_filter.is_some()));
    let cat = rt.ledger_category_filter.as_ref().and_then(|id| rt.vault.categories.iter().find(|x| &x.id == id));
    bar.extend(chip("C", cat.map(|x| x.name.clone()).unwrap_or("all skills".into()), cat.is_some()));
    f.render_widget(Paragraph::new(Line::from(bar)), r[0]);

    if r[1].height > 0 {
        let cols = ui::fit_cols(r[1], &[1, 1, 1, 1], 18);
        let total: u64 = list.iter().filter(|s| s.activity_kind.is_productive()).map(|s| s.elapsed_seconds).sum();
        let xp: u64 = list.iter().map(|s| session_xp(s)).sum();
        let done = list.iter().filter(|s| s.completion_state == CompletionState::Completed).count();
        let longest = list.iter().map(|s| s.elapsed_seconds).max().unwrap_or(0);
        let tiles = [
            ("≡", "Sessions", format!("{}", list.len()), format!("{} finished", done), p.accent),
            ("◷", "Total time", ui::hm(total), "focus + reading".into(), p.accent2),
            ("✦", "XP earned", format!("{}", xp), "from these sessions".into(), p.good),
            ("★", "Longest", ui::hm(longest), "single session".into(), p.accent3),
        ];
        for (i, t) in tiles.into_iter().enumerate().take(cols.len()) {
            ui::tile(f, cols[i], p, t.0, t.1, t.2, t.3, t.4);
        }
    }

    let cols = ui::fit_cols(r[2], &[3, 2], 46);
    let inner = ui::card(f, cols[0], p, "≡", "Sessions  (E edit · D delete)", p.accent, false);
    let mut sorted = list.clone();
    sorted.sort_by_key(|s| std::cmp::Reverse(s.started_at_utc));
    let cur = rt.ledger_cursor.min(sorted.len().saturating_sub(1));
    let h = inner.height as usize;
    let start = if cur >= h { cur + 1 - h } else { 0 };
    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    for (i, s) in sorted.iter().enumerate().skip(start).take(h) {
        let sel = i == cur;
        let cat = rt.vault.categories.iter().find(|x| x.id == s.category_id);
        let col = cat.map(|x| p.category(&x.color_hex)).unwrap_or(p.dim);
        let label = if s.objective.is_empty() { cat.map(|x| x.name.clone()).unwrap_or_else(|| s.activity_kind.short_label().into()) } else { s.objective.clone() };
        let tail = format!(" {:>7} +{:<4}{}", ui::hm(s.elapsed_seconds), session_xp(s), if s.completion_state == CompletionState::Completed { "✔" } else { "✘" });
        let head = format!("{}{} {} {} ", if sel { "▸" } else { " " }, kind_glyph(s.activity_kind), &s.primary_date_key[5.min(s.primary_date_key.len())..], weekday_name(&s.primary_date_key));
        let bg = if sel { p.card_hi } else { p.card };
        let mid = ui::pad_right(&label, w.saturating_sub(head.chars().count() + tail.chars().count()));
        lines.push(Line::from(vec![
            Span::styled(head, Style::default().fg(if sel { p.accent } else { col }).bg(bg).add_modifier(Modifier::BOLD)),
            Span::styled(mid, Style::default().fg(p.text).bg(bg)),
            Span::styled(tail, Style::default().fg(if s.completion_state == CompletionState::Completed { p.good } else { p.warn }).bg(bg)),
        ]));
    }
    let hints = vec!["No sessions match these filters.".to_string(), "Press 2 or 3, then Space, to log your first.".to_string(), "Finished sessions appear here instantly.".to_string()];
    ui::fill_hints(&mut lines, inner.height, p, &hints);
    ui::text(f, inner, lines);
    if cols.len() < 2 { return; }

    // detail (+ a recent-days chart underneath when there is room)
    let detail_area = if cols[1].height >= 24 {
        let r = ui::rows(cols[1], &[Constraint::Length(13), Constraint::Min(8)]);
        let mark = sorted.get(cur).map(|s| s.primary_date_key.clone());
        super::recent_days_card(f, r[1], c, "Recent days", None, mark.as_deref());
        r[0]
    } else {
        cols[1]
    };
    let inner = ui::card(f, detail_area, p, "◎", "Session details", p.accent2, false);
    let Some(s) = sorted.get(cur) else {
        ui::center(f, inner, p, vec![Line::from(Span::styled("Select a session to see everything about it.", Style::default().fg(p.dim)))]);
        return;
    };
    let cat = rt.vault.categories.iter().find(|x| x.id == s.category_id);
    let kv = |k: &str, v: String, col: ratatui::style::Color| Line::from(vec![Span::styled(format!("{:<9}", k), Style::default().fg(p.dim)), Span::styled(v, Style::default().fg(col))]);
    let mut lines = vec![
        Line::from(Span::styled(if s.objective.is_empty() { s.activity_kind.label().to_string() } else { s.objective.clone() }, Style::default().fg(p.text).add_modifier(Modifier::BOLD))),
        kv("When", s.started_at_utc.with_timezone(&chrono::Local).format("%a %d %b %Y, %H:%M").to_string(), p.text),
        kv("Length", format!("{} of {} planned", ui::hm(s.elapsed_seconds), ui::hm(s.configured_seconds)), p.accent),
        kv("Skill", cat.map(|x| format!("{} {}", x.icon_glyph, x.name)).unwrap_or("-".into()), cat.map(|x| p.category(&x.color_hex)).unwrap_or(p.text)),
        kv("Result", s.completion_state.label().to_string(), if s.completion_state == CompletionState::Completed { p.good } else { p.warn }),
        kv("XP", format!("+{}", session_xp(s)), p.good),
    ];
    if let Some(m) = &s.reading_meta {
        lines.push(kv("Book", format!("{} — {}", m.title, m.author), p.accent2));
        lines.push(kv("Pages", format!("{} read", m.pages_read), p.text));
    }
    if s.activity_kind != ActivityKind::Focus && s.reading_meta.is_none() { lines.push(kv("Type", s.activity_kind.label().into(), p.text)); }
    if !s.completion_notes.is_empty() {
        lines.push(Line::from(""));
        lines.push(Line::from(Span::styled("Notes", Style::default().fg(p.accent3).add_modifier(Modifier::BOLD))));
        lines.push(Line::from(Span::styled(s.completion_notes.clone(), Style::default().fg(p.text))));
    }
    ui::fill_hints(&mut lines, inner.height, p, &["Notes you add when finishing show up here.".to_string(), "Press E to fix the time or skill.".to_string()]);
    ui::text(f, inner, lines);
}

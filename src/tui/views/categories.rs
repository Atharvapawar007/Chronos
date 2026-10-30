//! Skills (categories): skill cards on the left, full detail on the right.

use super::Ctx;
use crate::domain::CategoryRecord;
use crate::tui::ui;
use ratatui::{
    layout::Rect,
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let cols = ui::fit_cols(area, &[3, 2], 46);
    let visible: Vec<&CategoryRecord> = c.rt.vault.categories.iter().filter(|x| c.rt.show_archived_categories || !x.is_archived).collect();
    let cur = c.rt.category_cursor.min(visible.len().saturating_sub(1));
    list(f, cols[0], c, &visible, cur);
    if cols.len() > 1 {
        let sel = visible.get(cur).copied();
        if cols[1].height >= 24 {
            let r = ui::rows(cols[1], &[ratatui::layout::Constraint::Length(13), ratatui::layout::Constraint::Min(8)]);
            detail(f, r[0], c, sel);
            super::recent_days_card(f, r[1], c, "Last days · this skill", sel.map(|x| x.id.as_str()), None);
        } else {
            detail(f, cols[1], c, sel);
        }
    }
}

fn totals(c: &Ctx, id: &str) -> (u64, usize) {
    let mut s = 0u64;
    let mut n = 0usize;
    for x in c.rt.vault.sessions.iter().filter(|x| x.category_id == id && x.activity_kind.is_productive()) {
        s += x.elapsed_seconds;
        n += 1;
    }
    (s, n)
}

fn list(f: &mut Frame, area: Rect, c: &Ctx, visible: &[&CategoryRecord], cur: usize) {
    let p = &c.p;
    let title = if c.rt.show_archived_categories { "Skill classes (showing archived)" } else { "Skill classes" };
    let inner = ui::card(f, area, p, "◆", title, p.good, false);
    let min_h = 4u16;
    let fit_total = (inner.height / min_h).max(1) as usize;
    let show = visible.len().min(fit_total.saturating_sub(if visible.len() < fit_total { 1 } else { 0 }).max(1));
    let start = if cur >= show { cur + 1 - show } else { 0 };
    let with_add = show < fit_total;
    let slots = show + usize::from(with_add);
    let slot_rects = ui::rows(inner, &vec![ratatui::layout::Constraint::Fill(1); slots]);
    for (slot, (i, cat)) in visible.iter().enumerate().skip(start).take(show).enumerate() {
        let r = slot_rects[slot];
        let sel = i == cur;
        let col = p.category(&cat.color_hex);
        let blk = Block::default().borders(Borders::ALL).border_type(if sel { BorderType::Double } else { BorderType::Rounded }).border_style(Style::default().fg(if sel { col } else { p.border })).style(Style::default().bg(if sel { p.card_hi } else { p.card }));
        let bi = blk.inner(r);
        f.render_widget(blk, r);
        let (secs, n) = totals(c, &cat.id);
        let active_f = c.rt.prefs.focus.default_category_id == cat.id;
        let active_r = c.rt.prefs.reading.default_category_id == cat.id;
        let w = bi.width as usize;
        let mut tags = String::new();
        if active_f { tags.push_str(" ★FOCUS"); }
        if active_r { tags.push_str(" ★READ"); }
        if cat.is_archived { tags.push_str(" (archived)"); }
        let scope = format!(" [{}]", cat.scope.label());
        let name_w = w.saturating_sub(tags.chars().count() + scope.chars().count() + 2);
        let l1 = Line::from(vec![
            Span::styled(format!("{} ", cat.icon_glyph), Style::default().fg(col).add_modifier(Modifier::BOLD)),
            Span::styled(ui::pad_right(&cat.name, name_w), Style::default().fg(p.text).add_modifier(Modifier::BOLD)),
            Span::styled(tags, Style::default().fg(p.accent3).add_modifier(Modifier::BOLD)),
            Span::styled(scope, Style::default().fg(p.dim)),
        ]);
        let goal = c.goals.category_goals.iter().find(|g| g.category_id == cat.id);
        let pct = goal.map(|g| g.weekly.percent.min(100)).unwrap_or(0) as f64 / 100.0;
        let tail = format!(" {} total · {} sessions", ui::hm(secs), n);
        let mut l2 = Vec::new();
        l2.extend(ui::meter(w.saturating_sub(tail.chars().count()).max(4), pct, col, if sel { p.card } else { p.card_hi }));
        l2.push(Span::styled(tail, Style::default().fg(p.dim)));
        let top = bi.height.saturating_sub(2) / 2;
        ui::text_nowrap(f, Rect::new(bi.x, bi.y + top, bi.width, bi.height.saturating_sub(top)), vec![l1, Line::from(l2)]);
    }
    if with_add {
        let r = slot_rects[slots - 1];
        let blk = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(p.border));
        let bi = blk.inner(r);
        f.render_widget(blk, r);
        ui::center(f, bi, p, vec![
            Line::from(Span::styled("+ New skill class (N)", Style::default().fg(p.good).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled("Group your time: Coding, Math, Fiction…", Style::default().fg(p.dim))),
        ]);
    }
}

fn detail(f: &mut Frame, area: Rect, c: &Ctx, cat: Option<&CategoryRecord>) {
    let p = &c.p;
    let Some(cat) = cat else {
        let inner = ui::card(f, area, p, "◎", "Details", p.accent2, false);
        ui::center(f, inner, p, vec![Line::from(Span::styled("No skill selected. Press N to create one.", Style::default().fg(p.dim)))]);
        return;
    };
    let col = p.category(&cat.color_hex);
    let inner = ui::card(f, area, p, &cat.icon_glyph, &cat.name, col, false);
    let (secs, n) = totals(c, &cat.id);
    let w = inner.width as usize;
    let goal = c.goals.category_goals.iter().find(|g| g.category_id == cat.id);
    let mut lines = vec![
        Line::from(Span::styled(if cat.description.is_empty() { "No description. Press E to add one.".to_string() } else { cat.description.clone() }, Style::default().fg(p.text))),
        Line::from(""),
        Line::from(vec![Span::styled("Used for  ", Style::default().fg(p.dim)), Span::styled(cat.scope.label(), Style::default().fg(p.accent))]),
        Line::from(vec![Span::styled("All time  ", Style::default().fg(p.dim)), Span::styled(format!("{} in {} sessions", ui::hm(secs), n), Style::default().fg(p.text))]),
    ];
    if let Some(g) = goal {
        for (lab, m) in [("Today", &g.daily), ("Week ", &g.weekly)] {
            if m.enabled {
                let mut s = vec![Span::styled(format!("{}  ", lab), Style::default().fg(p.dim))];
                s.extend(ui::meter(w.saturating_sub(24).max(4), m.percent.min(100) as f64 / 100.0, if m.is_met { p.good } else { col }, p.card_hi));
                s.push(Span::styled(format!(" {}/{}", ui::hm(m.done_seconds), ui::hm(m.target_seconds)), Style::default().fg(p.dim)));
                lines.push(Line::from(s));
            } else {
                lines.push(Line::from(Span::styled(format!("{}  no goal (press E to set one)", lab), Style::default().fg(p.dim))));
            }
        }
    }
    let hints = vec![
        "S makes this the skill used for new sessions.".to_string(),
        "A archives it (hidden, history kept).".to_string(),
        "D deletes it (sessions move to another skill).".to_string(),
    ];
    ui::fill_hints(&mut lines, inner.height, p, &hints);
    ui::text(f, inner, lines);
}

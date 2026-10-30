//! Reading Deck: bookshelf of progress cards, reading timer and reading stats.

use super::focus::timer_card;
use super::Ctx;
use crate::domain::{ActivityKind, ClockStatus, ReadingStatus};
use crate::tui::ui;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    if area.width >= 100 {
        let cols = ui::fit_cols(area, &[3, 2], 44);
        shelf(f, cols[0], c);
        if cols.len() > 1 { right(f, cols[1], c); }
    } else {
        let r = ui::rows(area, &[Constraint::Length(if area.height >= 30 { 16 } else { 0 }), Constraint::Min(8)]);
        if r[0].height > 0 { timer_card(f, r[0], c, "▣", "Reading Timer"); }
        shelf(f, r[1], c);
    }
}

fn right(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let stats_h = 8u16;
    if area.height >= 14 + stats_h {
        let r = ui::rows(area, &[Constraint::Min(14), Constraint::Length(stats_h)]);
        timer_card(f, r[0], c, "▣", "Reading Timer");
        stats(f, r[1], c);
    } else {
        timer_card(f, area, c, "▣", "Reading Timer");
    }
    let _ = p;
}

fn stats(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let inner = ui::card(f, area, p, "◎", "Reading Stats", p.accent2, false);
    let g = &c.goals.daily_reading;
    let shelf = &c.rt.vault.reading_shelf;
    let pages: u32 = shelf.iter().map(|m| m.total_pages_read).sum();
    let secs: u64 = shelf.iter().map(|m| m.total_reading_seconds).sum();
    let pph = if secs >= 60 && pages > 0 { pages as f64 / (secs as f64 / 3600.0) } else { 0.0 };
    let count = |s: ReadingStatus| shelf.iter().filter(|m| m.status == s).count();
    let w = (inner.width as usize).saturating_sub(14).clamp(4, 30);
    let mut bar = vec![Span::styled("Goal ", Style::default().fg(p.dim))];
    bar.extend(ui::meter(w, g.percent.min(100) as f64 / 100.0, p.accent2, p.card_hi));
    bar.push(Span::styled(format!(" {}%", g.percent.min(999)), Style::default().fg(p.text)));
    let lines = vec![
        Line::from(vec![Span::styled(ui::hm(c.day.reading_seconds), Style::default().fg(p.accent2).add_modifier(Modifier::BOLD)), Span::styled(format!(" read today · {} pages", c.day.pages_read), Style::default().fg(p.dim))]),
        Line::from(bar),
        Line::from(Span::styled(format!("Speed {:.0} pages/hour · {} pages total", pph, pages), Style::default().fg(p.text))),
        Line::from(vec![
            Span::styled(format!("{} reading", count(ReadingStatus::Reading)), Style::default().fg(p.accent)),
            Span::styled(format!(" · {} have", count(ReadingStatus::Have)), Style::default().fg(p.info)),
            Span::styled(format!(" · {} read", count(ReadingStatus::Read)), Style::default().fg(p.good)),
            Span::styled(format!(" · {} abandoned", count(ReadingStatus::Abandoned)), Style::default().fg(p.warn)),
        ]),
        Line::from(Span::styled(format!("Reading streak {}d (best {}d)", c.streaks.reading.current_days, c.streaks.reading.best_days), Style::default().fg(p.accent3))),
    ];
    ui::center(f, inner, p, lines);
}

fn shelf(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let inner = ui::card(f, area, p, "▤", "Bookshelf  (Enter read · N add · E edit · S state · D delete)", p.accent2, false);
    let books = &c.rt.vault.reading_shelf;
    let min_h = 5u16;
    let cur = c.rt.reading_shelf_cursor.min(books.len().saturating_sub(1));
    // How many cards fit; one extra slot is kept for the "add a book" card when there is room.
    let fit_total = (inner.height / min_h).max(1) as usize;
    let show_books = books.len().min(fit_total.saturating_sub(if books.len() < fit_total { 1 } else { 0 }).max(1));
    let start = if cur >= show_books { cur + 1 - show_books } else { 0 };
    let with_add = show_books < fit_total;
    let slots = show_books + usize::from(with_add);
    let slot_rects = ui::rows(inner, &vec![Constraint::Fill(1); slots]);
    let active_id = if c.rt.clock.activity_kind == ActivityKind::Reading && c.rt.clock.status != ClockStatus::Idle { c.rt.clock.reading_material_id.clone() } else { None };

    for (slot, (i, m)) in books.iter().enumerate().skip(start).take(show_books).enumerate() {
        let r = slot_rects[slot];
        let sel = i == cur;
        let cat = c.rt.vault.categories.iter().find(|x| x.id == m.category_id);
        let col = cat.map(|x| p.category(&x.color_hex)).unwrap_or(p.accent2);
        let live = active_id.as_deref() == Some(m.id.as_str());
        let block = Block::default()
            .borders(Borders::ALL)
            .border_type(if sel { BorderType::Double } else { BorderType::Rounded })
            .border_style(Style::default().fg(if sel { p.accent2 } else { p.border }))
            .style(Style::default().bg(if sel { p.card_hi } else { p.card }));
        let bi = block.inner(r);
        f.render_widget(block, r);
        let w = bi.width as usize;
        let status_col = match m.status { ReadingStatus::Read => p.good, ReadingStatus::Reading => p.accent, ReadingStatus::Have => p.info, ReadingStatus::Abandoned => p.warn };
        let tag = format!(" {} ", if live { "READING NOW" } else { m.status.label() });
        let avail = w.saturating_sub(2 + tag.chars().count() + 1);
        let author = ui::clamp_str(&m.author, (avail / 3).max(6));
        let title = ui::clamp_str(&m.title, avail.saturating_sub(author.chars().count() + 2));
        let mut l1 = vec![
            Span::styled(format!("{} ", if live { "●" } else { "▣" }), Style::default().fg(col).add_modifier(Modifier::BOLD)),
            Span::styled(title, Style::default().fg(p.text).add_modifier(Modifier::BOLD)),
            Span::styled(format!("  {}", author), Style::default().fg(p.dim)),
        ];
        let used: usize = l1.iter().map(|s| s.content.chars().count()).sum();
        l1.push(Span::raw(" ".repeat(w.saturating_sub(used + tag.chars().count()))));
        l1.push(Span::styled(tag, Style::default().fg(p.on_accent).bg(if live { p.good } else { status_col }).add_modifier(Modifier::BOLD)));
        let page_txt = format!(" p.{}/{} {}%", m.current_page, m.total_pages, m.progress_percent());
        let mut l2 = Vec::new();
        l2.extend(ui::meter(w.saturating_sub(page_txt.chars().count()).max(6), m.progress_percent() as f64 / 100.0, col, if sel { p.card } else { p.card_hi }));
        l2.push(Span::styled(page_txt, Style::default().fg(p.text)));
        let stats = format!("{} read · {:.0} pages/hour · {} sessions · {}", ui::hm(m.total_reading_seconds), m.pages_per_hour(), m.completed_sessions_count, cat.map(|x| x.name.as_str()).unwrap_or("-"));
        let l3 = Line::from(Span::styled(ui::clamp_str(&stats, w), Style::default().fg(p.dim)));
        let content = vec![Line::from(l1), Line::from(l2), l3];
        let top = (bi.height.saturating_sub(3)) / 2;
        let body = Rect::new(bi.x, bi.y + top, bi.width, bi.height.saturating_sub(top));
        ui::text_nowrap(f, body, content);
    }

    if with_add {
        let r = slot_rects[slots - 1];
        let blk = Block::default().borders(Borders::ALL).border_type(BorderType::Rounded).border_style(Style::default().fg(p.border));
        let bi = blk.inner(r);
        f.render_widget(blk, r);
        let lines = if books.is_empty() {
            vec![Line::from(Span::styled("Your shelf is empty", Style::default().fg(p.text).add_modifier(Modifier::BOLD))), Line::from(Span::styled("Press N to add your first book", Style::default().fg(p.accent2)))]
        } else {
            vec![Line::from(Span::styled("+ Add another book (N)", Style::default().fg(p.accent2).add_modifier(Modifier::BOLD))), Line::from(Span::styled("Tip: 20 pages a day ≈ 1 book per month", Style::default().fg(p.dim)))]
        };
        ui::center(f, bi, p, lines);
    }
}

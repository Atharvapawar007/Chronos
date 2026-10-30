//! Small drawing toolkit shared by every screen.

use super::theme::Palette;
use ratatui::{
    layout::{Alignment, Constraint, Direction, Layout, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{Block, BorderType, Borders, Clear, Paragraph, Wrap},
    Frame,
};

pub const EIGHTHS: [&str; 9] = [" ", "▏", "▎", "▍", "▌", "▋", "▊", "▉", "█"];
pub const SPARK: [&str; 8] = ["▁", "▂", "▃", "▄", "▅", "▆", "▇", "█"];

pub fn bold(p: &Palette, color: ratatui::style::Color) -> Style {
    let _ = p;
    Style::default().fg(color).add_modifier(Modifier::BOLD)
}

/// A rounded card. Draw it, then draw content in the returned inner area.
pub fn card(f: &mut Frame, area: Rect, p: &Palette, icon: &str, title: &str, accent: ratatui::style::Color, focused: bool) -> Rect {
    let border = if focused { accent } else { p.border };
    let block = Block::default()
        .borders(Borders::ALL)
        .border_type(if focused { BorderType::Double } else { BorderType::Rounded })
        .border_style(Style::default().fg(border))
        .style(Style::default().bg(p.card).fg(p.text))
        .title(Line::from(vec![
            Span::styled(format!(" {} ", icon), Style::default().fg(accent).add_modifier(Modifier::BOLD)),
            Span::styled(format!("{} ", title), Style::default().fg(p.text).add_modifier(Modifier::BOLD)),
        ]));
    let inner = block.inner(area);
    f.render_widget(Clear, area);
    f.render_widget(block, area);
    // one column of breathing room inside wide cards
    if inner.width > 8 {
        Rect::new(inner.x + 1, inner.y, inner.width - 2, inner.height)
    } else {
        inner
    }
}

/// Paints the whole area with the page background.
pub fn paint_bg(f: &mut Frame, area: Rect, p: &Palette) {
    f.render_widget(Block::default().style(Style::default().bg(p.bg).fg(p.text)), area);
}

/// Centres a block of lines both ways inside `area` (flexbox-style `place-items: center`).
pub fn center(f: &mut Frame, area: Rect, p: &Palette, lines: Vec<Line<'static>>) {
    let h = lines.len() as u16;
    let y = area.y + area.height.saturating_sub(h) / 2;
    let r = Rect::new(area.x, y, area.width, h.min(area.height));
    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center).style(Style::default().bg(p.card)),
        r,
    );
}

pub fn text(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    f.render_widget(Paragraph::new(lines).wrap(Wrap { trim: false }), area);
}

pub fn hm(secs: u64) -> String {
    let (h, m) = (secs / 3600, (secs % 3600) / 60);
    if h > 0 { format!("{}h {:02}m", h, m) } else { format!("{}m", m) }
}

pub fn hms(secs: u64) -> String {
    format!("{:02}:{:02}", secs / 60, secs % 60)
}

pub fn clamp_str(s: &str, max: usize) -> String {
    if max == 0 {
        return String::new();
    }
    let n = s.chars().count();
    if n <= max {
        s.to_string()
    } else {
        let mut o: String = s.chars().take(max.saturating_sub(1)).collect();
        o.push('…');
        o
    }
}

pub fn pad_right(s: &str, w: usize) -> String {
    let n = s.chars().count();
    if n >= w { clamp_str(s, w) } else { format!("{}{}", s, " ".repeat(w - n)) }
}

/// A horizontal meter with 1/8-cell precision. `frac` is 0.0..=1.0.
pub fn meter(width: usize, frac: f64, fg: ratatui::style::Color, bg: ratatui::style::Color) -> Vec<Span<'static>> {
    let total = (frac.clamp(0.0, 1.0) * width as f64 * 8.0).round() as usize;
    let full = total / 8;
    let part = total % 8;
    let mut s = "█".repeat(full.min(width));
    let mut used = full.min(width);
    if part > 0 && used < width {
        s.push_str(EIGHTHS[part]);
        used += 1;
    }
    vec![
        Span::styled(s, Style::default().fg(fg).bg(bg)),
        Span::styled(" ".repeat(width.saturating_sub(used)), Style::default().bg(bg)),
    ]
}

pub fn spark(values: &[u64], max: u64) -> String {
    let max = max.max(1);
    values
        .iter()
        .map(|v| {
            if *v == 0 { "·" } else { SPARK[((*v * 7 / max) as usize).min(7)] }
        })
        .collect()
}

pub fn chip(p: &Palette, key: &str, label: &str) -> Vec<Span<'static>> {
    vec![
        Span::styled(format!(" {} ", key), Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD)),
        Span::styled(format!(" {}  ", label), Style::default().fg(p.dim)),
    ]
}

pub fn pill(text: &str, fg: ratatui::style::Color, bg: ratatui::style::Color) -> Span<'static> {
    Span::styled(format!(" {} ", text), Style::default().fg(fg).bg(bg).add_modifier(Modifier::BOLD))
}

pub fn pct_color(p: &Palette, pct: f64) -> ratatui::style::Color {
    if pct >= 100.0 { p.good } else if pct >= 50.0 { p.accent } else if pct > 0.0 { p.warn } else { p.dim }
}

/// Splits `area` into up to `weights.len()` columns, dropping trailing cards that cannot
/// reach `min_w`. The kept cards share the full width, so nothing is left empty.
pub fn fit_cols(area: Rect, weights: &[u16], min_w: u16) -> Vec<Rect> {
    let mut n = weights.len();
    while n > 1 && (area.width / n as u16) < min_w {
        n -= 1;
    }
    let cs: Vec<Constraint> = weights[..n].iter().map(|w| Constraint::Fill(*w)).collect();
    Layout::default().direction(Direction::Horizontal).constraints(cs).split(area).to_vec()
}

pub fn rows(area: Rect, cs: &[Constraint]) -> Vec<Rect> {
    Layout::default().direction(Direction::Vertical).constraints(cs.to_vec()).split(area).to_vec()
}

/// Vertical bar chart that stretches to fill the whole area (bars grow in width and height).
/// Items: (label, value, colour). `mark` highlights one bar.
pub fn vbars(f: &mut Frame, area: Rect, p: &Palette, items: &[(String, u64, ratatui::style::Color)], mark: Option<usize>, show_values: bool) {
    if area.width < 2 || area.height < 3 || items.is_empty() {
        return;
    }
    let n = items.len() as u16;
    let gap: u16 = if area.width >= n * 3 { 1 } else { 0 };
    let bw = ((area.width.saturating_sub(gap * (n - 1))) / n).max(1);
    let used = bw * n + gap * (n - 1);
    let x0 = area.x + area.width.saturating_sub(used) / 2;
    let top_off: u16 = if show_values && area.height > 4 { 1 } else { 0 };
    let chart_h = area.height - 1 - top_off; // last row = labels, first row = values
    let label_y = area.y + area.height - 1;
    let max = items.iter().map(|i| i.1).max().unwrap_or(1).max(1);
    let buf = f.buffer_mut();
    for (i, (label, v, col)) in items.iter().enumerate() {
        let x = x0 + i as u16 * (bw + gap);
        let eighths = (*v as f64 / max as f64 * chart_h as f64 * 8.0).round() as u16;
        let color = if Some(i) == mark { p.accent3 } else { *col };
        for row in 0..chart_h {
            let from_bottom = chart_h - 1 - row;
            let cell_fill = eighths.saturating_sub(from_bottom * 8).min(8);
            let glyph = match cell_fill {
                0 => if *v == 0 && from_bottom == 0 { "▁" } else { " " },
                8 => "█",
                k => ["", "▁", "▂", "▃", "▄", "▅", "▆", "▇"][k as usize],
            };
            let fg = if glyph == "▁" && *v == 0 { p.border } else { color };
            for dx in 0..bw {
                if x + dx < area.x + area.width {
                    let cell = buf.get_mut(x + dx, area.y + top_off + row);
                    cell.set_symbol(glyph);
                    cell.set_style(Style::default().fg(fg).bg(p.card));
                }
            }
        }
        if top_off == 1 && *v > 0 {
            let txt = hm(*v);
            let tw = txt.chars().count() as u16;
            if tw <= bw {
                let bar_rows = eighths.div_ceil(8).min(chart_h);
                let vy = area.y + top_off + (chart_h - bar_rows);
                let vy = vy.saturating_sub(1).max(area.y);
                let vx = x + (bw - tw) / 2;
                for (k, ch) in txt.chars().enumerate() {
                    let cell = buf.get_mut(vx + k as u16, vy);
                    cell.set_symbol(&ch.to_string());
                    cell.set_style(Style::default().fg(if Some(i) == mark { p.accent3 } else { p.text }).bg(p.card));
                }
            }
        }
        let l = clamp_str(label, bw as usize);
        let lx = x + bw.saturating_sub(l.chars().count() as u16) / 2;
        for (k, ch) in l.chars().enumerate() {
            let cx = lx + k as u16;
            if cx < area.x + area.width {
                let cell = buf.get_mut(cx, label_y);
                cell.set_symbol(&ch.to_string());
                cell.set_style(Style::default().fg(if Some(i) == mark { p.accent3 } else { p.dim }).bg(p.card));
            }
        }
    }
}

/// A "stat tile": small caption, big value, optional footer. Centred in its card.
pub fn tile(f: &mut Frame, area: Rect, p: &Palette, icon: &str, caption: &str, value: String, foot: String, color: ratatui::style::Color) {
    let inner = card(f, area, p, icon, caption, color, false);
    center(
        f,
        inner,
        p,
        vec![
            Line::from(Span::styled(value, Style::default().fg(color).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(foot, Style::default().fg(p.dim))),
        ],
    );
}

/// Fills leftover rows of a list with helpful "coming up" hints so a card never ends in blank space.
pub fn fill_hints(lines: &mut Vec<Line<'static>>, height: u16, p: &Palette, hints: &[String]) {
    let mut i = 0;
    while (lines.len() as u16) < height && i < hints.len() {
        lines.push(Line::from(Span::styled(hints[i].clone(), Style::default().fg(p.dim))));
        i += 1;
    }
}

/// Splits `area` into `n` equal columns.
pub fn rows_h(area: Rect, n: usize) -> Vec<Rect> {
    Layout::default().direction(Direction::Horizontal).constraints(vec![Constraint::Fill(1); n.max(1)]).split(area).to_vec()
}

/// Like [`text`] but never wraps (long lines are clipped by the caller).
pub fn text_nowrap(f: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    f.render_widget(Paragraph::new(lines), area);
}

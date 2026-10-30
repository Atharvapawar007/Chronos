//! The circular timer: a braille-drawn ring plus a big-digit clock, used by Focus, Reading and Zen.

use super::theme::Palette;
use ratatui::{
    layout::{Alignment, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::{
        canvas::{Canvas, Points},
        Paragraph,
    },
    symbols::Marker,
    Frame,
};

const DIGITS: [[&str; 5]; 11] = [
    ["███", "█ █", "█ █", "█ █", "███"],
    ["  █", "  █", "  █", "  █", "  █"],
    ["███", "  █", "███", "█  ", "███"],
    ["███", "  █", "███", "  █", "███"],
    ["█ █", "█ █", "███", "  █", "  █"],
    ["███", "█  ", "███", "  █", "███"],
    ["███", "█  ", "███", "█ █", "███"],
    ["███", "  █", "  █", "  █", "  █"],
    ["███", "█ █", "███", "█ █", "███"],
    ["███", "█ █", "███", "  █", "███"],
    [" ", "█", " ", "█", " "],
];

/// Renders "MM:SS" (or "H:MM:SS") as 5-row block digits.
pub fn big_digits(text: &str) -> Vec<String> {
    let mut rows = vec![String::new(); 5];
    for ch in text.chars() {
        let idx = match ch {
            '0'..='9' => ch as usize - '0' as usize,
            _ => 10,
        };
        for (r, row) in rows.iter_mut().enumerate() {
            row.push_str(DIGITS[idx][r]);
            row.push(' ');
        }
    }
    rows.iter().map(|r| r.trim_end().to_string()).collect()
}

pub fn digits_width(text: &str) -> u16 {
    big_digits(text)[0].chars().count() as u16
}

/// Block digits enlarged `scale` times in both directions.
pub fn big_digits_scaled(text: &str, scale: usize) -> Vec<String> {
    let mut out = Vec::new();
    for row in big_digits(text) {
        let wide: String = row.chars().flat_map(|ch| std::iter::repeat_n(ch, scale)).collect();
        for _ in 0..scale {
            out.push(wide.clone());
        }
    }
    out
}

/// Draws the ring filling `area`. `frac` is the elapsed fraction (0..1). `tick` animates the tip.
pub fn draw_ring(f: &mut Frame, area: Rect, p: &Palette, frac: f64, color: ratatui::style::Color, tick: u64, animate: bool, bg: ratatui::style::Color) {
    if area.width < 6 || area.height < 3 {
        return;
    }
    // Braille dots are 2 wide x 4 tall per cell, so these bounds make a round circle.
    let (w, h) = (area.width as f64 * 2.0, area.height as f64 * 4.0);
    let radius = (w.min(h) / 2.0 - 2.0).max(2.0);
    let frac = frac.clamp(0.0, 1.0);
    let track = p.border;
    let thick = (radius / 9.0).clamp(1.0, 3.0);
    let canvas = Canvas::default()
        .background_color(bg)
        .marker(Marker::Braille)
        .x_bounds([-w / 2.0, w / 2.0])
        .y_bounds([-h / 2.0, h / 2.0])
        .paint(move |ctx| {
            let steps = (radius * 8.0) as usize;
            let mut track_pts = Vec::new();
            let mut arc_pts = Vec::new();
            for i in 0..steps {
                let t = i as f64 / steps as f64;
                let ang = std::f64::consts::FRAC_PI_2 - t * std::f64::consts::TAU;
                let mut r = radius;
                while r > radius - thick {
                    let pt = (r * ang.cos(), r * ang.sin());
                    if t <= frac { arc_pts.push(pt) } else { track_pts.push(pt) }
                    r -= 0.6;
                }
            }
            ctx.draw(&Points { coords: &track_pts, color: track });
            ctx.draw(&Points { coords: &arc_pts, color });
            if animate && frac > 0.0 && frac < 1.0 {
                let ang = std::f64::consts::FRAC_PI_2 - frac * std::f64::consts::TAU;
                let pulse = if tick % 20 < 10 { 2.2 } else { 1.2 };
                let mut tip = Vec::new();
                let mut dr = -pulse;
                while dr <= pulse {
                    let mut da = -pulse;
                    while da <= pulse {
                        tip.push(((radius - thick / 2.0 + dr) * ang.cos() - da * ang.sin(), (radius - thick / 2.0 + dr) * ang.sin() + da * ang.cos()));
                        da += 0.7;
                    }
                    dr += 0.7;
                }
                ctx.draw(&Points { coords: &tip, color: p.text });
            }
        });
    f.render_widget(canvas, area);
}

/// Time text for the middle of a ring: big digits when they fit, otherwise one bold line.
pub fn clock_lines(p: &Palette, time: &str, color: ratatui::style::Color, inner_w: u16, inner_h: u16) -> Vec<Line<'static>> {
    let scale = [3usize, 2, 1].into_iter().find(|s| inner_h as usize >= 5 * s + 4 && digits_width(time) as usize * s + 2 <= inner_w as usize);
    if let Some(scale) = scale {
        big_digits_scaled(time, scale)
            .into_iter()
            .map(|r| Line::from(Span::styled(r, Style::default().fg(color).add_modifier(Modifier::BOLD))))
            .collect()
    } else {
        let _ = p;
        vec![Line::from(Span::styled(time.to_string(), Style::default().fg(color).add_modifier(Modifier::BOLD)))]
    }
}

/// Ring with the clock in the middle and optional extra lines under the digits.
pub fn ring_with_clock(f: &mut Frame, area: Rect, p: &Palette, frac: f64, time: &str, color: ratatui::style::Color, extra: Vec<Line<'static>>, tick: u64, animate: bool, bg: ratatui::style::Color) {
    draw_ring(f, area, p, frac, color, tick, animate, bg);
    let inner_w = (area.width as f64 * 0.62) as u16;
    let inner_h = (area.height as f64 * 0.6) as u16;
    let mut lines = clock_lines(p, time, color, inner_w, inner_h);
    let room = inner_h.saturating_sub(lines.len() as u16 + 1) as usize;
    if room > 0 {
        lines.push(Line::from(""));
        lines.extend(extra.into_iter().take(room.saturating_sub(1)));
    }
    let h = lines.len() as u16;
    let y = area.y + area.height.saturating_sub(h) / 2;
    f.render_widget(
        Paragraph::new(lines).alignment(Alignment::Center).style(Style::default().bg(bg)),
        Rect::new(area.x + 2, y, area.width.saturating_sub(4), h.min(area.height)),
    );
}

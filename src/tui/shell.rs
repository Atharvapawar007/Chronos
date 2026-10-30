//! Frame layout: header, tab strip, body, footer, zen mode and the modal layer.

use super::modals::render_modal;
use super::ring::ring_with_clock;
use super::theme::Palette;
use super::ui::{self, chip, pill};
use super::views::{self, Ctx};
use crate::domain::{ActivityKind, ClockStatus};
use crate::runtime::{PrimaryRoute, WorkspaceRuntime};
use ratatui::{
    layout::{Alignment, Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    widgets::Paragraph,
    Frame,
};

pub fn render_workspace_frame(f: &mut Frame, rt: &WorkspaceRuntime) {
    let area = f.size();
    let ctx = Ctx::new(rt);
    ui::paint_bg(f, area, &ctx.p);

    let timer_route = matches!(rt.route, PrimaryRoute::FocusStudio | PrimaryRoute::ReadingSanctuary);
    if rt.zen_mode && timer_route {
        render_zen(f, area, &ctx);
    } else if area.width >= 20 && area.height >= 6 {
        let r = ui::rows(area, &[Constraint::Length(1), Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)]);
        render_header(f, r[0], &ctx);
        render_tabs(f, r[1], &ctx);
        render_body(f, r[2], &ctx);
        render_footer(f, r[3], &ctx);
    } else {
        // Tiny terminal: just the time.
        let t = ui::hms(ctx.snap.remaining_seconds);
        ui::center(f, area, &ctx.p, vec![Line::from(Span::styled(t, Style::default().fg(ctx.p.accent).add_modifier(Modifier::BOLD)))]);
    }

    if rt.modal.is_some() {
        render_modal(f, area, &ctx);
    }
}

fn render_body(f: &mut Frame, area: Rect, c: &Ctx) {
    match c.rt.route {
        PrimaryRoute::Overview => views::home::render(f, area, c),
        PrimaryRoute::FocusStudio => views::focus::render(f, area, c),
        PrimaryRoute::ReadingSanctuary => views::reading::render(f, area, c),
        PrimaryRoute::Analytics => views::analytics::render(f, area, c),
        PrimaryRoute::SessionLedger => views::history::render(f, area, c),
        PrimaryRoute::AchievementsHall => views::trophies::render(f, area, c),
        PrimaryRoute::CategoryStudio => views::categories::render(f, area, c),
        PrimaryRoute::SettingsCenter => views::settings::render(f, area, c),
        PrimaryRoute::Guide => views::guide::render(f, area, c),
    }
}

/// Zen mode: only the ring and the time. Nothing else is drawn.
fn render_zen(f: &mut Frame, area: Rect, c: &Ctx) {
    let size = area.width.min(area.height * 2).max(8);
    let w = size.min(area.width);
    let h = (size / 2).min(area.height).max(3);
    let black = c.p.pure_black();
    f.render_widget(ratatui::widgets::Block::default().style(Style::default().bg(black)), area);
    let ring = Rect::new(area.x + (area.width - w) / 2, area.y + (area.height - h) / 2, w, h);
    let running = c.rt.clock.status == ClockStatus::Running;
    let frac = c.snap.progress_permille as f64 / 1000.0;
    let color = if c.rt.clock.status == ClockStatus::Paused { c.p.warn } else { c.running_color() };
    ring_with_clock(f, ring, &c.p, frac, &ui::hms(c.snap.remaining_seconds), color, vec![], c.tick, running && c.animate, black);
}

fn render_header(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let mut left = vec![pill("◈ CHRONOS", p.on_accent, p.accent), Span::raw(" ")];
    if c.rt.clock.status != ClockStatus::Idle {
        let k = match c.rt.clock.activity_kind {
            ActivityKind::Focus => "FOCUS",
            ActivityKind::Reading => "READ",
            _ => "BREAK",
        };
        let dot = if c.rt.clock.status == ClockStatus::Running { "●" } else { "❚❚" };
        left.push(pill(&format!("{} {} {}", dot, k, ui::hms(c.snap.remaining_seconds)), p.on_accent, c.running_color()));
        left.push(Span::raw(" "));
    }
    let streak = c.streaks.combined.current_days;
    let mut right = vec![
        Span::styled(format!("ϟ{}d ", streak), Style::default().fg(if c.streak_at_risk() { p.warn } else { p.accent3 }).add_modifier(Modifier::BOLD)),
        Span::styled(format!("Lv {} ", c.lvl.level), Style::default().fg(p.accent2).add_modifier(Modifier::BOLD)),
    ];
    let bw = if area.width > 90 { 10 } else { 6 };
    right.extend(ui::meter(bw, c.lvl.progress_percent as f64 / 100.0, p.accent2, p.card_hi));
    right.push(Span::styled(format!(" {} XP ", c.lvl.total_xp), Style::default().fg(p.dim)));
    let rw: usize = right.iter().map(|s| s.content.chars().count()).sum();
    let lw: usize = left.iter().map(|s| s.content.chars().count()).sum();
    let gap = (area.width as usize).saturating_sub(lw + rw);
    let mut spans = left;
    spans.push(Span::raw(" ".repeat(gap)));
    spans.extend(right);
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn render_tabs(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let wide = area.width as usize >= PrimaryRoute::ALL.iter().map(|r| r.label().len() + 5).sum::<usize>();
    let mut spans = Vec::new();
    for r in PrimaryRoute::ALL {
        let on = r == c.rt.route;
        let label = if wide || on { format!(" {} {} ", r.key_num(), r.label()) } else { format!(" {} ", r.key_num()) };
        let st = if on {
            Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD)
        } else {
            Style::default().fg(p.dim).bg(p.card)
        };
        spans.push(Span::styled(label, st));
        spans.push(Span::styled(" ", Style::default().bg(p.bg)));
    }
    f.render_widget(Paragraph::new(Line::from(spans)).alignment(Alignment::Center), area);
}

fn render_footer(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    if let Some(t) = c.rt.active_toast() {
        let line = Line::from(vec![pill("★", p.on_accent, p.accent3), Span::styled(format!(" {}", t), Style::default().fg(p.text).add_modifier(Modifier::BOLD))]);
        f.render_widget(Paragraph::new(line), area);
        return;
    }
    let keys: &[(&str, &str)] = match c.rt.route {
        PrimaryRoute::Overview => &[("Space", "Start/Pause"), ("↑↓", "Queue"), ("Enter", "Launch"), ("N", "New quest"), ("←→", "Heatmap"), ("T", "Theme"), ("?", "Guide")],
        PrimaryRoute::FocusStudio => &[("Space", "Start/Pause"), ("Enter", "Finish"), ("Z", "Zen"), ("M", "Cadence"), ("C", "Skill"), ("B/L", "Break"), ("+/-", "±5m"), ("I", "Goal")],
        PrimaryRoute::ReadingSanctuary => &[("Space", "Start/Pause"), ("↑↓", "Shelf"), ("Enter", "Read/Finish"), ("N", "Add"), ("E", "Edit"), ("S", "State"), ("D", "Delete"), ("Z", "Zen"), ("C", "Skill")],
        PrimaryRoute::Analytics => &[("Tab", "Lens"), ("[ ]", "Range"), ("C", "Skill filter"), ("←→↑↓", "Pick day"), ("X", "CSV"), ("O", "JSON")],
        PrimaryRoute::SessionLedger => &[("↑↓", "Move"), ("/", "Search"), ("F", "Type"), ("S", "Status"), ("C", "Skill"), ("E", "Edit"), ("D", "Delete"), ("X", "CSV")],
        PrimaryRoute::AchievementsHall => &[("←→↑↓", "Browse"), ("Tab", "Filter family")],
        PrimaryRoute::CategoryStudio => &[("↑↓", "Move"), ("N", "New"), ("E", "Edit"), ("S", "Use"), ("A", "Archive"), ("V", "Archived"), ("D", "Delete")],
        PrimaryRoute::SettingsCenter => &[("Tab", "Group"), ("↑↓", "Row"), ("←→", "Change"), ("B", "Backup"), ("L", "Restore")],
        PrimaryRoute::Guide => &[("Tab/←→", "Topic"), ("↑↓", "Scroll"), ("PgUp/PgDn", "Fast scroll")],
    };
    let mut spans = Vec::new();
    let mut used = 0usize;
    let quit = chip(p, "Q", "Quit");
    let quit_w: usize = quit.iter().map(|s| s.content.chars().count()).sum();
    for (k, l) in keys {
        let c = chip(p, k, l);
        let w: usize = c.iter().map(|s| s.content.chars().count()).sum();
        if used + w + quit_w > area.width as usize { break; }
        used += w;
        spans.extend(c);
    }
    spans.extend(quit);
    f.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn dim(p: &Palette) -> Style {
    Style::default().fg(p.dim)
}

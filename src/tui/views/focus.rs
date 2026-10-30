//! Focus Arena: the big ring, the mission card, today's progress and the cadence cycle.

use super::{session_xp, Ctx};
use crate::domain::{ActivityKind, ClockStatus};
use crate::tui::ring::ring_with_clock;
use crate::tui::ui;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    Frame,
};

const TIPS: [&str; 12] = [
    "Put your phone in another room before you press Space.",
    "Write the goal first (press I). A named target beats a vague hour.",
    "Stuck? Shrink the task until starting feels easy.",
    "Breaks are part of the system. Take them fully.",
    "Zen mode (Z) hides everything except the ring.",
    "One finished session today keeps tomorrow's streak alive.",
    "Start with the hardest task while your energy is fresh.",
    "Keep a notepad nearby. Park stray thoughts, then return to work.",
    "Drink water before you start so you don't need to leave mid-session.",
    "Finish what you started, even if it feels slow. Slow is still progress.",
    "Check Stats to find your golden hour and plan there.",
    "A 5-minute start is allowed. Often you'll just keep going.",
];

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    if area.width >= 96 {
        let cols = ui::fit_cols(area, &[3, 2], 40);
        timer_card(f, cols[0], c, "◉", "Focus Arena");
        if cols.len() > 1 { side_cards(f, cols[1], c); }
    } else {
        let r = ui::rows(area, &[Constraint::Min(14), Constraint::Length(if area.height >= 28 { 10 } else { 0 })]);
        timer_card(f, r[0], c, "◉", "Focus Arena");
        if r[1].height > 0 { side_cards(f, r[1], c); }
    }
}

/// Ring + status line + hint. Shared by Focus and Reading.
pub fn timer_card(f: &mut Frame, area: Rect, c: &Ctx, icon: &str, title: &str) {
    let p = &c.p;
    let clock = &c.rt.clock;
    let running = clock.status == ClockStatus::Running;
    // While idle, the screen decides what the ring is for (Reading screen → reading).
    let kind = if clock.status == ClockStatus::Idle {
        match (c.rt.route, clock.activity_kind) {
            (crate::runtime::PrimaryRoute::ReadingSanctuary, _) => ActivityKind::Reading,
            (crate::runtime::PrimaryRoute::FocusStudio, ActivityKind::Reading) => ActivityKind::Focus,
            (_, k) => k,
        }
    } else {
        clock.activity_kind
    };
    let color = if clock.status == ClockStatus::Paused { p.warn } else { match kind { ActivityKind::Focus => p.accent, ActivityKind::Reading => p.accent2, _ => p.good } };
    let inner = ui::card(f, area, p, icon, title, color, running);
    let parts = ui::rows(inner, &[Constraint::Min(4), Constraint::Length(3)]);

    // ring box: as large as fits, kept roughly circular (cells are ~2x taller than wide)
    let rw = parts[0].width.min(parts[0].height * 2).max(6);
    let rh = (rw / 2).min(parts[0].height).max(3);
    let ring = Rect::new(parts[0].x + (parts[0].width.saturating_sub(rw)) / 2, parts[0].y + (parts[0].height.saturating_sub(rh)) / 2, rw.min(parts[0].width), rh);
    let frac = c.snap.progress_permille as f64 / 1000.0;
    let phase = match kind {
        ActivityKind::Focus => "FOCUS",
        ActivityKind::Reading => "READING",
        ActivityKind::ShortBreak => "SHORT BREAK",
        ActivityKind::LongBreak => "LONG BREAK",
    };
    let extra = vec![
        Line::from(Span::styled(phase, Style::default().fg(color).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(format!("{}% done", c.snap.progress_permille / 10), Style::default().fg(p.dim))),
    ];
    ring_with_clock(f, ring, p, frac, &ui::hms(c.snap.remaining_seconds), color, extra, c.tick, running && c.animate, p.card);

    // status + hint
    let cat = c.rt.vault.categories.iter().find(|x| x.id == clock.category_id);
    let status_col = match clock.status {
        ClockStatus::Running => p.good,
        ClockStatus::Paused => p.warn,
        ClockStatus::AwaitingCompletion => p.accent3,
        ClockStatus::Idle => p.dim,
    };
    let hint = match clock.status {
        ClockStatus::Idle => "Press Space to start",
        ClockStatus::Running => "Space pauses · Enter finishes & logs it",
        ClockStatus::Paused => "Space resumes · R abandons the session",
        ClockStatus::AwaitingCompletion => "Time's up! Press Enter to log it",
    };
    let l = vec![
        Line::from(vec![
            ui::pill(clock.status.label(), p.on_accent, status_col),
            Span::styled(format!("  {} {}", cat.map(|x| x.icon_glyph.as_str()).unwrap_or("◆"), cat.map(|x| x.name.as_str()).unwrap_or("No skill")), Style::default().fg(cat.map(|x| p.category(&x.color_hex)).unwrap_or(p.text))),
            Span::styled(
                if kind == ActivityKind::Reading { format!("  ·  {} min session (+/- to change)", c.snap.target_seconds / 60) } else { format!("  ·  {}", clock.cadence.short_name()) },
                Style::default().fg(p.dim),
            ),
        ]),
        Line::from(Span::styled(hint, Style::default().fg(p.text))),
        Line::from(Span::styled(
            if clock.objective.is_empty() { "Goal: (none — press I to set one)".to_string() } else { format!("Goal: {}", clock.objective) },
            Style::default().fg(p.dim),
        )),
    ];
    ui::center(f, parts[1], p, l);
}

fn side_cards(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let r = ui::rows(area, &[Constraint::Length(9), Constraint::Length(8), Constraint::Min(5)]);
    let (show_a, show_b) = (area.height >= 9 + 8 + 5, area.height >= 9 + 5);
    let parts: Vec<Rect> = if show_a { r } else if show_b { ui::rows(area, &[Constraint::Length(9), Constraint::Min(5)]) } else { vec![area] };

    // Today's progress
    let inner = ui::card(f, parts[0], p, "★", "Today's Progress", p.accent3, false);
    let g = &c.goals.daily_focus;
    let w = (inner.width as usize).saturating_sub(14).clamp(4, 30);
    let mut bar = vec![Span::styled("Goal  ", Style::default().fg(p.dim))];
    bar.extend(ui::meter(w, g.percent.min(100) as f64 / 100.0, ui::pct_color(p, g.percent as f64), p.card_hi));
    bar.push(Span::styled(format!(" {}%", g.percent.min(999)), Style::default().fg(p.text)));
    let xp_today: u64 = c.rt.vault.sessions.iter().filter(|s| s.primary_date_key == c.today).map(session_xp).sum();
    let mut lines = vec![
        Line::from(vec![Span::styled(ui::hm(c.day.focus_seconds), Style::default().fg(p.accent).add_modifier(Modifier::BOLD)), Span::styled(format!(" of {} focused", if g.enabled { ui::hm(g.target_seconds) } else { "—".into() }), Style::default().fg(p.dim))]),
        Line::from(bar),
        Line::from(Span::styled(format!("{} sessions done · {} cut short", c.day.completed_sessions, c.day.interrupted_sessions), Style::default().fg(p.text))),
        Line::from(Span::styled(format!("+{} XP earned today", xp_today), Style::default().fg(p.good).add_modifier(Modifier::BOLD))),
        Line::from(Span::styled(match c.chest_today() { Some(x) => format!("Daily Chest opened: +{} XP", x), None => format!("{} left to open the Daily Chest", ui::hm(g.target_seconds.saturating_sub(g.done_seconds))) }, Style::default().fg(p.warn))),
    ];
    ui::fill_hints(&mut lines, inner.height, p, &["Finished sessions pay 15 XP + 2 XP/minute.".to_string(), "Press I to set a goal first.".to_string()]);
    ui::text(f, inner, lines);
    if parts.len() < 2 { return; }

    // Cycle
    let (idx_tips, idx_cycle) = if parts.len() == 3 { (2, 1) } else { (1, usize::MAX) };
    if idx_cycle != usize::MAX {
        let inner = ui::card(f, parts[idx_cycle], p, "↻", "Cadence Cycle  (M to change)", p.accent2, false);
        let (fm, sb, lb, n) = c.rt.clock.cadence.schedule_minutes(&c.rt.prefs.focus);
        let done = c.rt.clock.completed_focus_intervals;
        let mut dots = Vec::new();
        for i in 0..n {
            let on = i < done;
            dots.push(Span::styled(if on { "● " } else { "○ " }, Style::default().fg(if on { p.good } else { p.dim }).add_modifier(Modifier::BOLD)));
        }
        let lines = vec![
            Line::from(dots),
            Line::from(Span::styled(format!("{}m focus → {}m break, ×{}", fm, sb, n), Style::default().fg(p.text))),
            Line::from(Span::styled(format!("then a {}m long break", lb), Style::default().fg(p.dim))),
            Line::from(Span::styled(format!("Interval {} of {}", (done + 1).min(n), n), Style::default().fg(p.accent2).add_modifier(Modifier::BOLD))),
        ];
        ui::center(f, inner, p, lines);
    }

    // Tips
    let inner = ui::card(f, parts[idx_tips], p, "✧", "Coach", p.good, false);
    let day = chrono::Datelike::ordinal(&chrono::Local::now().date_naive()) as usize;
    let mut lines: Vec<Line> = Vec::new();
    let tips: Vec<String> = (0..TIPS.len()).map(|i| TIPS[(day + i) % TIPS.len()].to_string()).collect();
    for t in tips.iter().take(((inner.height as usize) / 3).max(1)) {
        lines.push(Line::from(Span::styled(format!("• {}", t), Style::default().fg(p.text))));
        lines.push(Line::from(""));
    }
    ui::text(f, inner, lines);
}

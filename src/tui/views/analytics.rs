//! Stats: five lenses (Overview, Rhythm, Skills, Days, Review) over one shared report.

use super::{day_totals, fmt_hour, golden_hour, kind_glyph, session_xp, weekday_name, Ctx};
use crate::domain::{
    build_observatory_report, synthesize_weekly_reflection, HorizonFilter, ObservatoryReport,
};
use crate::runtime::AnalyticsLens;
use crate::tui::ui;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    Frame,
};

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let rt = c.rt;
    let rep = build_observatory_report(&rt.vault.sessions, &rt.vault.categories, &rt.prefs, rt.analytics_horizon, rt.analytics_category_filter.as_deref(), &c.today, None);
    let kpi_h = if area.height >= 24 { 4 } else { 0 };
    let r = ui::rows(area, &[Constraint::Length(1), Constraint::Length(kpi_h), Constraint::Min(4)]);
    controls(f, r[0], c);
    if kpi_h > 0 { kpis(f, r[1], c, &rep); }
    match rt.analytics_lens {
        AnalyticsLens::Summary => overview(f, r[2], c, &rep),
        AnalyticsLens::TrendsAndRhythm => rhythm(f, r[2], c, &rep),
        AnalyticsLens::CategoryMatrix => skills(f, r[2], c, &rep),
        AnalyticsLens::DayExplorer => days(f, r[2], c),
        AnalyticsLens::WeeklyReflection => review(f, r[2], c, &rep),
    }
}

fn controls(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let mut s = Vec::new();
    for l in AnalyticsLens::ALL {
        let on = l == c.rt.analytics_lens;
        s.push(Span::styled(format!(" {} ", l.label()), if on { Style::default().fg(p.on_accent).bg(p.accent2).add_modifier(Modifier::BOLD) } else { Style::default().fg(p.dim).bg(p.card) }));
        s.push(Span::raw(" "));
    }
    s.push(Span::styled(" │ ", Style::default().fg(p.border)));
    for h in [HorizonFilter::Today, HorizonFilter::Days7, HorizonFilter::Days30, HorizonFilter::Days90, HorizonFilter::Year1, HorizonFilter::AllTime] {
        let on = h == c.rt.analytics_horizon;
        s.push(Span::styled(format!(" {} ", h.short_key()), if on { Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD) } else { Style::default().fg(p.dim) }));
    }
    let filt = c.rt.analytics_category_filter.as_ref().and_then(|id| c.rt.vault.categories.iter().find(|x| &x.id == id));
    s.push(Span::styled(format!("  skill: {}", filt.map(|x| x.name.as_str()).unwrap_or("all")), Style::default().fg(p.accent3)));
    f.render_widget(ratatui::widgets::Paragraph::new(Line::from(s)), area);
}

fn kpis(f: &mut Frame, area: Rect, c: &Ctx, rep: &ObservatoryReport) {
    let p = &c.p;
    let cols = ui::fit_cols(area, &[1, 1, 1, 1, 1, 1], 17);
    let delta = match rep.period_delta_percent { Some(d) if d >= 0 => (format!("▲ {}%", d), p.good), Some(d) => (format!("▼ {}%", -d), p.warn), None => ("—".into(), p.dim) };
    let tiles: Vec<(&str, &str, String, String, ratatui::style::Color)> = vec![
        ("◷", "Total time", ui::hm(rep.total_productive_seconds), format!("F {} · R {}", ui::hm(rep.total_focus_seconds), ui::hm(rep.total_reading_seconds)), p.accent),
        ("◈", "Daily average", ui::hm(rep.daily_average_seconds), format!("over {} days", rep.span_days), p.accent2),
        ("↕", "vs. before", delta.0, "previous period".into(), delta.1),
        ("✔", "Finish rate", rep.completion_rate_percent.map(|v| format!("{}%", v)).unwrap_or("—".into()), format!("{} done · {} cut", rep.completed_sessions_count, rep.interrupted_sessions_count), p.good),
        ("ϟ", "Streak", format!("{}d", rep.streaks.combined.current_days), format!("best {}d", rep.streaks.combined.best_days), p.accent3),
        ("▣", "Pages", format!("{}", rep.total_pages_read), format!("avg session {}", ui::hm(rep.average_session_seconds)), p.info),
    ];
    for (i, t) in tiles.into_iter().enumerate().take(cols.len()) {
        ui::tile(f, cols[i], p, t.0, t.1, t.2, t.3, t.4);
    }
}

fn series_items(c: &Ctx, rep: &ObservatoryReport, max_bars: usize) -> Vec<(String, u64, ratatui::style::Color)> {
    let p = &c.p;
    let pts = &rep.daily_series;
    let bucket = pts.len().div_ceil(max_bars.max(1)).max(1);
    pts.chunks(bucket)
        .map(|ch| {
            let v: u64 = ch.iter().map(|d| d.focus_seconds + d.reading_seconds).sum();
            let first = &ch[0].date_key;
            let label = if bucket == 1 && pts.len() <= 14 { weekday_name(first).chars().take(2).collect() } else { first.get(8..10).unwrap_or("").to_string() };
            (label, v, p.accent)
        })
        .collect()
}

fn overview(f: &mut Frame, area: Rect, c: &Ctx, rep: &ObservatoryReport) {
    let p = &c.p;
    let cols = ui::fit_cols(area, &[3, 2], 40);
    let inner = ui::card(f, cols[0], p, "▂", &format!("Daily time · {} → {}", rep.start_date_key, rep.end_date_key), p.accent, false);
    let bars = (inner.width as usize / 3).max(4);
    let items = series_items(c, rep, bars);
    let mark = items.len().checked_sub(1);
    ui::vbars(f, inner, p, &items, mark, true);
    if cols.len() < 2 { return; }

    let right = ui::rows(cols[1], &[Constraint::Min(8), Constraint::Percentage(if cols[1].height >= 24 { 45 } else { 0 })]);
    let inner = ui::card(f, right[0], p, "✧", "Insights & Records", p.accent3, false);
    let mut lines: Vec<Line> = Vec::new();
    for i in &rep.insights {
        lines.push(Line::from(Span::styled(format!("• {}", i), Style::default().fg(p.text))));
    }
    if let Some(k) = &rep.peak_day_key {
        lines.push(Line::from(Span::styled(format!("★ Best day: {} {} ({})", weekday_name(k), k, ui::hm(rep.peak_day_seconds)), Style::default().fg(p.accent3))));
    }
    if rep.longest_session_seconds > 0 {
        lines.push(Line::from(Span::styled(format!("◉ Longest session: {} ({})", ui::hm(rep.longest_session_seconds), rep.longest_session_label), Style::default().fg(p.accent))));
    }
    if let Some(h) = golden_hour(&rep.hourly_distribution) {
        lines.push(Line::from(Span::styled(format!("☀ Golden hour: {}", fmt_hour(h)), Style::default().fg(p.accent2))));
    }
    let hints = vec![
        "Tab switches lens · [ ] changes the date range.".to_string(),
        "Press C to look at one skill at a time.".to_string(),
        "Press X to export everything as CSV.".to_string(),
        "Log a few more sessions to unlock sharper insights.".to_string(),
    ];
    ui::fill_hints(&mut lines, inner.height, p, &hints);
    ui::text(f, inner, lines);
    if right[1].height >= 6 {
        let inner = ui::card(f, right[1], p, "◆", "Skill mix", p.good, false);
        let items: Vec<(String, u64, ratatui::style::Color)> = rep.category_rows.iter().filter(|r| r.total_seconds > 0).take(6).map(|r| (r.icon_glyph.clone(), r.total_seconds, p.category(&r.color_hex))).collect();
        if items.is_empty() {
            ui::center(f, inner, p, vec![Line::from(Span::styled("Log a session to see your skill mix.", Style::default().fg(p.dim)))]);
        } else {
            ui::vbars(f, inner, p, &items, None, true);
        }
    }
}

fn rhythm(f: &mut Frame, area: Rect, c: &Ctx, rep: &ObservatoryReport) {
    let p = &c.p;
    let cols = ui::fit_cols(area, &[3, 2], 40);
    let best_h = golden_hour(&rep.hourly_distribution);
    let inner = ui::card(f, cols[0], p, "☀", &format!("Your day, hour by hour{}", best_h.map(|h| format!(" · golden hour {}", fmt_hour(h))).unwrap_or_default()), p.accent3, false);
    let items: Vec<(String, u64, ratatui::style::Color)> = rep.hourly_distribution.iter().enumerate().map(|(h, v)| (if h % 3 == 0 { format!("{:02}", h) } else { String::new() }, *v, if (6..12).contains(&h) { p.accent3 } else if (12..18).contains(&h) { p.accent } else if (18..24).contains(&h) { p.accent2 } else { p.info })).collect();
    ui::vbars(f, inner, p, &items, best_h, false);
    if cols.len() < 2 { return; }
    let r = ui::rows(cols[1], &[Constraint::Min(8), Constraint::Length(if cols[1].height >= 18 { 7 } else { 0 })]);
    let inner = ui::card(f, r[0], p, "▤", "Weekdays", p.accent2, false);
    let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    let best = rep.weekday_distribution.iter().enumerate().max_by_key(|(_, v)| **v).filter(|(_, v)| **v > 0).map(|(i, _)| i);
    let items: Vec<(String, u64, ratatui::style::Color)> = rep.weekday_distribution.iter().enumerate().map(|(i, v)| (names[i].to_string(), *v, if i >= 5 { p.accent2 } else { p.accent })).collect();
    ui::vbars(f, inner, p, &items, best, true);
    if r[1].height > 0 {
        let inner = ui::card(f, r[1], p, "✧", "What this means", p.good, false);
        let morning: u64 = rep.hourly_distribution[5..12].iter().sum();
        let afternoon: u64 = rep.hourly_distribution[12..18].iter().sum();
        let evening: u64 = rep.hourly_distribution[18..24].iter().sum::<u64>() + rep.hourly_distribution[0..5].iter().sum::<u64>();
        let top = [("morning", morning), ("afternoon", afternoon), ("evening/night", evening)].into_iter().max_by_key(|x| x.1).unwrap();
        let mut lines = vec![
            Line::from(Span::styled(if top.1 > 0 { format!("You focus most in the {}.", top.0) } else { "No data yet for this range.".into() }, Style::default().fg(p.text).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(best.map(|i| format!("{} is your strongest weekday.", names[i])).unwrap_or_default(), Style::default().fg(p.text))),
        ];
        ui::fill_hints(&mut lines, inner.height, p, &["Plan your hardest task in your golden hour.".to_string(), "Put easy tasks in your low-energy hours.".to_string()]);
        ui::text(f, inner, lines);
    }
}

fn skills(f: &mut Frame, area: Rect, c: &Ctx, rep: &ObservatoryReport) {
    let p = &c.p;
    let cols = ui::fit_cols(area, &[3, 2], 44);
    let inner = ui::card(f, cols[0], p, "◆", "Time by skill", p.good, false);
    let w = inner.width as usize;
    let total = rep.total_productive_seconds.max(1);
    let mut lines: Vec<Line> = Vec::new();
    // one stacked bar showing the whole mix
    let mut stack: Vec<Span> = Vec::new();
    let mut used = 0usize;
    for r in rep.category_rows.iter().filter(|r| r.total_seconds > 0) {
        let n = ((r.total_seconds as f64 / total as f64) * w as f64).round() as usize;
        let n = n.min(w - used);
        stack.push(Span::styled("█".repeat(n), Style::default().fg(p.category(&r.color_hex))));
        used += n;
    }
    if used < w { stack.push(Span::styled("░".repeat(w - used), Style::default().fg(p.border))); }
    lines.push(Line::from(stack));
    lines.push(Line::from(""));
    let max = rep.category_rows.iter().map(|r| r.total_seconds).max().unwrap_or(1).max(1);
    for r in rep.category_rows.iter().filter(|r| !r.is_archived || r.total_seconds > 0) {
        let col = p.category(&r.color_hex);
        let name_w = 14usize.min(w / 3);
        let bar_w = w.saturating_sub(name_w + 26).max(4);
        let mut s = vec![Span::styled(format!("{} ", r.icon_glyph), Style::default().fg(col)), Span::styled(ui::pad_right(&r.name, name_w), Style::default().fg(p.text))];
        s.extend(ui::meter(bar_w, r.total_seconds as f64 / max as f64, col, p.card_hi));
        s.push(Span::styled(format!(" {:>7} {:>3.0}% {}×", ui::hm(r.total_seconds), r.share_percent, r.sessions_count), Style::default().fg(p.dim)));
        lines.push(Line::from(s));
    }
    ui::fill_hints(&mut lines, inner.height, p, &["Press C to filter every lens to one skill.".to_string(), "Balance tip: mix deep focus with reading.".to_string()]);
    ui::text(f, inner, lines);
    if cols.len() < 2 { return; }

    let inner = ui::card(f, cols[1], p, "◎", "Skill goals (today / this week)", p.accent2, false);
    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    for g in c.goals.category_goals.iter().filter(|g| g.daily.enabled || g.weekly.enabled) {
        let col = p.category(&g.color_hex);
        lines.push(Line::from(vec![Span::styled(format!("{} {}", g.icon_glyph, g.category_name), Style::default().fg(col).add_modifier(Modifier::BOLD))]));
        for (lab, m) in [("day ", &g.daily), ("week", &g.weekly)] {
            if !m.enabled { continue; }
            let mut s = vec![Span::styled(format!("  {} ", lab), Style::default().fg(p.dim))];
            s.extend(ui::meter(w.saturating_sub(18).max(4), m.percent.min(100) as f64 / 100.0, if m.is_met { p.good } else { col }, p.card_hi));
            s.push(Span::styled(format!(" {}/{}", ui::hm(m.done_seconds), ui::hm(m.target_seconds)), Style::default().fg(p.dim)));
            lines.push(Line::from(s));
        }
    }
    ui::fill_hints(&mut lines, inner.height, p, &["No skill goals yet.".to_string(), "Open 7 (Skills) → pick one → E → set a".to_string(), "daily or weekly goal in minutes.".to_string()]);
    ui::text(f, inner, lines);
}

fn days(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let r = ui::rows(area, &[Constraint::Length(12), Constraint::Min(5)]);
    let inner = ui::card(f, r[0], p, "▦", "Pick a day  (←/→ week · ↑/↓ day · Home = today)", p.accent3, false);
    super::home::heatmap(f, inner, c);

    let sel = chrono::Local::now().date_naive() - chrono::Duration::days(c.rt.heatmap_cursor_days_ago as i64);
    let key = crate::domain::format_date_key(sel);
    let cols = ui::fit_cols(r[1], &[3, 2], 44);
    let inner = ui::card(f, cols[0], p, "≡", &format!("Sessions on {} {}", weekday_name(&key), key), p.accent, false);
    let mut list: Vec<_> = c.rt.vault.sessions.iter().filter(|s| s.primary_date_key == key).collect();
    list.sort_by_key(|s| s.started_at_utc);
    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    for s in list.iter() {
        let cat = c.rt.vault.categories.iter().find(|x| x.id == s.category_id);
        let col = cat.map(|x| p.category(&x.color_hex)).unwrap_or(p.dim);
        let tail = format!(" {}  +{}xp", ui::hm(s.elapsed_seconds), session_xp(s));
        let label = if s.objective.is_empty() { s.activity_kind.label().to_string() } else { s.objective.clone() };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", kind_glyph(s.activity_kind)), Style::default().fg(col)),
            Span::styled(format!("{} ", s.started_at_utc.with_timezone(&chrono::Local).format("%H:%M")), Style::default().fg(p.dim)),
            Span::styled(ui::pad_right(&label, w.saturating_sub(8 + tail.chars().count())), Style::default().fg(p.text)),
            Span::styled(tail, Style::default().fg(p.good)),
        ]));
    }
    let totals = day_totals(&c.rt.vault.sessions, c.rt.prefs.focus.count_interrupted_in_totals);
    let (fs, rs) = totals.get(&key).copied().unwrap_or((0, 0));
    let hints: Vec<String> = [
        format!("Day total: {} focus + {} reading", ui::hm(fs), ui::hm(rs)),
        if fs + rs == 0 { "A blank day. Tomorrow is a fresh start.".into() } else { "Open 5 (Log) to edit or delete sessions.".into() },
        "Brighter squares on the heatmap = more time.".into(),
        "The brightest square means the daily goal was reached.".into(),
        "Use ←/→ to jump a whole week at a time.".into(),
        "Use ↑/↓ to step one day.".into(),
        "Press Home to jump back to today.".into(),
        "Tip: look for dark days that repeat, then plan around them.".into(),
    ].to_vec();
    ui::fill_hints(&mut lines, inner.height, p, &hints);
    ui::text(f, inner, lines);
    if cols.len() < 2 { return; }

    // Day breakdown: goal meters and an hour-by-hour chart for this day.
    let parts = ui::rows(cols[1], &[Constraint::Length(if cols[1].height >= 14 { 6 } else { 0 }), Constraint::Min(5)]);
    if parts[0].height > 0 {
        let inner = ui::card(f, parts[0], p, "◎", "That day vs. your goals", p.accent2, false);
        let goal_f = (c.rt.prefs.goals.daily_focus_minutes as u64 * 60).max(1);
        let goal_r = (c.rt.prefs.goals.daily_reading_minutes as u64 * 60).max(1);
        let bw = (inner.width as usize).saturating_sub(22).clamp(4, 30);
        let mut lines = Vec::new();
        for (name, v, g, col) in [("Focus", fs, goal_f, p.accent), ("Read ", rs, goal_r, p.accent2)] {
            let mut sp = vec![Span::styled(format!("{} ", name), Style::default().fg(p.dim))];
            sp.extend(ui::meter(bw, (v as f64 / g as f64).min(1.0), col, p.card_hi));
            sp.push(Span::styled(format!(" {}/{}", ui::hm(v), ui::hm(g)), Style::default().fg(p.text)));
            lines.push(Line::from(sp));
        }
        ui::center(f, inner, p, lines);
    }
    let inner = ui::card(f, parts[1], p, "☀", "Hour by hour", p.accent3, false);
    let mut hours = [0u64; 24];
    for s in list.iter().filter(|s| s.activity_kind.is_productive()) {
        let h = chrono::Timelike::hour(&s.started_at_utc.with_timezone(&chrono::Local)) as usize;
        hours[h.min(23)] += s.elapsed_seconds;
    }
    let items: Vec<(String, u64, ratatui::style::Color)> = hours.iter().enumerate().map(|(h, v)| (format!("{:02}", h), *v, p.accent)).collect();
    if hours.iter().all(|v| *v == 0) {
        ui::center(f, inner, p, vec![Line::from(Span::styled("Nothing logged on this day.", Style::default().fg(p.dim))), Line::from(Span::styled("Pick a brighter square above.", Style::default().fg(p.dim)))]);
    } else {
        ui::vbars(f, inner, p, &items, None, false);
    }
}

fn review(f: &mut Frame, area: Rect, c: &Ctx, rep: &ObservatoryReport) {
    let p = &c.p;
    let wr = synthesize_weekly_reflection(&c.rt.vault.sessions, &c.rt.vault.categories, &c.rt.prefs, &c.today);
    let r = ui::rows(area, &[Constraint::Length(if area.height >= 20 { 10 } else { 0 }), Constraint::Min(6)]);
    if r[0].height > 0 {
        let cols = ui::fit_cols(r[0], &[1, 1, 1], 30);
        let inner = ui::card(f, cols[0], p, "☰", &format!("Your week · {} → {}", wr.start_date_key, wr.end_date_key), p.accent, false);
        let delta = match wr.delta_percent { Some(d) if d >= 0 => format!("▲ {}% vs last week", d), Some(d) => format!("▼ {}% vs last week", -d), None => "no previous week yet".into() };
        let mut lines = vec![
            Line::from(Span::styled(ui::hm(wr.total_productive_seconds), Style::default().fg(p.accent).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(delta, Style::default().fg(p.text))),
            Line::from(Span::styled(format!("focus {} · reading {}", ui::hm(wr.focus_seconds), ui::hm(wr.reading_seconds)), Style::default().fg(p.dim))),
            Line::from(Span::styled(format!("goal met: focus {}/7 · reading {}/7 days", wr.focus_goal_days_met, wr.reading_goal_days_met), Style::default().fg(p.text))),
        ];
        if let Some(d) = &wr.best_day_key { lines.push(Line::from(Span::styled(format!("best day {} ({})", d, ui::hm(wr.best_day_seconds)), Style::default().fg(p.accent3)))); }
        if let Some(n) = &wr.top_category_name { lines.push(Line::from(Span::styled(format!("top skill {} ({})", n, ui::hm(wr.top_category_seconds)), Style::default().fg(p.good)))); }
        ui::center(f, inner, p, lines);
        if cols.len() > 1 {
            let inner = ui::card(f, cols[1], p, "➤", "Coach says", p.good, false);
            let mut lines: Vec<Line> = wr.recommendations.iter().map(|x| Line::from(Span::styled(format!("• {}", x), Style::default().fg(p.text)))).collect();
            ui::fill_hints(&mut lines, inner.height, p, &["Keep sessions short and frequent.".to_string(), "Protect your streak: even 10 minutes counts.".to_string(), "Plan tomorrow before you close the app.".to_string()]);
            ui::text(f, inner, lines);
        }
        if cols.len() > 2 {
            let inner = ui::card(f, cols[2], p, "◇", "Forecast", p.accent2, false);
            let avg = rep.daily_average_seconds;
            let xp14: u64 = c.rt.vault.sessions.iter().filter(|s| s.started_at_utc > chrono::Utc::now() - chrono::Duration::days(14)).map(session_xp).sum();
            let xp_day = xp14 / 14;
            let days_to = if xp_day > 0 && c.lvl.xp_needed_for_next > 0 { format!("~{} days to Level {}", c.lvl.xp_needed_for_next.div_ceil(xp_day), c.lvl.level + 1) } else { "Log sessions to forecast your next level.".into() };
            let goal_w = c.rt.prefs.goals.weekly_focus_minutes as u64 * 60;
            let lines = vec![
                Line::from(Span::styled(format!("At this pace: {} per week", ui::hm(avg * 7)), Style::default().fg(p.text).add_modifier(Modifier::BOLD))),
                Line::from(Span::styled(days_to, Style::default().fg(p.accent2))),
                Line::from(Span::styled(if goal_w > 0 { format!("Weekly focus goal: {}", ui::hm(goal_w)) } else { "No weekly goal set".into() }, Style::default().fg(p.dim))),
                Line::from(Span::styled(format!("Challenge: beat {} this week", if rep.peak_day_seconds > 0 { ui::hm(rep.peak_day_seconds) } else { "25m".into() }), Style::default().fg(p.accent3))),
            ];
            ui::center(f, inner, p, lines);
        }
    }

    let cols = ui::fit_cols(r[1], &[3, 2], 40);
    // this week, day by day
    let totals = day_totals(&c.rt.vault.sessions, c.rt.prefs.focus.count_interrupted_in_totals);
    let items: Vec<(String, u64, ratatui::style::Color)> = (0..7).rev().map(|b| {
        let key = crate::domain::shift_date_key(&c.today, -b);
        let v = totals.get(&key).map(|t| t.0 + t.1).unwrap_or(0);
        (weekday_name(&key).to_string(), v, p.accent)
    }).collect();
    let inner = ui::card(f, cols[0], p, "▂", "Last 7 days", p.accent, false);
    ui::vbars(f, inner, p, &items, Some(6), true);
    if cols.len() < 2 { return; }

    // closest trophies
    let inner = ui::card(f, cols[1], p, "❂", "Closest trophies", p.accent3, false);
    let mut near: Vec<_> = c.items.iter().filter(|i| !i.is_unlocked).collect();
    near.sort_by_key(|i| std::cmp::Reverse(i.progress_percent));
    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    for it in near.iter().take((inner.height as usize / 3).max(1)) {
        lines.push(Line::from(vec![Span::styled(format!("{} ", it.spec.rarity.glyph()), Style::default().fg(p.accent3)), Span::styled(ui::clamp_str(it.spec.title, w.saturating_sub(14)), Style::default().fg(p.text).add_modifier(Modifier::BOLD)), Span::styled(format!("  +{} XP", it.spec.rarity.xp_bonus()), Style::default().fg(p.good))]));
        let mut m = vec![Span::raw("  ")];
        m.extend(ui::meter(w.saturating_sub(14).max(4), it.progress_percent as f64 / 100.0, p.accent3, p.card_hi));
        m.push(Span::styled(format!(" {}/{}", it.current_value.min(it.spec.target), it.spec.target), Style::default().fg(p.dim)));
        lines.push(Line::from(m));
        lines.push(Line::from(""));
    }
    ui::fill_hints(&mut lines, inner.height, p, &["Every trophy also pays bonus XP.".to_string()]);
    ui::text(f, inner, lines);
}

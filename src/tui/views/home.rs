//! Home / Command Deck: banner, today's stats, quests, queue, log and heatmap.

use super::{day_totals, fmt_hour, golden_hour, kind_glyph, session_xp, weekday_name, Ctx, MANTRAS};
use crate::domain::{build_observatory_report, CompletionState, HorizonFilter, SessionRecord};
use crate::tui::ui;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    Frame,
};

const BANNER_FULL: [&str; 6] = [
    r#" ██████╗██╗  ██╗██████╗  ██████╗ ███╗   ██╗ ██████╗ ███████╗"#,
    r#"██╔════╝██║  ██║██╔══██╗██╔═══██╗████╗  ██║██╔═══██╗██╔════╝"#,
    r#"██║     ███████║██████╔╝██║   ██║██╔██╗ ██║██║   ██║███████╗"#,
    r#"██║     ██╔══██║██╔══██╗██║   ██║██║╚██╗██║██║   ██║╚════██║"#,
    r#"╚██████╗██║  ██║██║  ██║╚██████╔╝██║ ╚████║╚██████╔╝███████║"#,
    r#" ╚═════╝╚═╝  ╚═╝╚═╝  ╚═╝ ╚═════╝ ╚═╝  ╚═══╝ ╚═════╝ ╚══════╝"#,
];
const BANNER_COMPACT: [&str; 4] = [
    r#"  ___ _                        "#,
    r#" / __| |_  _ _ ___ _ _  ___ ___"#,
    r#"| (__| ' \| '_/ _ \ ' \/ _ (_-<"#,
    r#" \___|_||_|_| \___/_||_\___/__/"#,
];

const TAGLINE: &str = "Discipline = Value + Reward - Friction";

fn hero_height(area: Rect, full: bool) -> u16 {
    let w = BANNER_FULL[0].chars().count() as u16;
    let cw = BANNER_COMPACT[0].chars().count() as u16;
    if full && area.width >= w + 4 { BANNER_FULL.len() as u16 + 4 } else if area.width >= cw + 4 { BANNER_COMPACT.len() as u16 + 3 } else { 3 }
}

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    // Greedy section planning: stats → quests row → heatmap row → hero. Leftover goes to the quests row.
    let mut avail = area.height;
    let stats_h = 8u16;
    let bottom_h = 12u16;
    let mid_min = 9u16;
    let full_hero = hero_height(area, true);
    let comp_hero = hero_height(area, false);

    let show_stats = avail >= stats_h + 3;
    if show_stats { avail -= stats_h; }
    let show_mid = avail >= mid_min || !show_stats;
    if show_mid { avail = avail.saturating_sub(mid_min); }
    let show_bottom = show_mid && avail >= bottom_h;
    if show_bottom { avail -= bottom_h; }
    let (show_hero, hero_h) = if show_bottom && avail >= full_hero { (true, full_hero) } else if show_bottom && avail >= comp_hero { (true, comp_hero) } else { (false, 0) };

    let mut cs = Vec::new();
    if show_hero { cs.push(Constraint::Length(hero_h)); }
    if show_stats { cs.push(if show_mid { Constraint::Length(stats_h) } else { Constraint::Fill(1) }); }
    if show_mid { cs.push(Constraint::Min(mid_min)); }
    if show_bottom { cs.push(Constraint::Length(bottom_h)); }
    let r = ui::rows(area, &cs);
    let mut i = 0;
    if show_hero { render_hero(f, r[i], c, hero_h > 8); i += 1; }
    if show_stats { render_stats(f, r[i], c); i += 1; }
    if show_mid { render_mid(f, r[i], c); i += 1; }
    if show_bottom {
        let cols = ui::fit_cols(r[i], &[3, 2], 34);
        let inner = ui::card(f, cols[0], p, "▦", "Activity Heatmap  (←/→ week · Home = today)", p.accent3, false);
        heatmap(f, inner, c);
        if cols.len() > 1 { render_skill_split(f, cols[1], c); }
    }
}

fn render_hero(f: &mut Frame, area: Rect, c: &Ctx, full: bool) {
    let p = &c.p;
    let inner = ui::card(f, area, p, "◈", "Welcome back", p.accent, false);
    let art: &[&str] = if full && area.height > 9 { &BANNER_FULL } else { &BANNER_COMPACT };
    let width = art[0].chars().count().max(1);
    let bands = [p.accent, p.accent2, p.accent3];
    let mut lines: Vec<Line> = Vec::new();
    if inner.width as usize >= width {
        for row in art {
            let spans: Vec<Span> = row
                .chars()
                .enumerate()
                .map(|(col, ch)| Span::styled(ch.to_string(), Style::default().fg(bands[(col * 3 / width).min(2)]).add_modifier(Modifier::BOLD)))
                .collect();
            lines.push(Line::from(spans));
        }
    } else {
        lines.push(Line::from(Span::styled("C H R O N O S", Style::default().fg(p.accent).add_modifier(Modifier::BOLD))));
    }
    lines.push(Line::from(Span::styled(TAGLINE, Style::default().fg(p.text).add_modifier(Modifier::ITALIC))));
    if inner.height as usize > lines.len() {
        let day = chrono::Datelike::ordinal(&chrono::Local::now().date_naive()) as usize;
        lines.push(Line::from(Span::styled(MANTRAS[day % MANTRAS.len()], Style::default().fg(p.dim))));
    }
    ui::center(f, inner, p, lines);
}

fn render_stats(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let cols = ui::fit_cols(area, &[1, 1, 1, 1], 24);
    // Today
    let inner = ui::card(f, cols[0], p, "◉", "Today", p.accent, false);
    let fg = &c.goals.daily_focus;
    let rg = &c.goals.daily_reading;
    let bw = (inner.width as usize).saturating_sub(12).clamp(4, 24);
    let mut l = vec![Line::from(vec![
        Span::styled(ui::hm(c.productive_today()), Style::default().fg(p.text).add_modifier(Modifier::BOLD)),
        Span::styled(" logged", Style::default().fg(p.dim)),
    ])];
    for (name, g, col) in [("Focus", fg, p.accent), ("Read ", rg, p.accent2)] {
        let mut s = vec![Span::styled(format!("{} ", name), Style::default().fg(p.dim))];
        s.extend(ui::meter(bw, g.percent.min(100) as f64 / 100.0, col, p.card_hi));
        s.push(Span::styled(format!(" {:>3}%", g.percent.min(999)), Style::default().fg(ui::pct_color(p, g.percent as f64))));
        l.push(Line::from(s));
    }
    l.push(Line::from(Span::styled(format!("{} focus · {} read · {}p", ui::hm(c.day.focus_seconds), ui::hm(c.day.reading_seconds), c.day.pages_read), Style::default().fg(p.dim))));
    ui::center(f, inner, p, l);

    if cols.len() < 2 { return; }
    // Streak
    let inner = ui::card(f, cols[1], p, "ϟ", "Streak", p.accent3, c.streak_at_risk());
    let cur = c.streaks.combined.current_days;
    let next = c.next_milestone();
    let mut dots = Vec::new();
    let totals = day_totals(&c.rt.vault.sessions, c.rt.prefs.focus.count_interrupted_in_totals);
    for back in (0..7).rev() {
        let key = crate::domain::shift_date_key(&c.today, -back);
        let v = totals.get(&key).map(|t| t.0 + t.1).unwrap_or(0);
        let (g, col) = if v >= 60 { ("●", p.good) } else if back == 0 { ("◌", p.warn) } else { ("○", p.dim) };
        dots.push(Span::styled(format!("{} ", g), Style::default().fg(col)));
    }
    let mut l = vec![
        Line::from(vec![Span::styled(format!("{}", cur), Style::default().fg(p.accent3).add_modifier(Modifier::BOLD)), Span::styled(if cur == 1 { " day" } else { " days" }, Style::default().fg(p.dim)), Span::styled(format!("   best {}", c.streaks.combined.best_days), Style::default().fg(p.dim))]),
        Line::from(dots),
    ];
    let mut m = vec![Span::styled("next ", Style::default().fg(p.dim))];
    m.extend(ui::meter(10, cur as f64 / next as f64, p.accent3, p.card_hi));
    m.push(Span::styled(format!(" {}d", next), Style::default().fg(p.dim)));
    l.push(Line::from(m));
    l.push(Line::from(Span::styled(
        if c.streak_at_risk() { "Log just 1 minute to keep it alive!" } else if cur == 0 { "Start today — 1 minute counts." } else { "Safe for today. Keep going!" },
        Style::default().fg(if c.streak_at_risk() { p.warn } else { p.good }).add_modifier(Modifier::BOLD),
    )));
    ui::center(f, inner, p, l);

    if cols.len() < 3 { return; }
    // Level
    let inner = ui::card(f, cols[2], p, "✦", "Level", p.accent2, false);
    let bw = (inner.width as usize).saturating_sub(8).clamp(6, 26);
    let mut bar = ui::meter(bw, c.lvl.progress_percent as f64 / 100.0, p.accent2, p.card_hi);
    bar.push(Span::styled(format!(" {}%", c.lvl.progress_percent), Style::default().fg(p.dim)));
    let l = vec![
        Line::from(vec![Span::styled(format!("Lv {} ", c.lvl.level), Style::default().fg(p.accent2).add_modifier(Modifier::BOLD)), Span::styled(ui::clamp_str(c.lvl.title, inner.width.saturating_sub(7) as usize), Style::default().fg(p.text))]),
        Line::from(bar),
        Line::from(Span::styled(format!("{} XP total", c.lvl.total_xp), Style::default().fg(p.text))),
        Line::from(Span::styled(match c.lvl.next_title { Some(t) => format!("{} XP to {}", c.lvl.xp_needed_for_next, ui::clamp_str(t, 22)), None => "Max level reached!".into() }, Style::default().fg(p.dim))),
    ];
    ui::center(f, inner, p, l);

    if cols.len() < 4 { return; }
    // Chest
    let chest = c.chest_today();
    let inner = ui::card(f, cols[3], p, "▣", "Daily Chest", p.warn, chest.is_some());
    let l = match chest {
        Some(xp) => vec![
            Line::from(Span::styled(format!("★ {} CHEST OPEN ★", crate::domain::chest_tier(xp).to_uppercase()), Style::default().fg(p.warn).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled(format!("+{} XP", xp), Style::default().fg(p.good).add_modifier(Modifier::BOLD))),
            Line::from(Span::styled("Come back tomorrow for", Style::default().fg(p.dim))),
            Line::from(Span::styled("a brand-new surprise.", Style::default().fg(p.dim))),
        ],
        None => {
            let mut bar = ui::meter(14, fg.percent.min(100) as f64 / 100.0, p.warn, p.card_hi);
            bar.push(Span::styled(format!(" {}%", fg.percent.min(100)), Style::default().fg(p.dim)));
            vec![
                Line::from(Span::styled("▣ LOCKED", Style::default().fg(p.dim).add_modifier(Modifier::BOLD))),
                Line::from(bar),
                Line::from(Span::styled(format!("{} more focus to open", ui::hm(fg.target_seconds.saturating_sub(fg.done_seconds))), Style::default().fg(p.text))),
                Line::from(Span::styled("Prize: 30–150 XP (a surprise!)", Style::default().fg(p.dim))),
            ]
        }
    };
    ui::center(f, inner, p, l);
}

fn render_mid(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let cols = ui::fit_cols(area, &[1, 1, 1], 34);
    // Quests
    let inner = ui::card(f, cols[0], p, "☑", "Daily Quests", p.accent, false);
    let qs = c.quests();
    let mut lines: Vec<Line> = Vec::new();
    let bw = (inner.width as usize).saturating_sub(12).clamp(4, 30);
    let per = if inner.height >= (qs.len() as u16) * 3 { 3 } else { 2 };
    for q in &qs {
        let col = if q.done { p.good } else { p.accent };
        lines.push(Line::from(vec![
            Span::styled(if q.done { "✔ " } else { "◻ " }, Style::default().fg(col).add_modifier(Modifier::BOLD)),
            Span::styled(ui::pad_right(&q.title, (inner.width as usize).saturating_sub(q.reward.chars().count() + 3)), Style::default().fg(if q.done { p.dim } else { p.text })),
            Span::styled(format!(" {}", q.reward), Style::default().fg(p.accent3).add_modifier(Modifier::BOLD)),
        ]));
        let mut s = vec![Span::raw("  ")];
        s.extend(ui::meter(bw, q.pct as f64 / 100.0, col, p.card_hi));
        s.push(Span::styled(format!(" {}", q.hint), Style::default().fg(p.dim)));
        lines.push(Line::from(s));
        if per == 3 { lines.push(Line::from("")); }
    }
    let done = qs.iter().filter(|q| q.done).count();
    let hints = vec![
        format!("{}/{} quests done today", done, qs.len()),
        if done == qs.len() { "All clear! Bonus round: beat your best day.".to_string() } else { "Finish all quests for a perfect day.".to_string() },
    ];
    ui::fill_hints(&mut lines, inner.height, p, &hints);
    ui::text(f, inner, lines);

    if cols.len() < 2 { return; }
    // Queue
    let inner = ui::card(f, cols[1], p, "▸", "Quest Queue  (N new · Enter launch)", p.accent2, false);
    let pending: Vec<_> = c.rt.vault.planned_blocks.iter().filter(|b| b.status == crate::domain::PlanStatus::Pending).collect();
    let mut lines: Vec<Line> = Vec::new();
    for (i, b) in pending.iter().enumerate() {
        let sel = i == c.rt.overview_plan_cursor.min(pending.len().saturating_sub(1));
        let cat = c.rt.vault.categories.iter().find(|x| x.id == b.category_id);
        let col = cat.map(|x| p.category(&x.color_hex)).unwrap_or(p.accent);
        lines.push(Line::from(vec![
            Span::styled(if sel { "▸ " } else { "  " }, Style::default().fg(p.accent2).add_modifier(Modifier::BOLD)),
            Span::styled(ui::clamp_str(&b.title, inner.width.saturating_sub(4) as usize), if sel { Style::default().fg(p.on_accent).bg(p.accent2).add_modifier(Modifier::BOLD) } else { Style::default().fg(p.text) }),
        ]));
        lines.push(Line::from(vec![
            Span::raw("   "),
            Span::styled(format!("{} ", kind_glyph(b.activity_kind)), Style::default().fg(col)),
            Span::styled(ui::clamp_str(&format!("{}m · {:?} · {}", b.target_minutes, b.priority, cat.map(|x| x.name.as_str()).unwrap_or("-")), inner.width.saturating_sub(5) as usize), Style::default().fg(p.dim)),
        ]));
    }
    let mut hints: Vec<String> = if pending.is_empty() {
        vec!["Nothing planned yet. Plans beat willpower:".to_string(), "  press N to schedule a block,".to_string(), "  or Space to start a quick session.".to_string()]
    } else {
        vec![format!("{} block(s) waiting. Enter launches one.", pending.len())]
    };
    if let Some(h) = golden_hour(&c.rt_hourly()) { hints.push(format!("Tip: your golden hour is {} — plan there.", fmt_hour(h))); }
    hints.push("Every planned block finished = +20 XP.".to_string());
    ui::fill_hints(&mut lines, inner.height, p, &hints);
    ui::text(f, inner, lines);

    if cols.len() < 3 { return; }
    // Mission log
    let inner = ui::card(f, cols[2], p, "≡", "Mission Log", p.accent3, false);
    let mut recent: Vec<&SessionRecord> = c.rt.vault.sessions.iter().collect();
    recent.sort_by_key(|s| std::cmp::Reverse(s.started_at_utc));
    let mut lines: Vec<Line> = Vec::new();
    let w = inner.width as usize;
    for s in recent.iter().take(inner.height as usize) {
        let cat = c.rt.vault.categories.iter().find(|x| x.id == s.category_id);
        let col = cat.map(|x| p.category(&x.color_hex)).unwrap_or(p.dim);
        let label = if s.objective.is_empty() { cat.map(|x| x.name.clone()).unwrap_or_else(|| s.activity_kind.short_label().to_string()) } else { s.objective.clone() };
        let tail = format!(" {} +{}xp", ui::hm(s.elapsed_seconds), session_xp(s));
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", kind_glyph(s.activity_kind)), Style::default().fg(col)),
            Span::styled(format!("{} ", weekday_name(&s.primary_date_key)), Style::default().fg(p.dim)),
            Span::styled(ui::pad_right(&label, w.saturating_sub(6 + tail.chars().count())), Style::default().fg(p.text)),
            Span::styled(tail, Style::default().fg(if s.completion_state == CompletionState::Completed { p.good } else { p.warn })),
        ]));
    }
    let hints = vec![
        "No sessions yet — your story starts here.".to_string(),
        "Press 2 for Focus or 3 for Reading, then Space.".to_string(),
        "Every finished session earns XP and keeps".to_string(),
        "your streak glowing.".to_string(),
    ];
    ui::fill_hints(&mut lines, inner.height, p, &hints);
    ui::text(f, inner, lines);
}

impl Ctx<'_> {
    pub fn rt_hourly(&self) -> [u64; 24] {
        build_observatory_report(&self.rt.vault.sessions, &self.rt.vault.categories, &self.rt.prefs, HorizonFilter::Days90, None, &self.today, None).hourly_distribution
    }
}

fn render_skill_split(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let inner = ui::card(f, area, p, "◆", "Skills · last 7 days", p.good, false);
    let rep = build_observatory_report(&c.rt.vault.sessions, &c.rt.vault.categories, &c.rt.prefs, HorizonFilter::Days7, None, &c.today, None);
    let w = inner.width as usize;
    let mut lines: Vec<Line> = Vec::new();
    let max = rep.category_rows.iter().map(|r| r.total_seconds).max().unwrap_or(1).max(1);
    for r in rep.category_rows.iter().filter(|r| !r.is_archived).take(inner.height as usize) {
        let col = p.category(&r.color_hex);
        let name_w = 16usize.min(w / 3);
        let bar_w = w.saturating_sub(name_w + 12).max(3);
        let mut s = vec![Span::styled(format!("{} ", r.icon_glyph), Style::default().fg(col)), Span::styled(format!("{} ", ui::pad_right(&r.name, name_w)), Style::default().fg(p.text))];
        s.extend(ui::meter(bar_w, r.total_seconds as f64 / max as f64, col, p.card_hi));
        s.push(Span::styled(format!(" {:>6}", ui::hm(r.total_seconds)), Style::default().fg(p.dim)));
        lines.push(Line::from(s));
    }
    let hints = vec![
        format!("Week total: {}", ui::hm(rep.total_productive_seconds)),
        match rep.period_delta_percent { Some(d) if d >= 0 => format!("▲ {}% vs last week — nice!", d), Some(d) => format!("▼ {}% vs last week", -d), None => "Log a few days to see weekly trends.".into() },
        format!("{} sessions · {} pages", rep.completed_sessions_count, rep.total_pages_read),
        "Press 7 to manage your skill classes.".to_string(),
    ];
    ui::fill_hints(&mut lines, inner.height, p, &hints);
    ui::text(f, inner, lines);
}

/// Heatmap that fills its area. Uses the global heatmap cursor (days ago).
pub fn heatmap(f: &mut Frame, area: Rect, c: &Ctx) {
    use chrono::Datelike;
    let p = &c.p;
    if area.height < 5 || area.width < 16 { return; }
    let totals = day_totals(&c.rt.vault.sessions, c.rt.prefs.focus.count_interrupted_in_totals);
    let today = crate::domain::parse_date_key(&c.today).unwrap_or_else(|| chrono::Local::now().date_naive());
    let goal = (c.rt.prefs.goals.daily_focus_minutes as u64 * 60).max(1800);
    let cw: usize = if (area.width as usize).saturating_sub(4) / 2 >= 26 { 2 } else { 1 };
    let weeks = ((area.width as usize).saturating_sub(4) / cw).clamp(1, 53);
    let monday = today - chrono::Duration::days(today.weekday().num_days_from_monday() as i64);
    let pad = (area.width as usize).saturating_sub(4 + weeks * cw) / 2;
    let sel_date = today - chrono::Duration::days(c.rt.heatmap_cursor_days_ago as i64);

    let mut lines: Vec<Line> = Vec::new();
    // month header
    let mut hdr = String::new();
    let mut last_month = 0;
    for col in 0..weeks {
        let d = monday - chrono::Duration::days(((weeks - 1 - col) * 7) as i64);
        let m = d.month();
        let cell = if m != last_month && d.day() <= 7 { last_month = m; ["", "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][m as usize] } else { "" };
        let want = col * cw;
        if hdr.chars().count() <= want && !cell.is_empty() {
            while hdr.chars().count() < want { hdr.push(' '); }
            hdr.push_str(cell);
        }
    }
    lines.push(Line::from(vec![Span::raw(" ".repeat(4 + pad)), Span::styled(hdr, Style::default().fg(p.dim))]));
    let names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    for row in 0..7usize {
        let mut s = vec![Span::raw(" ".repeat(pad)), Span::styled(format!("{} ", names[row]), Style::default().fg(p.dim))];
        for col in 0..weeks {
            let d = monday - chrono::Duration::days(((weeks - 1 - col) * 7) as i64) + chrono::Duration::days(row as i64);
            if d > today {
                s.push(Span::raw(" ".repeat(cw)));
                continue;
            }
            let key = crate::domain::format_date_key(d);
            let v = totals.get(&key).map(|t| t.0 + t.1).unwrap_or(0);
            let lvl = if v == 0 { 0 } else if v * 4 < goal { 1 } else if v * 2 < goal { 2 } else if v < goal { 3 } else { 4 };
            let selected = d == sel_date;
            let glyph = if selected { "◆" } else { "■" };
            let style = if selected { Style::default().fg(p.accent3).add_modifier(Modifier::BOLD) } else { Style::default().fg(p.heat[lvl]) };
            s.push(Span::styled(format!("{}{}", glyph, if cw == 2 { " " } else { "" }), style));
        }
        lines.push(Line::from(s));
    }
    let key = crate::domain::format_date_key(sel_date);
    let (fs, rs) = totals.get(&key).copied().unwrap_or((0, 0));
    lines.push(Line::from(vec![
        Span::raw(" ".repeat(pad)),
        Span::styled(format!("◆ {} {}  ", weekday_name(&key), key), Style::default().fg(p.accent3).add_modifier(Modifier::BOLD)),
        Span::styled(format!("focus {} · read {} · {}% of goal", ui::hm(fs), ui::hm(rs), ((fs + rs) * 100 / goal).min(999)), Style::default().fg(p.text)),
    ]));
    if (lines.len() as u16) < area.height {
        let mut leg = vec![Span::raw(" ".repeat(pad)), Span::styled("less ", Style::default().fg(p.dim))];
        for h in p.heat { leg.push(Span::styled("■ ", Style::default().fg(h))); }
        leg.push(Span::styled("more   (full colour = daily goal reached)", Style::default().fg(p.dim)));
        lines.push(Line::from(leg));
    }
    ui::center(f, area, p, lines);
}

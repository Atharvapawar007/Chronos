//! One module per screen, plus the shared per-frame context.

pub mod analytics;
pub mod categories;
pub mod focus;
pub mod guide;
pub mod history;
pub mod home;
pub mod reading;
pub mod settings;
pub mod trophies;

use crate::domain::{
    attribute_day_seconds, compute_streaks, to_local_date_key, AchievementStatusItem,
    ActivityKind, DayAttribution, GoalsEvaluation, LevelSnapshot, LiveClockSnapshot, StreakSummary,
};
use crate::runtime::WorkspaceRuntime;
use crate::tui::theme::Palette;
use chrono::Utc;

/// Everything a screen needs, computed once per frame.
pub struct Ctx<'a> {
    pub rt: &'a WorkspaceRuntime,
    pub p: Palette,
    pub today: String,
    pub goals: GoalsEvaluation,
    pub lvl: LevelSnapshot,
    pub items: Vec<AchievementStatusItem>,
    pub streaks: StreakSummary,
    pub day: DayAttribution,
    pub snap: LiveClockSnapshot,
    pub tick: u64,
    pub animate: bool,
}

pub struct Quest {
    pub title: String,
    pub hint: String,
    pub reward: String,
    pub pct: u16,
    pub done: bool,
}

impl<'a> Ctx<'a> {
    pub fn new(rt: &'a WorkspaceRuntime) -> Self {
        let now = Utc::now();
        let today = to_local_date_key(now, None);
        let (lvl, items) = rt.current_progression_snapshot();
        let incl = rt.prefs.focus.count_interrupted_in_totals;
        Self {
            rt,
            p: rt.palette(),
            goals: rt.current_goals(),
            streaks: compute_streaks(&rt.vault.sessions, &today, incl),
            day: attribute_day_seconds(&rt.vault.sessions, &today, incl),
            snap: rt.clock.snapshot(now),
            lvl,
            items,
            tick: rt.frame_tick,
            animate: rt.prefs.appearance.motion != crate::domain::MotionLevel::Off,
            today,
        }
    }

    pub fn productive_today(&self) -> u64 {
        self.day.focus_seconds + self.day.reading_seconds
    }

    /// True when a streak is alive but nothing has been logged yet today.
    pub fn streak_at_risk(&self) -> bool {
        self.streaks.combined.current_days > 0 && self.productive_today() < 60
    }

    pub fn running_color(&self) -> ratatui::style::Color {
        match self.rt.clock.activity_kind {
            ActivityKind::Focus => self.p.accent,
            ActivityKind::Reading => self.p.accent2,
            _ => self.p.good,
        }
    }

    /// Daily quests with honest rewards (XP is 2 per minute + 15 per finished session).
    pub fn quests(&self) -> Vec<Quest> {
        let f = self.day.focus_seconds;
        let r = self.day.reading_seconds;
        let pct = |v: u64, t: u64| ((v * 100) / t.max(1)).min(100) as u16;
        let g = &self.goals.daily_focus;
        vec![
            Quest { title: "Warm-up: 25 min of focus".into(), hint: format!("{} / 25m", crate::tui::ui::hm(f)), reward: "+65 XP".into(), pct: pct(f, 1500), done: f >= 1500 },
            Quest { title: "Read for 20 minutes".into(), hint: format!("{} / 20m", crate::tui::ui::hm(r)), reward: "+55 XP".into(), pct: pct(r, 1200), done: r >= 1200 },
            Quest {
                title: if g.enabled { format!("Hit your {} focus goal", crate::tui::ui::hm(g.target_seconds)) } else { "Set a daily focus goal (Settings)".into() },
                hint: format!("{}%", g.percent.min(100)),
                reward: "Daily Chest".into(),
                pct: g.percent.min(100),
                done: g.is_met,
            },
            Quest { title: "Two-mode day: 10m focus + 10m reading".into(), hint: format!("{}m + {}m", (f / 60).min(10), (r / 60).min(10)), reward: "+Explorer".into(), pct: ((f.min(600) + r.min(600)) * 100 / 1200) as u16, done: f >= 600 && r >= 600 },
        ]
    }

    /// Chest for today: `Some(xp)` once the focus goal is met.
    pub fn chest_today(&self) -> Option<u64> {
        if self.goals.daily_focus.is_met { Some(crate::domain::daily_chest_xp(&self.today)) } else { None }
    }

    pub fn next_milestone(&self) -> u32 {
        let cur = self.streaks.combined.current_days;
        [3u32, 7, 14, 30, 60, 100, 200, 365].into_iter().find(|m| *m > cur).unwrap_or(cur + 100)
    }
}

use crate::domain::{CompletionState, SessionRecord};
use std::collections::HashMap;

/// (focus_seconds, reading_seconds) per local date key.
pub fn day_totals(sessions: &[SessionRecord], include_interrupted: bool) -> HashMap<String, (u64, u64)> {
    let mut map: HashMap<String, (u64, u64)> = HashMap::new();
    for s in sessions {
        if !s.activity_kind.is_productive() || (!include_interrupted && s.completion_state == CompletionState::Interrupted) {
            continue;
        }
        let mut add = |key: &str, secs: u64| {
            let e = map.entry(key.to_string()).or_insert((0, 0));
            if s.activity_kind == ActivityKind::Focus { e.0 += secs } else { e.1 += secs }
        };
        if s.day_slices.is_empty() {
            add(&s.primary_date_key, s.elapsed_seconds);
        } else {
            for d in &s.day_slices {
                add(&d.date_key, d.seconds);
            }
        }
    }
    map
}

/// XP a finished session earned (2 per minute plus 15 for completing it).
pub fn session_xp(s: &SessionRecord) -> u64 {
    if !s.activity_kind.is_productive() {
        return 0;
    }
    s.elapsed_seconds / 60 * 2 + if s.completion_state == CompletionState::Completed { 15 } else { 0 }
}

pub fn kind_glyph(k: ActivityKind) -> &'static str {
    match k {
        ActivityKind::Focus => "◉",
        ActivityKind::Reading => "▣",
        ActivityKind::ShortBreak | ActivityKind::LongBreak => "☾",
    }
}

/// The hour of day (0-23) with the most productive time, if any data exists.
pub fn golden_hour(hourly: &[u64; 24]) -> Option<usize> {
    let (h, v) = hourly.iter().enumerate().max_by_key(|(_, v)| **v)?;
    if *v > 0 { Some(h) } else { None }
}

pub fn fmt_hour(h: usize) -> String {
    format!("{:02}:00", h)
}

pub fn weekday_name(date_key: &str) -> &'static str {
    use chrono::Datelike;
    match crate::domain::parse_date_key(date_key).map(|d| d.weekday().num_days_from_monday()) {
        Some(0) => "Mon",
        Some(1) => "Tue",
        Some(2) => "Wed",
        Some(3) => "Thu",
        Some(4) => "Fri",
        Some(5) => "Sat",
        Some(6) => "Sun",
        _ => "---",
    }
}

pub const MANTRAS: [&str; 8] = [
    "Show up for 25 minutes. That is the whole job.",
    "Small sessions, stacked daily, beat heroic weekends.",
    "Don't break the chain. One minute still counts.",
    "Start before you feel ready. Momentum arrives later.",
    "Friction is the enemy. Keep the first step tiny.",
    "Close the tabs. Open the book. Press Space.",
    "Future you is watching today's streak. Be kind to them.",
    "Reward comes after the work. Make the work easy to begin.",
];

/// Card with a bar chart of the most recent days (optionally for one skill). Stretches to fill.
pub fn recent_days_card(f: &mut ratatui::Frame, area: ratatui::layout::Rect, c: &Ctx, title: &str, category: Option<&str>, mark_key: Option<&str>) {
    let p = &c.p;
    let inner = crate::tui::ui::card(f, area, p, "▂", title, p.accent, false);
    let n = ((inner.width as usize) / 5).clamp(5, 21);
    let incl = c.rt.prefs.focus.count_interrupted_in_totals;
    let mut items = Vec::new();
    let mut mark = None;
    for back in (0..n as i64).rev() {
        let key = crate::domain::shift_date_key(&c.today, -back);
        let mut secs = 0u64;
        for s in c.rt.vault.sessions.iter().filter(|s| s.activity_kind.is_productive() && (incl || s.completion_state != CompletionState::Interrupted)) {
            if category.is_some_and(|id| s.category_id != id) { continue; }
            if s.day_slices.is_empty() {
                if s.primary_date_key == key { secs += s.elapsed_seconds; }
            } else {
                secs += s.day_slices.iter().filter(|d| d.date_key == key).map(|d| d.seconds).sum::<u64>();
            }
        }
        if Some(key.as_str()) == mark_key { mark = Some(items.len()); }
        items.push((key.get(8..10).unwrap_or("").to_string(), secs, p.accent));
    }
    crate::tui::ui::vbars(f, inner, p, &items, mark, true);
}

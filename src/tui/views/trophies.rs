//! Trophies: a 3-column grid of achievement cards that stretches to fill the screen.

use super::Ctx;
use crate::domain::{AchievementFamily, RarityTier};
use crate::tui::ui;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    Frame,
};

const FAMILIES: [AchievementFamily; 6] = [
    AchievementFamily::Consistency,
    AchievementFamily::DeepWork,
    AchievementFamily::Reading,
    AchievementFamily::Exploration,
    AchievementFamily::PersonalImprovement,
    AchievementFamily::LongTermMilestones,
];

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let r = ui::rows(area, &[Constraint::Length(if area.height >= 16 { 5 } else { 0 }), Constraint::Length(1), Constraint::Min(5)]);

    let unlocked = c.items.iter().filter(|i| i.is_unlocked).count();
    if r[0].height > 0 {
        let cols = ui::fit_cols(r[0], &[1, 1, 1, 1], 20);
        let count = |t: RarityTier| (c.items.iter().filter(|i| i.is_unlocked && i.spec.rarity == t).count(), c.items.iter().filter(|i| i.spec.rarity == t).count());
        let bonus: u64 = c.items.iter().filter(|i| i.is_unlocked).map(|i| i.spec.rarity.xp_bonus()).sum();
        let (cm, cmt) = count(RarityTier::Common);
        let (rr, rrt) = count(RarityTier::Rare);
        let (my, myt) = count(RarityTier::Mythic);
        let tiles = [
            ("✦", "Collected", format!("{}/{}", unlocked, c.items.len()), format!("{}% complete", unlocked * 100 / c.items.len().max(1)), p.accent),
            ("◇", "Common", format!("{}/{}", cm, cmt), "easy wins".into(), p.dim),
            ("✦", "Rare", format!("{}/{}", rr, rrt), "real effort".into(), p.accent2),
            ("❂", "Mythic", format!("{}/{}", my, myt), format!("+{} XP bonus", bonus), p.accent3),
        ];
        for (i, t) in tiles.into_iter().enumerate().take(cols.len()) {
            ui::tile(f, cols[i], p, t.0, t.1, t.2, t.3, t.4);
        }
    }

    // family chips
    let mut s = vec![Span::styled(" Tab ", Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD)), Span::raw(" ")];
    let all_on = c.rt.achievements_family_filter.is_none();
    s.push(Span::styled(" All ", if all_on { Style::default().fg(p.on_accent).bg(p.accent2).add_modifier(Modifier::BOLD) } else { Style::default().fg(p.dim) }));
    for fam in FAMILIES {
        let on = c.rt.achievements_family_filter == Some(fam);
        s.push(Span::styled(format!(" {} ", fam.label()), if on { Style::default().fg(p.on_accent).bg(p.accent2).add_modifier(Modifier::BOLD) } else { Style::default().fg(p.dim) }));
    }
    f.render_widget(ratatui::widgets::Paragraph::new(Line::from(s)), r[1]);

    let list: Vec<_> = c.items.iter().filter(|i| c.rt.achievements_family_filter.is_none_or(|fam| i.spec.family == fam)).collect();
    if list.is_empty() { return; }
    let cur = c.rt.achievements_cursor.min(list.len() - 1);
    let ncols = 3usize;
    let total_rows = list.len().div_ceil(ncols);
    let vis_rows = ((r[2].height / 7).max(1) as usize).min(total_rows);
    let cur_row = cur / ncols;
    let first_row = if cur_row >= vis_rows { cur_row + 1 - vis_rows } else { 0 };
    let row_rects = ui::rows(r[2], &vec![Constraint::Fill(1); vis_rows]);
    for (ri, rr) in row_rects.iter().enumerate() {
        let col_rects = ui::rows_h(*rr, ncols);
        for (ci, cr) in col_rects.iter().enumerate() {
            let idx = (first_row + ri) * ncols + ci;
            let Some(it) = list.get(idx) else { 
                let inner = ui::card(f, *cr, p, "·", "More to come", p.border, false);
                ui::center(f, inner, p, vec![Line::from(Span::styled("Keep going — new trophies", Style::default().fg(p.dim))), Line::from(Span::styled("appear as you grow.", Style::default().fg(p.dim)))]);
                continue;
            };
            let sel = idx == cur;
            let rar = match it.spec.rarity { RarityTier::Common => p.dim, RarityTier::Uncommon => p.good, RarityTier::Rare => p.accent2, RarityTier::Mythic => p.accent3 };
            let col = if it.is_unlocked { rar } else { p.border };
            let inner = ui::card(f, *cr, p, it.spec.rarity.glyph(), &ui::clamp_str(it.spec.title, cr.width.saturating_sub(8) as usize), col, sel);
            let w = inner.width as usize;
            let mut lines = vec![Line::from(Span::styled(it.spec.rarity.label(), Style::default().fg(rar).add_modifier(Modifier::BOLD)))];
            lines.push(Line::from(Span::styled(it.spec.description, Style::default().fg(if it.is_unlocked { p.text } else { p.dim }))));
            let mut m = Vec::new();
            m.extend(ui::meter(w.saturating_sub(10).max(4), it.progress_percent as f64 / 100.0, if it.is_unlocked { p.good } else { col }, p.card_hi));
            m.push(Span::styled(format!(" {}/{}", it.current_value.min(it.spec.target), it.spec.target), Style::default().fg(p.dim)));
            let foot = if it.is_unlocked {
                Line::from(Span::styled(format!("✔ Unlocked {} · +{} XP", it.unlocked_at_utc.map(|d| d.with_timezone(&chrono::Local).format("%d %b").to_string()).unwrap_or_default(), it.spec.rarity.xp_bonus()), Style::default().fg(p.good).add_modifier(Modifier::BOLD)))
            } else {
                Line::from(Span::styled(format!("Reward: +{} XP", it.spec.rarity.xp_bonus()), Style::default().fg(p.accent3)))
            };
            let mut all = lines;
            all.push(Line::from(m));
            all.push(foot);
            ui::text(f, inner, all);
        }
    }
}

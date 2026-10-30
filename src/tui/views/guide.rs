//! Guide: the in-app manual. Plain words, short sentences, examples.

use super::Ctx;
use crate::tui::ui;
use ratatui::{
    layout::{Constraint, Rect},
    style::{Modifier, Style},
    text::{Line, Span},
    Frame,
};

pub const TOPICS: &[(&str, &str)] = &[
    ("★", "Start here"),
    ("◉", "The timer & ring"),
    ("▣", "Reading deck"),
    ("◌", "Zen mode"),
    ("✦", "XP, levels & chest"),
    ("ϟ", "Streaks & quests"),
    ("❂", "Trophies"),
    ("▲", "Stats & heatmap"),
    ("◆", "Skills & goals"),
    ("▸", "Planning ahead"),
    ("≡", "The log"),
    ("⚙", "Settings & backups"),
    ("◈", "All keys"),
    ("?", "Questions & fixes"),
];

// Kinds: H heading · P paragraph · B bullet · E example · K key (key|meaning) · T tip
type Item = (char, &'static str);

const START: &[Item] = &[
    ('H', "What is Chronos?"),
    ('P', "Chronos is a timer that turns focused work into a game. You work for a set time, Chronos records it, and you earn points (XP), levels and trophies. Everything stays on your computer."),
    ('H', "Your first 2 minutes"),
    ('B', "Press 2 to open the Focus screen."),
    ('B', "Press Space. The ring starts filling. That's it, you are focusing."),
    ('B', "When the time ends, press Enter. Add a short note if you like, then press Enter again."),
    ('B', "Press 1 to go Home and watch your XP, streak and quests update."),
    ('E', "Example: You press Space at 9:00 and finish at 9:25. You just earned about 65 XP and kept your streak alive."),
    ('H', "How to move around"),
    ('P', "The tab strip at the top shows every screen. Press the number key to jump there: 1 Home, 2 Focus, 3 Reading, 4 Stats, 5 Log, 6 Trophies, 7 Skills, 8 Settings, 9 Guide (this page)."),
    ('T', "The bar at the bottom always shows the keys that work on the screen you are on."),
    ('H', "The big idea"),
    ('P', "Discipline = Value + Reward - Friction. Make the work worth it (value), celebrate it (reward), and make starting easy (less friction). Chronos helps with all three."),
];

const TIMER: &[Item] = &[
    ('H', "The ring"),
    ('P', "The ring is a clock you can see at a glance. It fills as time passes. The number inside is how much time is LEFT. A bright dot sits at the tip while it runs."),
    ('H', "Starting and stopping"),
    ('K', "Space|Start, pause or continue the timer"),
    ('K', "Enter|Finish early and save it (you can add a note)"),
    ('K', "R|Give up on this session. It is saved as 'cut short'"),
    ('K', "+ / -|Add or remove 5 minutes (only before you finish)"),
    ('E', "Example: You planned 25 minutes but feel great. Press + twice to make it 35."),
    ('H', "Focus, breaks and rhythm"),
    ('K', "F|Switch to a focus block"),
    ('K', "B|Short break (stand up, drink water)"),
    ('K', "L|Long break (after a full cycle)"),
    ('K', "M|Change rhythm: Harmonic 25/5, Deep Block 50/10, Monolith 90/20, Custom"),
    ('P', "A rhythm is just a pattern: work, small break, work, small break... then a long break. The little dots on the Focus screen show how far you are in the cycle."),
    ('H', "Give it a goal"),
    ('P', "Press I to write what you want to finish. A clear goal like 'Finish chapter 3 notes' makes starting much easier than 'study'."),
    ('H', "Pick a skill"),
    ('P', "Press C to choose which skill (like Coding or Math) this time belongs to. Your Stats will show where your time goes."),
    ('T', "If the app closes by accident, your running timer is saved. It comes back paused when you reopen Chronos."),
];

const READING: &[Item] = &[
    ('H', "Your bookshelf"),
    ('P', "Press 3 for the Reading deck. Each book is a card showing a progress bar, the page you are on, your reading speed and how many sessions you did."),
    ('K', "N|Add a book (title, author, pages)"),
    ('K', "E|Edit the selected book"),
    ('K', "Up / Down|Choose a book"),
    ('K', "Enter|Start reading the selected book"),
    ('K', "S|Change the book's state (see below)"),
    ('K', "D|Delete the selected book (asks first)"),
    ('H', "Book states"),
    ('P', "Every book has one of four states. Press S on a book to move to the next one."),
    ('B', "Have: you own it or want it, but haven't started."),
    ('B', "Reading: you are in the middle of it."),
    ('B', "Read: you finished it (marking it Read jumps to the last page)."),
    ('B', "Abandoned: you started it and decided to stop."),
    ('T', "States change by themselves too: starting a session makes a book Reading, and reaching the last page makes it Read."),
    ('P', "Deleting a book only removes it from the shelf. Your past reading sessions stay in your history and stats."),
    ('H', "How long is a reading session?"),
    ('P', "Reading has its own length instead of focus rhythms like 25/5. Press + or - to change it by 5 minutes, or set the default in Settings > Reading. The M key (rhythm) belongs to the Focus screen only."),
    ('H', "A reading session"),
    ('B', "Choose a book and press Enter. The timer starts with the page you stopped at last time."),
    ('B', "When you finish, press Enter and type the page you reached."),
    ('B', "Chronos works out how many pages you read and updates the book."),
    ('E', "Example: You start at page 120 and stop at page 138. That is 18 pages. If it took 30 minutes, your speed is 36 pages per hour."),
    ('T', "Another timer already running? Finish or reset it first. Chronos will not throw away a session by accident."),
    ('P', "Reading earns XP too: 2 XP per minute, plus 1 XP for every 2 pages."),
];

const ZEN: &[Item] = &[
    ('H', "Just the ring"),
    ('P', "Zen mode hides everything except the ring and the time. No menus, no numbers, no hints. Nothing to look at except the one thing you are doing."),
    ('K', "Z|Turn zen mode on or off (on the Focus or Reading screen)"),
    ('K', "Esc|Leave zen mode"),
    ('K', "Space|Still pauses and resumes"),
    ('E', "Example: Press 2, then Space to start, then Z. Put your phone away. When the ring is full, press Z to come back."),
    ('T', "You can make zen mode start automatically. See Settings."),
];

const XP: &[Item] = &[
    ('H', "XP: points for effort"),
    ('P', "XP is your score. You earn it for real work, never for just opening the app."),
    ('B', "2 XP for every minute you focus or read"),
    ('B', "15 XP for each finished session"),
    ('B', "10 XP when you add a note"),
    ('B', "20 XP for finishing a planned block"),
    ('B', "1 XP for every 2 pages you read"),
    ('P', "Only the first 5 hours of each day give XP. This keeps things healthy. Rest is part of getting better."),
    ('H', "Levels"),
    ('P', "Enough XP raises your level and gives you a new title, like Apprentice of Attention. The bar at the top of every screen shows how close you are."),
    ('H', "The Daily Chest"),
    ('P', "Reach your daily focus goal and the chest opens with a surprise prize between 30 and 150 XP. A rare jackpot gives 150. The prize is different every day, so it is fun to check."),
    ('E', "Example: Your goal is 2 hours. After 2 hours of focus the chest on the Home screen changes from LOCKED to OPEN and shows +62 XP."),
    ('T', "Set a daily goal of at least 60 minutes in Settings > Goals. Smaller goals do not open the chest."),
];

const STREAK: &[Item] = &[
    ('H', "Streaks"),
    ('P', "A streak counts how many days in a row you did something. Even 1 minute counts. The goal is to show up, not to be perfect."),
    ('B', "The row of dots on Home shows the last 7 days. A filled dot means you did something that day."),
    ('B', "When the streak is at risk (nothing yet today), the number glows orange and Chronos tells you."),
    ('B', "Milestones: 3, 7, 14, 30, 60, 100, 200 and 365 days."),
    ('T', "Missed a day? Don't be harsh on yourself. Start again today. The 'Returner' trophy rewards coming back."),
    ('H', "Daily quests"),
    ('P', "Quests are small daily challenges on the Home screen: a 25-minute warm-up, 20 minutes of reading, your daily goal, and a two-mode day (10 minutes focus plus 10 minutes reading). Each one has a progress bar."),
    ('E', "Example: You focus for 15 minutes. The warm-up quest shows 60%. Ten more minutes completes it."),
];

const TROPHIES: &[Item] = &[
    ('H', "Trophies"),
    ('P', "Press 6 to see every trophy. Grey ones are locked and show how close you are. Bright ones are yours."),
    ('B', "Four rarities: Common, Uncommon, Rare, Mythic. Rarer trophies give more bonus XP."),
    ('B', "Six families: Consistency, Deep Work, Reading, Exploration, Personal Improvement, Long-Term."),
    ('K', "Arrows|Move between trophies"),
    ('K', "Tab|Show one family at a time"),
    ('T', "A trophy can never be taken away. Once it's yours, it's yours."),
];

const STATS: &[Item] = &[
    ('H', "Stats: five views"),
    ('P', "Press 4. Press Tab to flip between five views. The six number cards at the top always show the totals for the date range you picked."),
    ('B', "Overview: daily bars, insights, your best day and longest session."),
    ('B', "Rhythm: which hours and weekdays you work best. Your 'golden hour' is highlighted."),
    ('B', "Skills: how your time is split, and your goals per skill."),
    ('B', "Days: pick any day on the heatmap and see its sessions."),
    ('B', "Review: your week, advice, and a forecast of when you reach the next level."),
    ('H', "Changing the range"),
    ('K', "[ and ]|Shorter or longer range: Today, 7d, 30d, 90d, 1y, All"),
    ('K', "C|Look at one skill only"),
    ('K', "X / O|Export CSV / JSON"),
    ('H', "The heatmap"),
    ('P', "Every square is one day. Darker means less, brighter means more. The brightest square means you reached your daily goal."),
    ('K', "Left / Right|Move a week"),
    ('K', "Up / Down|Move a day"),
    ('K', "Home|Jump back to today"),
    ('E', "Example: You notice every Thursday is dark. Plan an easy 25-minute session for Thursdays and the streak stays alive."),
];

const SKILLS: &[Item] = &[
    ('H', "What is a skill?"),
    ('P', "A skill is a label for your time, like Coding, Math or Fiction. Pick one before you start and Chronos shows you where your hours really go."),
    ('K', "7|Open the Skills screen"),
    ('K', "N|Create a skill (name, colour, icon)"),
    ('K', "E|Edit, including goals"),
    ('K', "S|Use this skill for new sessions"),
    ('K', "A|Archive (hide) it. History stays"),
    ('K', "D|Delete it. Its sessions move to another skill"),
    ('H', "Goals"),
    ('P', "Each skill can have a daily and a weekly goal in minutes. The progress bars turn green when you reach them."),
    ('P', "You also have overall goals for focus and reading in Settings > Goals."),
    ('E', "Example: Coding has a goal of 90 minutes a day. After 45 minutes the bar shows 50%."),
];

const PLAN: &[Item] = &[
    ('H', "Quest queue"),
    ('P', "On Home, the Quest Queue is your to-do list for focus time. Planning ahead removes friction: when the time comes you just press Enter."),
    ('K', "N|Plan a new block (title, type, skill, length, priority)"),
    ('K', "Up / Down|Choose a block"),
    ('K', "Enter|Launch it. The timer is ready with your title and skill"),
    ('E', "Example: Tonight you plan 'Read 20 pages' for 30 minutes. Tomorrow you press 1, Enter, Space. Done."),
    ('T', "Finishing a planned block gives 20 bonus XP."),
];

const LOG: &[Item] = &[
    ('H', "Every session, in one list"),
    ('P', "Press 5. The list shows when, what, how long and how much XP. The card on the right shows everything about the selected session, including your notes."),
    ('K', "/|Search by date, goal, notes or book title"),
    ('K', "F|Filter by type"),
    ('K', "S|Filter by finished or cut short"),
    ('K', "C|Filter by skill"),
    ('K', "E|Edit a session (fix the time, skill, pages)"),
    ('K', "D|Delete a session (it asks you first)"),
    ('K', "X / O|Export CSV / JSON"),
    ('T', "Forgot to start the timer? Edit a session afterwards to correct the minutes."),
];

const SETTINGS: &[Item] = &[
    ('H', "Settings"),
    ('P', "Press 8. Tab changes group, Up/Down chooses a row, Left/Right (or Enter) changes the value. Changes are saved right away."),
    ('H', "Themes"),
    ('P', "Press T anywhere to switch theme. There are nine, including Monochrome: pure black, white and grey, with no colour at all."),
    ('H', "Backups"),
    ('K', "B|Make a backup now"),
    ('K', "L|Restore the newest backup (shows a preview first)"),
    ('P', "Chronos also keeps an automatic safety copy every time it starts."),
    ('H', "Resets"),
    ('P', "Resets are in the last group. You must type a phrase to confirm, and a backup is made first."),
    ('T', "Your data lives in your user data folder as plain files. You can copy or read them any time."),
];

const KEYS: &[Item] = &[
    ('H', "Everywhere"),
    ('K', "1-9|Go to a screen"),
    ('K', "Space|Start / pause the timer"),
    ('K', "+ / -|Change timer length by 5 minutes"),
    ('K', "T|Next theme"),
    ('K', "?|This guide"),
    ('K', "Shift+K|Quick key list pop-up"),
    ('K', "Q or Esc|Quit (asks if a session is running)"),
    ('K', "Ctrl+C|Quit now"),
    ('H', "Focus & Reading"),
    ('K', "Enter|Finish and save"),
    ('K', "R|Abandon session"),
    ('K', "F / B / L|Focus / short break / long break"),
    ('K', "M|Rhythm"),
    ('K', "C|Skill"),
    ('K', "I|Write a goal"),
    ('K', "Z|Zen mode"),
    ('H', "Lists"),
    ('K', "Up/Down or j/k|Move"),
    ('K', "N / E / D|New / Edit / Delete"),
    ('K', "Tab|Next group or lens"),
];

const FAQ: &[Item] = &[
    ('H', "Common questions"),
    ('P', "Q: I pressed Enter and nothing happened."),
    ('P', "A: Enter finishes a session only if the timer has started. Press Space first."),
    ('P', "Q: The timer won't let me start a queued block."),
    ('P', "A: Another session is already running. Finish it (Enter) or reset it (R) first."),
    ('P', "Q: My streak says 0 but I worked yesterday."),
    ('P', "A: A day counts when you log at least 1 minute. If today has nothing yet, yesterday's streak is still alive until midnight."),
    ('P', "Q: Where is my data?"),
    ('P', "A: Settings > Groups shows the path. It is a plain file you can back up."),
    ('P', "Q: Colours look wrong."),
    ('P', "A: Set CHRONOS_COLOR_MODE to truecolor, 256 or 16 before starting Chronos."),
    ('P', "Q: The screen looks cramped."),
    ('P', "A: Make the window larger. Chronos shows more cards when there is room, and hides the least important ones when there isn't."),
    ('T', "Still stuck? Run 'chronos doctor' in a normal terminal to check your data."),
];

fn items(topic: usize) -> &'static [Item] {
    match topic {
        0 => START, 1 => TIMER, 2 => READING, 3 => ZEN, 4 => XP, 5 => STREAK, 6 => TROPHIES,
        7 => STATS, 8 => SKILLS, 9 => PLAN, 10 => LOG, 11 => SETTINGS, 12 => KEYS, _ => FAQ,
    }
}

fn wrap(text: &str, width: usize) -> Vec<String> {
    let width = width.max(8);
    let mut out = Vec::new();
    let mut cur = String::new();
    for word in text.split_whitespace() {
        if !cur.is_empty() && cur.chars().count() + 1 + word.chars().count() > width {
            out.push(std::mem::take(&mut cur));
        }
        if !cur.is_empty() { cur.push(' '); }
        cur.push_str(word);
    }
    if !cur.is_empty() { out.push(cur); }
    out
}

fn build(c: &Ctx, topic: usize, width: usize) -> Vec<Line<'static>> {
    let p = &c.p;
    let mut lines: Vec<Line> = Vec::new();
    for (kind, text) in items(topic) {
        match kind {
            'H' => {
                if !lines.is_empty() { lines.push(Line::from("")); }
                lines.push(Line::from(Span::styled(format!("▌ {}", text), Style::default().fg(p.accent).add_modifier(Modifier::BOLD))));
            }
            'P' => { for l in wrap(text, width) { lines.push(Line::from(Span::styled(l, Style::default().fg(p.text)))); } }
            'B' => {
                for (i, l) in wrap(text, width.saturating_sub(3)).into_iter().enumerate() {
                    lines.push(Line::from(vec![Span::styled(if i == 0 { " • " } else { "   " }, Style::default().fg(p.accent2)), Span::styled(l, Style::default().fg(p.text))]));
                }
            }
            'E' => {
                for l in wrap(text, width.saturating_sub(4)) { lines.push(Line::from(vec![Span::styled(" ┃ ", Style::default().fg(p.accent3)), Span::styled(l, Style::default().fg(p.accent3))])); }
            }
            'T' => {
                for (i, l) in wrap(text, width.saturating_sub(4)).into_iter().enumerate() {
                    lines.push(Line::from(vec![Span::styled(if i == 0 { " ✧ " } else { "   " }, Style::default().fg(p.good)), Span::styled(l, Style::default().fg(p.good))]));
                }
            }
            _ => {
                let (k, d) = text.split_once('|').unwrap_or((text, ""));
                lines.push(Line::from(vec![Span::styled(format!(" {} ", k), Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD)), Span::styled(format!(" {}", d), Style::default().fg(p.text))]));
            }
        }
    }
    lines
}

pub fn render(f: &mut Frame, area: Rect, c: &Ctx) {
    let p = &c.p;
    let topic = c.rt.guide_topic % TOPICS.len();
    let (list_area, body_area) = if area.width >= 70 {
        let split = ratatui::layout::Layout::default()
            .direction(ratatui::layout::Direction::Horizontal)
            .constraints([Constraint::Length(if area.width >= 90 { 27 } else { 22 }), Constraint::Min(30)])
            .split(area);
        (Some(split[0]), split[1])
    } else {
        (None, area)
    };
    if let Some(la) = list_area {
        let inner = ui::card(f, la, p, "☰", "Topics (Tab)", p.accent, false);
        let mut lines: Vec<Line> = Vec::new();
        for (i, (icon, title)) in TOPICS.iter().enumerate() {
            let on = i == topic;
            lines.push(Line::from(Span::styled(ui::pad_right(&format!(" {} {}", icon, title), inner.width as usize), if on { Style::default().fg(p.on_accent).bg(p.accent).add_modifier(Modifier::BOLD) } else { Style::default().fg(p.text) })));
        }
        let hints: Vec<String> = ["", "── Cheat sheet ──", "Space  start / pause", "Enter  finish & save", "Z      zen mode", "T      change theme", "N      add something new", "Q      quit", "", "── Reading this page ──", "↑↓       scroll", "Tab ←→   next topic", "PgUp/Dn  fast scroll", "", "Tip: every screen shows its", "own keys at the bottom."].iter().map(|s| s.to_string()).collect();
        ui::fill_hints(&mut lines, inner.height, p, &hints);
        ui::text(f, inner, lines);
    }
    let (icon, title) = TOPICS[topic];
    let inner = ui::card(f, body_area, p, icon, &format!("{}  ({}/{})", title, topic + 1, TOPICS.len()), p.accent2, false);
    let lines = build(c, topic, inner.width.saturating_sub(1) as usize);
    // sticky footer row: where to go next
    let prev = TOPICS[(topic + TOPICS.len() - 1) % TOPICS.len()].1;
    let next = TOPICS[(topic + 1) % TOPICS.len()].1;
    if inner.height > 3 {
        let foot = Rect::new(inner.x, inner.y + inner.height - 1, inner.width, 1);
        let txt = format!(" ◂ {}   ·   Tab   ·   {} ▸", prev, next);
        f.render_widget(ratatui::widgets::Paragraph::new(Line::from(Span::styled(ui::pad_right(&txt, inner.width as usize), Style::default().fg(p.accent2).bg(p.card_hi)))), foot);
    }
    let inner = Rect::new(inner.x, inner.y, inner.width, inner.height.saturating_sub(if inner.height > 3 { 1 } else { 0 }));
    let h = inner.height as usize;
    let max = lines.len().saturating_sub(h);
    c.rt.guide_max_scroll.set(max as u16);
    let off = (c.rt.guide_scroll as usize).min(max);
    let shown: Vec<Line> = lines.into_iter().skip(off).take(h).collect();
    ui::text(f, inner, shown);
    if let Some(pos) = (off * (inner.height as usize).saturating_sub(1)).checked_div(max) {
        let bar_h = inner.height as usize;
        let buf = f.buffer_mut();
        for y in 0..bar_h {
            let cell = buf.get_mut(body_area.x + body_area.width - 1, inner.y + y as u16);
            cell.set_symbol(if y == pos { "█" } else { "│" });
            cell.set_style(Style::default().fg(if y == pos { p.accent2 } else { p.border }).bg(p.card));
        }
    }
}

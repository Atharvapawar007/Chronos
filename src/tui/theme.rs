//! Colour themes. Every colour the UI uses comes from a [`Palette`], so a theme is just data.

use crate::domain::models::parse_hex_rgb;
use ratatui::style::Color;
use std::env;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeId {
    Neon,
    Tokyo,
    Mocha,
    Dracula,
    Matrix,
    Blood,
    Nordic,
    Gruvbox,
    Mono,
}

impl ThemeId {
    pub const ALL: [ThemeId; 9] = [
        ThemeId::Neon,
        ThemeId::Tokyo,
        ThemeId::Mocha,
        ThemeId::Dracula,
        ThemeId::Matrix,
        ThemeId::Blood,
        ThemeId::Nordic,
        ThemeId::Gruvbox,
        ThemeId::Mono,
    ];

    pub fn key(self) -> &'static str {
        match self {
            ThemeId::Neon => "cyberpunk_neon",
            ThemeId::Tokyo => "tokyonight_ops",
            ThemeId::Mocha => "catppuccin_mocha",
            ThemeId::Dracula => "dracula_synth",
            ThemeId::Matrix => "matrix_phosphor",
            ThemeId::Blood => "blood_moon",
            ThemeId::Nordic => "nordic_aurora",
            ThemeId::Gruvbox => "gruvbox_obsidian",
            ThemeId::Mono => "monochrome",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Neon => "Cyberpunk Neon",
            ThemeId::Tokyo => "Tokyo Night",
            ThemeId::Mocha => "Catppuccin Mocha",
            ThemeId::Dracula => "Dracula",
            ThemeId::Matrix => "Matrix Green",
            ThemeId::Blood => "Blood Moon",
            ThemeId::Nordic => "Nordic Aurora",
            ThemeId::Gruvbox => "Gruvbox",
            ThemeId::Mono => "Monochrome",
        }
    }

    pub fn from_key(key: &str) -> Self {
        match key.trim().to_lowercase().as_str() {
            "tokyonight_ops" | "tokyonight" | "athenaeum_dark" => ThemeId::Tokyo,
            "catppuccin_mocha" | "catppuccin" => ThemeId::Mocha,
            "dracula_synth" | "dracula" => ThemeId::Dracula,
            "matrix_phosphor" | "matrix" | "botanical_sage" => ThemeId::Matrix,
            "blood_moon" | "terracotta_dusk" => ThemeId::Blood,
            "nordic_aurora" | "nord" | "monolith_slate" => ThemeId::Nordic,
            "gruvbox_obsidian" | "gruvbox" => ThemeId::Gruvbox,
            "monochrome" | "mono" | "minimal" | "bw" => ThemeId::Mono,
            _ => ThemeId::Neon,
        }
    }

    pub fn next(self) -> Self {
        let i = Self::ALL.iter().position(|t| *t == self).unwrap_or(0);
        Self::ALL[(i + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColorDepth {
    True,
    Ansi256,
    Ansi16,
}

impl ColorDepth {
    pub fn detect() -> Self {
        if let Ok(v) = env::var("CHRONOS_COLOR_MODE") {
            match v.trim().to_lowercase().as_str() {
                "truecolor" | "24bit" | "rgb" => return ColorDepth::True,
                "256" | "ansi256" => return ColorDepth::Ansi256,
                "16" | "ansi16" | "basic" => return ColorDepth::Ansi16,
                _ => {}
            }
        }
        let term = env::var("TERM").unwrap_or_default().to_lowercase();
        if term == "linux" || term == "dumb" {
            ColorDepth::Ansi16
        } else {
            ColorDepth::True
        }
    }

    fn map(self, r: u8, g: u8, b: u8) -> Color {
        match self {
            ColorDepth::True => Color::Rgb(r, g, b),
            ColorDepth::Ansi256 => Color::Indexed(to_256(r, g, b)),
            ColorDepth::Ansi16 => to_16(r, g, b),
        }
    }
}

fn to_256(r: u8, g: u8, b: u8) -> u8 {
    if r == g && g == b {
        return match r {
            0..=7 => 16,
            248..=255 => 231,
            v => 232 + ((v as u16 - 8) * 24 / 247) as u8,
        };
    }
    let q = |v: u8| ((v as u16 * 5 + 127) / 255) as u8;
    16 + 36 * q(r) + 6 * q(g) + q(b)
}

fn to_16(r: u8, g: u8, b: u8) -> Color {
    let lum = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
    let bright = lum > 150;
    let (hr, hg, hb) = (r > 110, g > 110, b > 110);
    match (hr, hg, hb, bright) {
        (false, false, false, _) => {
            if lum > 60 { Color::DarkGray } else { Color::Black }
        }
        (true, false, false, b) => if b { Color::LightRed } else { Color::Red },
        (false, true, false, b) => if b { Color::LightGreen } else { Color::Green },
        (false, false, true, b) => if b { Color::LightBlue } else { Color::Blue },
        (true, true, false, b) => if b { Color::LightYellow } else { Color::Yellow },
        (true, false, true, b) => if b { Color::LightMagenta } else { Color::Magenta },
        (false, true, true, b) => if b { Color::LightCyan } else { Color::Cyan },
        (true, true, true, b) => if b { Color::White } else { Color::Gray },
    }
}

/// Every colour the interface uses.
#[derive(Debug, Clone, Copy)]
pub struct Palette {
    pub id: ThemeId,
    pub depth: ColorDepth,
    pub bg: Color,
    pub card: Color,
    pub card_hi: Color,
    pub border: Color,
    pub text: Color,
    pub dim: Color,
    pub accent: Color,
    pub accent2: Color,
    pub accent3: Color,
    pub good: Color,
    pub warn: Color,
    pub bad: Color,
    pub info: Color,
    pub on_accent: Color,
    pub heat: [Color; 5],
}

struct Spec {
    bg: &'static str,
    card: &'static str,
    card_hi: &'static str,
    border: &'static str,
    text: &'static str,
    dim: &'static str,
    accent: &'static str,
    accent2: &'static str,
    accent3: &'static str,
    good: &'static str,
    warn: &'static str,
    bad: &'static str,
    info: &'static str,
    on_accent: &'static str,
    heat: [&'static str; 5],
}

fn spec(id: ThemeId) -> Spec {
    match id {
        ThemeId::Neon => Spec { bg: "#07070f", card: "#0e0e1c", card_hi: "#181830", border: "#2a2a52", text: "#e8e8ff", dim: "#8585ad", accent: "#00f0ff", accent2: "#ff2bd6", accent3: "#f9f002", good: "#39ff88", warn: "#ffb454", bad: "#ff4d6d", info: "#5ab0ff", on_accent: "#04040a", heat: ["#171728", "#0d4f5c", "#0890a3", "#00c8dc", "#00f0ff"] },
        ThemeId::Tokyo => Spec { bg: "#16161e", card: "#1a1b26", card_hi: "#24283b", border: "#3b4261", text: "#c0caf5", dim: "#6b7399", accent: "#7aa2f7", accent2: "#bb9af7", accent3: "#e0af68", good: "#9ece6a", warn: "#e0af68", bad: "#f7768e", info: "#7dcfff", on_accent: "#16161e", heat: ["#20222f", "#2f4a7a", "#4a72b8", "#6a92e6", "#7aa2f7"] },
        ThemeId::Mocha => Spec { bg: "#11111b", card: "#181825", card_hi: "#313244", border: "#45475a", text: "#cdd6f4", dim: "#7f849c", accent: "#cba6f7", accent2: "#f5c2e7", accent3: "#f9e2af", good: "#a6e3a1", warn: "#fab387", bad: "#f38ba8", info: "#89dceb", on_accent: "#11111b", heat: ["#1e1e2e", "#4a3a68", "#7a5fa6", "#a88ad8", "#cba6f7"] },
        ThemeId::Dracula => Spec { bg: "#191a21", card: "#21222c", card_hi: "#343746", border: "#44475a", text: "#f8f8f2", dim: "#8a8fb0", accent: "#bd93f9", accent2: "#ff79c6", accent3: "#f1fa8c", good: "#50fa7b", warn: "#ffb86c", bad: "#ff5555", info: "#8be9fd", on_accent: "#191a21", heat: ["#262833", "#4b3a73", "#7557b0", "#9d78e6", "#bd93f9"] },
        ThemeId::Matrix => Spec { bg: "#010601", card: "#041004", card_hi: "#0a2a0a", border: "#10491a", text: "#b9ffb9", dim: "#4f9a58", accent: "#00ff41", accent2: "#7dff9a", accent3: "#c8ff00", good: "#00ff41", warn: "#d6ff4d", bad: "#ff6060", info: "#3affc0", on_accent: "#010601", heat: ["#0a1a0a", "#0a4d1a", "#0a8a2c", "#08c83a", "#00ff41"] },
        ThemeId::Blood => Spec { bg: "#0d0505", card: "#160a0a", card_hi: "#2a1212", border: "#552222", text: "#ffe3de", dim: "#a37575", accent: "#ff4d4d", accent2: "#ff8a5c", accent3: "#ffc15c", good: "#7bd88f", warn: "#ffb347", bad: "#ff2e4d", info: "#ff9aa2", on_accent: "#0d0505", heat: ["#1f1010", "#5a1f1f", "#9a2e2e", "#d63c3c", "#ff4d4d"] },
        ThemeId::Nordic => Spec { bg: "#242933", card: "#2e3440", card_hi: "#3b4252", border: "#4c566a", text: "#eceff4", dim: "#8f9bb3", accent: "#88c0d0", accent2: "#b48ead", accent3: "#ebcb8b", good: "#a3be8c", warn: "#d08770", bad: "#bf616a", info: "#81a1c1", on_accent: "#242933", heat: ["#353c4a", "#3d6a78", "#5a99ab", "#74b0c2", "#88c0d0"] },
        ThemeId::Gruvbox => Spec { bg: "#151718", card: "#1d2021", card_hi: "#32302f", border: "#504945", text: "#ebdbb2", dim: "#a89984", accent: "#fabd2f", accent2: "#fe8019", accent3: "#8ec07c", good: "#b8bb26", warn: "#fe8019", bad: "#fb4934", info: "#83a598", on_accent: "#151718", heat: ["#282828", "#6b5a1a", "#a8841f", "#d9a527", "#fabd2f"] },
        ThemeId::Mono => Spec { bg: "#000000", card: "#080808", card_hi: "#161616", border: "#3c3c3c", text: "#ffffff", dim: "#8a8a8a", accent: "#ffffff", accent2: "#cfcfcf", accent3: "#a6a6a6", good: "#e8e8e8", warn: "#bdbdbd", bad: "#ffffff", info: "#9c9c9c", on_accent: "#000000", heat: ["#141414", "#3a3a3a", "#6e6e6e", "#a8a8a8", "#ffffff"] },
    }
}

impl Palette {
    pub fn resolve(id: ThemeId) -> Self {
        Self::with_depth(id, ColorDepth::detect())
    }

    pub fn with_depth(id: ThemeId, depth: ColorDepth) -> Self {
        let s = spec(id);
        let c = |hex: &str| {
            let (r, g, b) = parse_hex_rgb(hex).unwrap_or((255, 0, 255));
            depth.map(r, g, b)
        };
        Self {
            id,
            depth,
            bg: c(s.bg),
            card: c(s.card),
            card_hi: c(s.card_hi),
            border: c(s.border),
            text: c(s.text),
            dim: c(s.dim),
            accent: c(s.accent),
            accent2: c(s.accent2),
            accent3: c(s.accent3),
            good: c(s.good),
            warn: c(s.warn),
            bad: c(s.bad),
            info: c(s.info),
            on_accent: c(s.on_accent),
            heat: [c(s.heat[0]), c(s.heat[1]), c(s.heat[2]), c(s.heat[3]), c(s.heat[4])],
        }
    }

    /// Colour for a user category. In the monochrome theme every category becomes a grey tone.
    pub fn category(&self, hex: &str) -> Color {
        let Some((r, g, b)) = parse_hex_rgb(hex) else {
            return self.accent;
        };
        if self.id == ThemeId::Mono {
            let lum = (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000;
            let v = (110 + lum * 145 / 255).min(255) as u8;
            self.depth.map(v, v, v)
        } else {
            self.depth.map(r, g, b)
        }
    }

    /// True black in every theme (used by zen mode).
    pub fn pure_black(&self) -> Color {
        self.depth.map(0, 0, 0)
    }

    /// Cycle of accent colours for items that have no colour of their own.
    pub fn cycle(&self, i: usize) -> Color {
        [self.accent, self.accent2, self.accent3, self.good, self.info, self.warn][i % 6]
    }
}

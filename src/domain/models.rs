//! Core domain models, preferences, identifiers, and color/date utilities for Chronos.

use crate::error::{ChronosError, ChronosResult};
use chrono::{DateTime, Datelike, Duration as ChronoDuration, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};

pub const VAULT_SCHEMA_VERSION: u32 = 3;
pub const APP_VERSION: &str = "1.0.0";

/// Healthy daily XP ceiling: maximum 5 hours (18,000 seconds) per calendar day credited toward XP.
pub const MAX_DAILY_REWARDED_SECS: u64 = 5 * 3600;
pub const MIN_FOCUS_GOAL_MINS_FOR_REWARD: u32 = 60;
pub const MIN_READING_GOAL_MINS_FOR_REWARD: u32 = 20;
pub const REQUIRED_RESET_CONFIRMATION: &str = "RESET MY DATA";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActivityKind {
    Focus,
    Reading,
    ShortBreak,
    LongBreak,
}

impl ActivityKind {
    pub fn label(self) -> &'static str {
        match self {
            ActivityKind::Focus => "Focus Session",
            ActivityKind::Reading => "Reading Sanctuary",
            ActivityKind::ShortBreak => "Short Breather",
            ActivityKind::LongBreak => "Long Recovery Break",
        }
    }

    pub fn short_label(self) -> &'static str {
        match self {
            ActivityKind::Focus => "Focus",
            ActivityKind::Reading => "Reading",
            ActivityKind::ShortBreak => "Short Break",
            ActivityKind::LongBreak => "Long Break",
        }
    }

    pub fn is_productive(self) -> bool {
        matches!(self, ActivityKind::Focus | ActivityKind::Reading)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompletionState {
    Completed,
    Interrupted,
}

impl CompletionState {
    pub fn label(self) -> &'static str {
        match self {
            CompletionState::Completed => "Completed",
            CompletionState::Interrupted => "Interrupted",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryScope {
    FocusOnly,
    ReadingOnly,
    Universal,
}

impl CategoryScope {
    pub fn label(self) -> &'static str {
        match self {
            CategoryScope::FocusOnly => "Focus",
            CategoryScope::ReadingOnly => "Reading",
            CategoryScope::Universal => "Focus & Reading",
        }
    }

    pub fn supports_activity(self, kind: ActivityKind) -> bool {
        matches!(
            (self, kind),
            (CategoryScope::Universal, _)
                | (CategoryScope::FocusOnly, ActivityKind::Focus)
                | (CategoryScope::ReadingOnly, ActivityKind::Reading)
                | (_, ActivityKind::ShortBreak | ActivityKind::LongBreak)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CadencePreset {
    Harmonic,
    DeepBlock,
    Monolith,
    Custom,
}

impl CadencePreset {
    pub const ALL: [CadencePreset; 4] = [
        CadencePreset::Harmonic,
        CadencePreset::DeepBlock,
        CadencePreset::Monolith,
        CadencePreset::Custom,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CadencePreset::Harmonic => "Harmonic (25 / 5 / 15 ×4)",
            CadencePreset::DeepBlock => "Deep Block (50 / 10 / 25 ×3)",
            CadencePreset::Monolith => "Monolith (90 / 20 / 40 ×2)",
            CadencePreset::Custom => "Custom Cadence",
        }
    }

    pub fn short_name(self) -> &'static str {
        match self {
            CadencePreset::Harmonic => "Harmonic",
            CadencePreset::DeepBlock => "Deep Block",
            CadencePreset::Monolith => "Monolith",
            CadencePreset::Custom => "Custom",
        }
    }

    pub fn schedule_minutes(self, prefs: &FocusPrefs) -> (u32, u32, u32, u32) {
        match self {
            CadencePreset::Harmonic => (25, 5, 15, 4),
            CadencePreset::DeepBlock => (50, 10, 25, 3),
            CadencePreset::Monolith => (90, 20, 40, 2),
            CadencePreset::Custom => (
                prefs.custom_focus_minutes.clamp(5, 240),
                prefs.custom_short_break_minutes.clamp(1, 120),
                prefs.custom_long_break_minutes.clamp(5, 180),
                prefs.custom_intervals.clamp(1, 12),
            ),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialKind {
    Book,
    AcademicPaper,
    TechnicalManual,
    EssayOrArticle,
}

impl MaterialKind {
    pub const ALL: [MaterialKind; 4] = [
        MaterialKind::Book,
        MaterialKind::AcademicPaper,
        MaterialKind::TechnicalManual,
        MaterialKind::EssayOrArticle,
    ];

    pub fn label(self) -> &'static str {
        match self {
            MaterialKind::Book => "Book / Monograph",
            MaterialKind::AcademicPaper => "Academic Paper",
            MaterialKind::TechnicalManual => "Technical Manual / RFC",
            MaterialKind::EssayOrArticle => "Essay / Long-Form Article",
        }
    }

    pub fn next(self) -> Self {
        match self {
            MaterialKind::Book => MaterialKind::AcademicPaper,
            MaterialKind::AcademicPaper => MaterialKind::TechnicalManual,
            MaterialKind::TechnicalManual => MaterialKind::EssayOrArticle,
            MaterialKind::EssayOrArticle => MaterialKind::Book,
        }
    }
}

/// Proportional attribution of a session's elapsed seconds to a specific local calendar date (`YYYY-MM-DD`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DaySlice {
    pub date_key: String,
    pub seconds: u64,
}

/// Optional reading metadata attached to a `Reading` session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingSessionMeta {
    pub material_id: Option<String>,
    pub title: String,
    pub author: String,
    pub source_kind: MaterialKind,
    pub start_page: Option<u32>,
    pub end_page: Option<u32>,
    pub pages_read: u32,
}

/// Authoritative record of a completed or interrupted session in Chronos.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionRecord {
    pub id: String,
    pub activity_kind: ActivityKind,
    pub category_id: String,
    pub cadence: CadencePreset,
    pub completion_state: CompletionState,
    pub started_at_utc: DateTime<Utc>,
    pub ended_at_utc: DateTime<Utc>,
    pub primary_date_key: String,
    pub configured_seconds: u64,
    pub elapsed_seconds: u64,
    pub day_slices: Vec<DaySlice>,
    pub objective: String,
    pub completion_notes: String,
    pub planned_block_id: Option<String>,
    pub reading_meta: Option<ReadingSessionMeta>,
    #[serde(default)]
    pub reassigned_from_category: Option<String>,
}

// ─── User Preferences (TOML-backed) ─────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MotionLevel {
    Full,
    Reduced,
    Off,
}

impl MotionLevel {
    pub fn label(self) -> &'static str {
        match self {
            MotionLevel::Full => "Full Motion",
            MotionLevel::Reduced => "Reduced / Calm",
            MotionLevel::Off => "No Animation",
        }
    }

    pub fn next(self) -> Self {
        match self {
            MotionLevel::Full => MotionLevel::Reduced,
            MotionLevel::Reduced => MotionLevel::Off,
            MotionLevel::Off => MotionLevel::Full,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LayoutDensity {
    Comfortable,
    Compact,
}

impl LayoutDensity {
    pub fn label(self) -> &'static str {
        match self {
            LayoutDensity::Comfortable => "Comfortable (Spacious M3)",
            LayoutDensity::Compact => "Compact (High Density)",
        }
    }

    pub fn next(self) -> Self {
        match self {
            LayoutDensity::Comfortable => LayoutDensity::Compact,
            LayoutDensity::Compact => LayoutDensity::Comfortable,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppearancePrefs {
    pub theme: String,
    pub density: LayoutDensity,
    pub motion: MotionLevel,
    pub show_gamification: bool,
    pub show_navigation_labels: bool,
}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self {
            theme: "cyberpunk_neon".to_string(),
            density: LayoutDensity::Comfortable,
            motion: MotionLevel::Full,
            show_gamification: true,
            show_navigation_labels: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FocusPrefs {
    pub active_cadence: CadencePreset,
    pub custom_focus_minutes: u32,
    pub custom_short_break_minutes: u32,
    pub custom_long_break_minutes: u32,
    pub custom_intervals: u32,
    pub default_category_id: String,
    pub auto_start_breaks: bool,
    pub auto_start_focus: bool,
    pub zen_mode_default: bool,
    pub count_interrupted_in_totals: bool,
}

impl Default for FocusPrefs {
    fn default() -> Self {
        Self {
            active_cadence: CadencePreset::Harmonic,
            custom_focus_minutes: 40,
            custom_short_break_minutes: 8,
            custom_long_break_minutes: 20,
            custom_intervals: 3,
            default_category_id: "cat_software_eng".to_string(),
            auto_start_breaks: false,
            auto_start_focus: false,
            zen_mode_default: false,
            count_interrupted_in_totals: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingPrefs {
    pub default_duration_minutes: u32,
    pub default_category_id: String,
    pub track_pages_read: bool,
    pub prompt_notes_on_complete: bool,
}

impl Default for ReadingPrefs {
    fn default() -> Self {
        Self {
            default_duration_minutes: 35,
            default_category_id: "cat_tech_reading".to_string(),
            track_pages_read: true,
            prompt_notes_on_complete: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GoalPrefs {
    pub daily_focus_minutes: u32,
    pub weekly_focus_minutes: u32,
    pub daily_reading_minutes: u32,
    pub weekly_reading_minutes: u32,
}

impl Default for GoalPrefs {
    fn default() -> Self {
        Self {
            daily_focus_minutes: 180,
            weekly_focus_minutes: 900,
            daily_reading_minutes: 60,
            weekly_reading_minutes: 300,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotificationPrefs {
    pub desktop_notifications: bool,
    pub sound_enabled: bool,
    pub sound_preset: String,
    pub status_bar_export: bool,
}

impl Default for NotificationPrefs {
    fn default() -> Self {
        Self {
            desktop_notifications: true,
            sound_enabled: true,
            sound_preset: "singing_bowl".to_string(),
            status_bar_export: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyPrefs {
    pub confirm_quit_active_session: bool,
    pub confirm_mode_or_route_abort: bool,
    pub auto_backup_before_reset: bool,
}

impl Default for SafetyPrefs {
    fn default() -> Self {
        Self {
            confirm_quit_active_session: true,
            confirm_mode_or_route_abort: true,
            auto_backup_before_reset: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct Preferences {
    #[serde(default)]
    pub appearance: AppearancePrefs,
    #[serde(default)]
    pub focus: FocusPrefs,
    #[serde(default)]
    pub reading: ReadingPrefs,
    #[serde(default)]
    pub goals: GoalPrefs,
    #[serde(default)]
    pub notifications: NotificationPrefs,
    #[serde(default)]
    pub safety: SafetyPrefs,
}

impl Preferences {
    pub fn sanitized(mut self) -> Self {
        self.focus.custom_focus_minutes = self.focus.custom_focus_minutes.clamp(5, 240);
        self.focus.custom_short_break_minutes =
            self.focus.custom_short_break_minutes.clamp(1, 120);
        self.focus.custom_long_break_minutes = self.focus.custom_long_break_minutes.clamp(5, 180);
        self.focus.custom_intervals = self.focus.custom_intervals.clamp(1, 12);
        self.reading.default_duration_minutes =
            self.reading.default_duration_minutes.clamp(5, 240);
        self.goals.daily_focus_minutes = self.goals.daily_focus_minutes.clamp(0, 960);
        self.goals.weekly_focus_minutes = self.goals.weekly_focus_minutes.clamp(0, 5040);
        self.goals.daily_reading_minutes = self.goals.daily_reading_minutes.clamp(0, 720);
        self.goals.weekly_reading_minutes = self.goals.weekly_reading_minutes.clamp(0, 3600);
        self
    }
}

// ─── Deterministic ID, Date & WCAG Color Utilities ──────────────────────────

static ID_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1000);

pub fn generate_entity_id(prefix: &str) -> String {
    let seq = ID_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let millis = Utc::now().timestamp_millis().unsigned_abs();
    format!("{}_{:x}_{}", prefix, millis, seq)
}

/// Converts a UTC timestamp to a local calendar date key `YYYY-MM-DD`.
/// When `fixed_offset_minutes` is provided (e.g. inside unit tests), uses that exact offset.
pub fn to_local_date_key(utc_dt: DateTime<Utc>, fixed_offset_minutes: Option<i32>) -> String {
    if let Some(offset_mins) = fixed_offset_minutes {
        let shifted = utc_dt + ChronoDuration::minutes(offset_mins as i64);
        shifted.format("%Y-%m-%d").to_string()
    } else {
        utc_dt.with_timezone(&Local).format("%Y-%m-%d").to_string()
    }
}

/// Returns the local hour (`0..=23`) for a UTC timestamp.
pub fn to_local_hour(utc_dt: DateTime<Utc>, fixed_offset_minutes: Option<i32>) -> usize {
    use chrono::Timelike;
    if let Some(offset_mins) = fixed_offset_minutes {
        let shifted = utc_dt + ChronoDuration::minutes(offset_mins as i64);
        shifted.hour() as usize
    } else {
        utc_dt.with_timezone(&Local).hour() as usize
    }
}

pub fn parse_date_key(date_key: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(date_key.trim(), "%Y-%m-%d").ok()
}

pub fn format_date_key(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

pub fn shift_date_key(date_key: &str, delta_days: i64) -> String {
    if let Some(d) = parse_date_key(date_key) {
        format_date_key(d + ChronoDuration::days(delta_days))
    } else {
        date_key.to_string()
    }
}

pub fn monday_of_week_key(date_key: &str) -> String {
    if let Some(d) = parse_date_key(date_key) {
        let days_from_mon = d.weekday().num_days_from_monday() as i64;
        format_date_key(d - ChronoDuration::days(days_from_mon))
    } else {
        date_key.to_string()
    }
}

/// Returns the UTC timestamp corresponding to `00:00:00` at the start of `date_key` in the given timezone.
pub fn start_of_local_date_utc(
    date: NaiveDate,
    fixed_offset_minutes: Option<i32>,
) -> DateTime<Utc> {
    let naive_midnight = date.and_hms_opt(0, 0, 0).expect("valid midnight");
    if let Some(offset_mins) = fixed_offset_minutes {
        Utc.from_utc_datetime(&(naive_midnight - ChronoDuration::minutes(offset_mins as i64)))
    } else {
        match Local.from_local_datetime(&naive_midnight).single() {
            Some(local_dt) => local_dt.with_timezone(&Utc),
            None => Utc.from_utc_datetime(&naive_midnight),
        }
    }
}

/// Parses a `#RRGGBB` hex string into `(r, g, b)`.
pub fn parse_hex_rgb(hex: &str) -> Option<(u8, u8, u8)> {
    let clean = hex.trim().strip_prefix('#').unwrap_or(hex.trim());
    if clean.len() != 6 || !clean.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let r = u8::from_str_radix(&clean[0..2], 16).ok()?;
    let g = u8::from_str_radix(&clean[2..4], 16).ok()?;
    let b = u8::from_str_radix(&clean[4..6], 16).ok()?;
    Some((r, g, b))
}

/// Computes WCAG 2.1 relative luminance (`0.0..=1.0`) for an RGB triplet.
pub fn relative_luminance(rgb: (u8, u8, u8)) -> f64 {
    fn channel(c: u8) -> f64 {
        let s = (c as f64) / 255.0;
        if s <= 0.03928 {
            s / 12.92
        } else {
            ((s + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(rgb.0) + 0.7152 * channel(rgb.1) + 0.0722 * channel(rgb.2)
}

/// Computes WCAG contrast ratio (`1.0..=21.0`) between two RGB colors.
pub fn contrast_ratio(a: (u8, u8, u8), b: (u8, u8, u8)) -> f64 {
    let la = relative_luminance(a);
    let lb = relative_luminance(b);
    let brightest = la.max(lb);
    let darkest = la.min(lb);
    (brightest + 0.05) / (darkest + 0.05)
}

/// Validates a category hex color for readability across both dark (`#121619`) and light (`#F7F9FA`) surfaces.
pub fn validate_category_hex(hex: &str) -> ChronosResult<String> {
    let rgb = parse_hex_rgb(hex).ok_or_else(|| {
        ChronosError::Validation(
            "Category color must be a 6-digit hex code such as #26A69A.".to_string(),
        )
    })?;
    let lum = relative_luminance(rgb);
    if lum < 0.04 {
        return Err(ChronosError::Validation(
            "Selected color is too dark to remain legible on dark terminal backgrounds."
                .to_string(),
        ));
    }
    if lum > 0.76 {
        return Err(ChronosError::Validation(
            "Selected color is too bright to remain legible on light terminal backgrounds."
                .to_string(),
        ));
    }
    Ok(format!("#{:02X}{:02X}{:02X}", rgb.0, rgb.1, rgb.2))
}

pub fn format_duration_hms(seconds: u64) -> String {
    let hrs = seconds / 3600;
    let mins = (seconds % 3600) / 60;
    let secs = seconds % 60;
    if hrs > 0 {
        format!("{:02}:{:02}:{:02}", hrs, mins, secs)
    } else {
        format!("{:02}:{:02}", mins, secs)
    }
}

pub fn format_hours_minutes(seconds: u64) -> String {
    let total_mins = seconds / 60;
    let hrs = total_mins / 60;
    let mins = total_mins % 60;
    if hrs > 0 && mins > 0 {
        format!("{}h {:02}m", hrs, mins)
    } else if hrs > 0 {
        format!("{}h", hrs)
    } else if total_mins > 0 {
        format!("{}m", mins)
    } else if seconds > 0 {
        format!("{}s", seconds)
    } else {
        "0m".to_string()
    }
}

//! Chronos Category Management Domain (`taxonomy.rs`).
//!
//! Enforces:
//! - Stable immutable category identifiers (`id`) so renaming or recoloring never breaks historical records.
//! - Case-insensitive duplicate name prevention.
//! - WCAG contrast-validated RGB hex colors.
//! - Safe archival and deletion with explicit historical session reassignment.

use crate::domain::models::{
    generate_entity_id, validate_category_hex, ActivityKind, CategoryScope, Preferences,
    SessionRecord,
};
use crate::domain::planner::PlannedBlock;
use crate::domain::sanctuary::ReadingMaterial;
use crate::error::{ChronosError, ChronosResult};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

pub const APPROVED_CATEGORY_COLORS: &[(&str, &str)] = &[
    ("#26A69A", "Botanical Teal"),
    ("#5C6BC0", "Athenaeum Indigo"),
    ("#EC407A", "Terracotta Rose"),
    ("#FFA726", "Warm Amber"),
    ("#66BB6A", "Mineral Sage"),
    ("#AB47BC", "Iris Violet"),
    ("#29B6F6", "Cerulean Sky"),
    ("#EF5350", "Coral Vermilion"),
    ("#8D6E63", "Umber Bronze"),
    ("#78909C", "Monolith Slate"),
];

pub const CATEGORY_ICON_GLYPHS: &[(&str, &str)] = &[
    ("◈", "Diamond"),
    ("λ", "Lambda / Code"),
    ("◆", "Prism"),
    ("▪", "Monograph"),
    ("▲", "Apex"),
    ("✦", "Nova"),
    ("❖", "Nexus"),
    ("Ω", "Omega"),
];

pub fn sanitize_icon_glyph(raw: &str) -> String {
    match raw.trim() {
        "◈" | "λ" | "◆" | "▪" | "▲" | "✦" | "❖" | "Ω" | "◉" | "≡" => raw.trim().to_string(),
        "⌘" | "⚙" => "◆".to_string(),
        "✎" => "✦".to_string(),
        "📖" => "▪".to_string(),
        "⚗" => "▲".to_string(),
        _ => "◈".to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CategoryRecord {
    pub id: String,
    pub name: String,
    pub color_hex: String,
    pub icon_glyph: String,
    pub scope: CategoryScope,
    pub description: String,
    pub daily_goal_minutes: u32,
    pub weekly_goal_minutes: u32,
    pub is_default: bool,
    pub is_archived: bool,
    pub created_at_utc: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct CategoryDraft {
    pub name: String,
    pub color_hex: String,
    pub icon_glyph: String,
    pub scope: CategoryScope,
    pub description: String,
    pub daily_goal_minutes: u32,
    pub weekly_goal_minutes: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CategoryDeleteStrategy {
    ArchiveInstead,
    ReassignTo(String),
}

pub fn default_categories(now_utc: DateTime<Utc>) -> Vec<CategoryRecord> {
    vec![
        CategoryRecord {
            id: "cat_software_eng".to_string(),
            name: "Software Engineering".to_string(),
            color_hex: "#26A69A".to_string(),
            icon_glyph: "λ".to_string(),
            scope: CategoryScope::FocusOnly,
            description: "Systems design, implementation, debugging, and verification".to_string(),
            daily_goal_minutes: 90,
            weekly_goal_minutes: 450,
            is_default: true,
            is_archived: false,
            created_at_utc: now_utc,
        },
        CategoryRecord {
            id: "cat_deep_research".to_string(),
            name: "Research & Inquiry".to_string(),
            color_hex: "#5C6BC0".to_string(),
            icon_glyph: "▲".to_string(),
            scope: CategoryScope::Universal,
            description: "Academic research, domain investigation, and quantitative analysis"
                .to_string(),
            daily_goal_minutes: 60,
            weekly_goal_minutes: 300,
            is_default: true,
            is_archived: false,
            created_at_utc: now_utc,
        },
        CategoryRecord {
            id: "cat_writing".to_string(),
            name: "Writing & Specs".to_string(),
            color_hex: "#EC407A".to_string(),
            icon_glyph: "✦".to_string(),
            scope: CategoryScope::FocusOnly,
            description: "Technical documentation, manuscripts, essays, and structured notes"
                .to_string(),
            daily_goal_minutes: 45,
            weekly_goal_minutes: 225,
            is_default: true,
            is_archived: false,
            created_at_utc: now_utc,
        },
        CategoryRecord {
            id: "cat_tech_reading".to_string(),
            name: "Technical Literature".to_string(),
            color_hex: "#FFA726".to_string(),
            icon_glyph: "▪".to_string(),
            scope: CategoryScope::ReadingOnly,
            description: "Engineering textbooks, academic papers, RFCs, and specifications"
                .to_string(),
            daily_goal_minutes: 40,
            weekly_goal_minutes: 200,
            is_default: true,
            is_archived: false,
            created_at_utc: now_utc,
        },
        CategoryRecord {
            id: "cat_humanities".to_string(),
            name: "Philosophy & History".to_string(),
            color_hex: "#66BB6A".to_string(),
            icon_glyph: "◈".to_string(),
            scope: CategoryScope::ReadingOnly,
            description: "Long-form non-fiction, history, biographies, and monographs".to_string(),
            daily_goal_minutes: 30,
            weekly_goal_minutes: 150,
            is_default: true,
            is_archived: false,
            created_at_utc: now_utc,
        },
    ]
}

pub fn validate_category_draft(
    existing: &[CategoryRecord],
    draft: &CategoryDraft,
    editing_id: Option<&str>,
) -> ChronosResult<(String, String)> {
    let trimmed_name = draft.name.trim();
    if trimmed_name.chars().count() < 2 || trimmed_name.chars().count() > 40 {
        return Err(ChronosError::Validation(
            "Category name must be between 2 and 40 characters.".to_string(),
        ));
    }

    let duplicate = existing.iter().any(|c| {
        Some(c.id.as_str()) != editing_id && c.name.trim().eq_ignore_ascii_case(trimmed_name)
    });
    if duplicate {
        return Err(ChronosError::Validation(format!(
            "A category named '{}' already exists.",
            trimmed_name
        )));
    }

    let normalized_hex = validate_category_hex(&draft.color_hex)?;
    Ok((trimmed_name.to_string(), normalized_hex))
}

pub fn create_category(
    categories: &mut Vec<CategoryRecord>,
    draft: CategoryDraft,
    now_utc: DateTime<Utc>,
) -> ChronosResult<CategoryRecord> {
    let (clean_name, normalized_hex) = validate_category_draft(categories, &draft, None)?;
    let record = CategoryRecord {
        id: generate_entity_id("cat"),
        name: clean_name,
        color_hex: normalized_hex,
        icon_glyph: if draft.icon_glyph.trim().is_empty() {
            "◈".to_string()
        } else {
            draft.icon_glyph.trim().to_string()
        },
        scope: draft.scope,
        description: draft.description.trim().to_string(),
        daily_goal_minutes: draft.daily_goal_minutes.clamp(0, 720),
        weekly_goal_minutes: draft.weekly_goal_minutes.clamp(0, 5040),
        is_default: false,
        is_archived: false,
        created_at_utc: now_utc,
    };
    categories.push(record.clone());
    Ok(record)
}

pub fn update_category(
    categories: &mut [CategoryRecord],
    category_id: &str,
    draft: CategoryDraft,
) -> ChronosResult<CategoryRecord> {
    let (clean_name, normalized_hex) =
        validate_category_draft(categories, &draft, Some(category_id))?;

    let idx = categories
        .iter()
        .position(|c| c.id == category_id)
        .ok_or_else(|| ChronosError::Validation(format!("Category not found: {}", category_id)))?;

    let mut preview = categories.to_vec();
    preview[idx].name = clean_name;
    preview[idx].color_hex = normalized_hex;
    preview[idx].icon_glyph = if draft.icon_glyph.trim().is_empty() {
        preview[idx].icon_glyph.clone()
    } else {
        draft.icon_glyph.trim().to_string()
    };
    preview[idx].scope = draft.scope;
    preview[idx].description = draft.description.trim().to_string();
    preview[idx].daily_goal_minutes = draft.daily_goal_minutes.clamp(0, 720);
    preview[idx].weekly_goal_minutes = draft.weekly_goal_minutes.clamp(0, 5040);

    ensure_active_category_coverage(&preview)?;
    categories[idx] = preview[idx].clone();
    Ok(categories[idx].clone())
}

pub fn set_category_archived(
    categories: &mut [CategoryRecord],
    category_id: &str,
    archived: bool,
    active_timer_category_id: Option<&str>,
) -> ChronosResult<()> {
    if archived && active_timer_category_id == Some(category_id) {
        return Err(ChronosError::ActiveSessionGuard(
            "Cannot archive a category while it is used by an active timer session.".to_string(),
        ));
    }

    let idx = categories
        .iter()
        .position(|c| c.id == category_id)
        .ok_or_else(|| ChronosError::Validation(format!("Category not found: {}", category_id)))?;

    let mut preview = categories.to_vec();
    preview[idx].is_archived = archived;
    ensure_active_category_coverage(&preview)?;
    categories[idx].is_archived = archived;
    Ok(())
}

/// Safely deletes or archives a category while guaranteeing historical data integrity.
pub fn delete_category_safely(
    categories: &mut Vec<CategoryRecord>,
    sessions: &mut [SessionRecord],
    reading_materials: &mut [ReadingMaterial],
    planned_blocks: &mut [PlannedBlock],
    prefs: &mut Preferences,
    category_id: &str,
    active_timer_category_id: Option<&str>,
    strategy: CategoryDeleteStrategy,
) -> ChronosResult<usize> {
    if active_timer_category_id == Some(category_id) {
        return Err(ChronosError::ActiveSessionGuard(
            "Cannot delete a category while an active timer session is using it.".to_string(),
        ));
    }

    let target = categories
        .iter()
        .find(|c| c.id == category_id)
        .cloned()
        .ok_or_else(|| ChronosError::Validation(format!("Category not found: {}", category_id)))?;

    match strategy {
        CategoryDeleteStrategy::ArchiveInstead => {
            set_category_archived(categories, category_id, true, active_timer_category_id)?;
            Ok(0)
        }
        CategoryDeleteStrategy::ReassignTo(ref replacement_id) => {
            if replacement_id == category_id {
                return Err(ChronosError::Validation(
                    "Replacement category must be different from the category being deleted."
                        .to_string(),
                ));
            }
            let remaining: Vec<CategoryRecord> = categories
                .iter()
                .filter(|c| c.id != category_id)
                .cloned()
                .collect();
            ensure_active_category_coverage(&remaining)?;

            let linked_sessions = sessions
                .iter()
                .filter(|s| s.category_id == category_id)
                .count();
            let linked_materials = reading_materials
                .iter()
                .filter(|m| m.category_id == category_id)
                .count();
            let linked_plans = planned_blocks
                .iter()
                .filter(|p| p.category_id == category_id)
                .count();

            let replacement = if linked_sessions > 0 || linked_materials > 0 || linked_plans > 0 {
                remaining
                    .iter()
                    .find(|c| &c.id == replacement_id)
                    .cloned()
                    .ok_or_else(|| {
                        ChronosError::Validation(format!(
                            "Category '{}' is referenced by {} historical session(s). Please choose a valid replacement category or archive it instead.",
                            target.name, linked_sessions
                        ))
                    })?
            } else {
                remaining
                    .iter()
                    .find(|c| &c.id == replacement_id)
                    .or_else(|| remaining.iter().find(|c| !c.is_archived))
                    .cloned()
                    .ok_or_else(|| {
                        ChronosError::Validation("No remaining categories available.".to_string())
                    })?
            };

            let mut reassigned_count = 0usize;
            for session in sessions.iter_mut() {
                if session.category_id == category_id {
                    session.category_id = replacement.id.clone();
                    if session.reassigned_from_category.is_none() {
                        session.reassigned_from_category = Some(target.name.clone());
                    }
                    reassigned_count += 1;
                }
            }

            for mat in reading_materials.iter_mut() {
                if mat.category_id == category_id {
                    mat.category_id = replacement.id.clone();
                }
            }

            for plan in planned_blocks.iter_mut() {
                if plan.category_id == category_id {
                    plan.category_id = replacement.id.clone();
                }
            }

            if prefs.focus.default_category_id == category_id {
                if let Some(focus_cat) = remaining
                    .iter()
                    .find(|c| !c.is_archived && c.scope.supports_activity(ActivityKind::Focus))
                {
                    prefs.focus.default_category_id = focus_cat.id.clone();
                }
            }
            if prefs.reading.default_category_id == category_id {
                if let Some(read_cat) = remaining
                    .iter()
                    .find(|c| !c.is_archived && c.scope.supports_activity(ActivityKind::Reading))
                {
                    prefs.reading.default_category_id = read_cat.id.clone();
                }
            }

            *categories = remaining;
            Ok(reassigned_count)
        }
    }
}

pub fn ensure_active_category_coverage(categories: &[CategoryRecord]) -> ChronosResult<()> {
    let has_focus = categories
        .iter()
        .any(|c| !c.is_archived && c.scope.supports_activity(ActivityKind::Focus));
    let has_reading = categories
        .iter()
        .any(|c| !c.is_archived && c.scope.supports_activity(ActivityKind::Reading));

    if !has_focus {
        return Err(ChronosError::Validation(
            "At least one active Focus category must remain available.".to_string(),
        ));
    }
    if !has_reading {
        return Err(ChronosError::Validation(
            "At least one active Reading category must remain available.".to_string(),
        ));
    }
    Ok(())
}

pub fn active_categories_for(
    categories: &[CategoryRecord],
    activity: ActivityKind,
) -> Vec<&CategoryRecord> {
    categories
        .iter()
        .filter(|c| !c.is_archived && c.scope.supports_activity(activity))
        .collect()
}

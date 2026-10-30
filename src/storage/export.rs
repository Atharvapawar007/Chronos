//! Data Export Engine (`export.rs`) for RFC-4180 CSV and Structured JSON.

use crate::error::ChronosResult;
use crate::storage::repository::{write_atomically, ChronosPaths, ChronosVault};
use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

fn escape_csv_field(field: &str) -> String {
    if field.contains(',') || field.contains('"') || field.contains('\n') || field.contains('\r') {
        format!("\"{}\"", field.replace('"', "\"\""))
    } else {
        field.to_string()
    }
}

/// Formats all session records in `vault` as an RFC-4180 CSV string.
pub fn format_sessions_csv(vault: &ChronosVault) -> String {
    let cat_names: HashMap<&str, &str> = vault
        .categories
        .iter()
        .map(|c| (c.id.as_str(), c.name.as_str()))
        .collect();

    let mut out = String::from(
        "session_id,date,started_at_utc,ended_at_utc,activity_kind,category_id,category_name,completion_state,configured_seconds,elapsed_seconds,elapsed_minutes,objective,completion_notes,reading_title,reading_author,pages_read\n",
    );

    for s in &vault.sessions {
        let cat_name = cat_names
            .get(s.category_id.as_str())
            .copied()
            .unwrap_or("Unassigned");
        let (r_title, r_author, r_pages) = match s.reading_meta {
            Some(ref m) => (
                m.title.as_str(),
                m.author.as_str(),
                m.pages_read.to_string(),
            ),
            None => ("", "", String::new()),
        };
        let mins = format!("{:.2}", (s.elapsed_seconds as f64) / 60.0);

        let cols = [
            escape_csv_field(&s.id),
            escape_csv_field(&s.primary_date_key),
            escape_csv_field(&s.started_at_utc.to_rfc3339()),
            escape_csv_field(&s.ended_at_utc.to_rfc3339()),
            escape_csv_field(s.activity_kind.short_label()),
            escape_csv_field(&s.category_id),
            escape_csv_field(cat_name),
            escape_csv_field(s.completion_state.label()),
            s.configured_seconds.to_string(),
            s.elapsed_seconds.to_string(),
            mins,
            escape_csv_field(&s.objective),
            escape_csv_field(&s.completion_notes),
            escape_csv_field(r_title),
            escape_csv_field(r_author),
            r_pages,
        ];
        out.push_str(&cols.join(","));
        out.push('\n');
    }

    out
}

/// Writes a CSV export file to `custom_dest` or `paths.exports_dir`.
pub fn export_sessions_csv_file(
    paths: &ChronosPaths,
    vault: &ChronosVault,
    custom_dest: Option<&Path>,
    now_utc: DateTime<Utc>,
) -> ChronosResult<PathBuf> {
    let dest = if let Some(p) = custom_dest {
        p.to_path_buf()
    } else {
        fs::create_dir_all(&paths.exports_dir)?;
        let stamp = now_utc.format("%Y%m%d-%H%M%S");
        paths
            .exports_dir
            .join(format!("chronos-sessions-{}.csv", stamp))
    };

    let csv_text = format_sessions_csv(vault);
    write_atomically(&dest, csv_text.as_bytes())?;
    Ok(dest)
}

/// Writes a structured JSON export file to `custom_dest` or `paths.exports_dir`.
pub fn export_structured_json_file(
    paths: &ChronosPaths,
    vault: &ChronosVault,
    custom_dest: Option<&Path>,
    now_utc: DateTime<Utc>,
) -> ChronosResult<PathBuf> {
    let dest = if let Some(p) = custom_dest {
        p.to_path_buf()
    } else {
        fs::create_dir_all(&paths.exports_dir)?;
        let stamp = now_utc.format("%Y%m%d-%H%M%S");
        paths
            .exports_dir
            .join(format!("chronos-structured-{}.json", stamp))
    };

    let bytes = serde_json::to_vec_pretty(vault)?;
    write_atomically(&dest, &bytes)?;
    Ok(dest)
}

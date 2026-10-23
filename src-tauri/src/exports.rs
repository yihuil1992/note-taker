use crate::storage::{get_meeting_detail, initialize_database, MeetingDetailRecord};
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ExportError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Meeting not found: {0}")]
    MissingMeeting(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExportResult {
    pub meeting_id: String,
    pub format: String,
    pub path: String,
    pub bytes: u64,
}

pub fn export_meeting_markdown(
    database_path: &Path,
    exports_dir: &Path,
    meeting_id: &str,
) -> Result<ExportResult, ExportError> {
    let detail = load_detail(database_path, meeting_id)?;
    fs::create_dir_all(exports_dir)?;
    let path = exports_dir.join(format!(
        "{}-{}.md",
        safe_file_stem(&detail.meeting.title),
        meeting_id
    ));
    let markdown = render_markdown(&detail)?;
    fs::write(&path, markdown.as_bytes())?;
    Ok(ExportResult {
        meeting_id: meeting_id.to_string(),
        format: "markdown".to_string(),
        bytes: fs::metadata(&path)?.len(),
        path: path.display().to_string(),
    })
}

pub fn export_meeting_json(
    database_path: &Path,
    exports_dir: &Path,
    meeting_id: &str,
) -> Result<ExportResult, ExportError> {
    let detail = load_detail(database_path, meeting_id)?;
    fs::create_dir_all(exports_dir)?;
    let path = exports_dir.join(format!(
        "{}-{}.json",
        safe_file_stem(&detail.meeting.title),
        meeting_id
    ));
    let json = serde_json::to_string_pretty(&detail)?;
    fs::write(&path, json.as_bytes())?;
    Ok(ExportResult {
        meeting_id: meeting_id.to_string(),
        format: "json".to_string(),
        bytes: fs::metadata(&path)?.len(),
        path: path.display().to_string(),
    })
}

fn load_detail(database_path: &Path, meeting_id: &str) -> Result<MeetingDetailRecord, ExportError> {
    initialize_database(database_path)?;
    get_meeting_detail(database_path, meeting_id)?
        .ok_or_else(|| ExportError::MissingMeeting(meeting_id.to_string()))
}

fn render_markdown(detail: &MeetingDetailRecord) -> Result<String, ExportError> {
    let mut output = String::new();
    output.push_str(&format!("# {}\n\n", detail.meeting.title));
    output.push_str(&format!("- Started: {}\n", detail.meeting.started_at));
    if let Some(ended_at) = &detail.meeting.ended_at {
        output.push_str(&format!("- Ended: {}\n", ended_at));
    }
    output.push_str(&format!("- Status: {}\n\n", detail.meeting.status));

    if let Some(summary) = &detail.summary {
        output.push_str("## Summary\n\n");
        output.push_str(&summary.overview);
        output.push_str("\n\n");

        render_string_array_section(&mut output, "Topics", &summary.topics_json)?;
        render_object_array_section(&mut output, "Decisions", &summary.decisions_json, "text")?;
        render_action_items(&mut output, &summary.action_items_json)?;
        render_object_array_section(
            &mut output,
            "Open Questions",
            &summary.risks_or_questions_json,
            "text",
        )?;
    }

    output.push_str("## Transcript\n\n");
    if detail.transcript_segments.is_empty() {
        output.push_str("_No transcript segments yet._\n");
    } else {
        for segment in &detail.transcript_segments {
            output.push_str(&format!(
                "- [{}-{} ms] **{}**: {}\n",
                segment.start_ms, segment.end_ms, segment.speaker_label, segment.text
            ));
        }
    }

    Ok(output)
}

fn render_string_array_section(
    output: &mut String,
    title: &str,
    raw_json: &str,
) -> Result<(), ExportError> {
    let items: Vec<String> = serde_json::from_str(raw_json)?;
    if items.is_empty() {
        return Ok(());
    }
    output.push_str(&format!("## {title}\n\n"));
    for item in items {
        output.push_str(&format!("- {item}\n"));
    }
    output.push('\n');
    Ok(())
}

fn render_object_array_section(
    output: &mut String,
    title: &str,
    raw_json: &str,
    field: &str,
) -> Result<(), ExportError> {
    let items: Vec<Value> = serde_json::from_str(raw_json)?;
    if items.is_empty() {
        return Ok(());
    }
    output.push_str(&format!("## {title}\n\n"));
    for item in items {
        if let Some(text) = item.get(field).and_then(Value::as_str) {
            output.push_str(&format!("- {text}\n"));
        }
    }
    output.push('\n');
    Ok(())
}

fn render_action_items(output: &mut String, raw_json: &str) -> Result<(), ExportError> {
    let items: Vec<Value> = serde_json::from_str(raw_json)?;
    if items.is_empty() {
        return Ok(());
    }
    output.push_str("## Action Items\n\n");
    for item in items {
        let task = item
            .get("task")
            .and_then(Value::as_str)
            .unwrap_or("Untitled task");
        let owner = item.get("owner").and_then(Value::as_str);
        let due_date = item.get("dueDate").and_then(Value::as_str);
        let suffix = match (owner, due_date) {
            (Some(owner), Some(due_date)) => format!(" ({owner}, due {due_date})"),
            (Some(owner), None) => format!(" ({owner})"),
            (None, Some(due_date)) => format!(" (due {due_date})"),
            (None, None) => String::new(),
        };
        output.push_str(&format!("- [ ] {task}{suffix}\n"));
    }
    output.push('\n');
    Ok(())
}

fn safe_file_stem(title: &str) -> String {
    let stem = title
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else if ch.is_whitespace() {
                '-'
            } else {
                '_'
            }
        })
        .collect::<String>();
    let stem = stem.trim_matches(['-', '_']).trim();
    if stem.is_empty() {
        "meeting".to_string()
    } else {
        stem.chars().take(80).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::{archive_meeting, restore_meeting};

    #[test]
    fn archived_and_restored_meetings_export_without_losing_content() {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join("fixture.sqlite3");
        let exports = root.path().join("exports");
        initialize_database(&database).unwrap();
        let connection = rusqlite::Connection::open(&database).unwrap();
        connection
            .execute_batch(
                r#"
            INSERT INTO meetings (id, title, started_at, status)
            VALUES ('fixture', '../Review: meeting', '2026-10-23T08:00:00Z', 'summarized');
            INSERT INTO transcript_segments (id, meeting_id, source_kind, speaker_label,
                language, start_ms, end_ms, text, provider)
            VALUES ('segment', 'fixture', 'microphone', 'Speaker', 'en', 0, 1000,
                'Isolated transcript', 'fixture');
            INSERT INTO meeting_summaries (meeting_id, suggested_title, provider, model,
                language, overview, decisions_json, action_items_json, topics_json,
                risks_or_questions_json, raw_json)
            VALUES ('fixture', 'Review', 'fixture', 'fixture', 'en', 'Summary preserved',
                '[]', '[{"task":"Follow up"}]', '["Security"]', '[]', '{}');
        "#,
            )
            .unwrap();
        drop(connection);
        archive_meeting(&database, "fixture").unwrap();
        let archived = export_meeting_json(&database, &exports, "fixture").unwrap();
        let json: Value = serde_json::from_slice(&fs::read(&archived.path).unwrap()).unwrap();
        assert!(json["meeting"]["archivedAt"].is_string());
        assert_eq!(json["transcriptSegments"][0]["text"], "Isolated transcript");
        assert_eq!(Path::new(&archived.path).parent(), Some(exports.as_path()));
        restore_meeting(&database, "fixture").unwrap();
        let markdown = export_meeting_markdown(&database, &exports, "fixture").unwrap();
        let text = fs::read_to_string(&markdown.path).unwrap();
        assert!(text.contains("Summary preserved") && text.contains("Follow up"));
        assert!(text.contains("Isolated transcript"));
        assert_eq!(markdown.bytes, text.len() as u64);
        assert_eq!(Path::new(&markdown.path).parent(), Some(exports.as_path()));
        let restored = export_meeting_json(&database, &exports, "fixture").unwrap();
        let json: Value = serde_json::from_slice(&fs::read(&restored.path).unwrap()).unwrap();
        assert!(json["meeting"]["archivedAt"].is_null());
    }

    #[test]
    fn missing_meeting_export_does_not_create_output() {
        let root = tempfile::tempdir().unwrap();
        let database = root.path().join("fixture.sqlite3");
        let exports = root.path().join("exports");
        assert!(matches!(
            export_meeting_json(&database, &exports, "missing"),
            Err(ExportError::MissingMeeting(_))
        ));
        assert!(matches!(
            export_meeting_markdown(&database, &exports, "missing"),
            Err(ExportError::MissingMeeting(_))
        ));
        assert!(!exports.exists());
    }
}

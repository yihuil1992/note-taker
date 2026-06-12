use crate::storage::{
    initialize_database, list_transcript_segments_for_meeting, update_meeting_status,
    update_meeting_title, upsert_meeting_summary, NewMeetingSummary, TranscriptSegmentRecord,
};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use thiserror::Error;

const DEFAULT_CODEX_MODEL: &str = "gpt-5.4";
const SUMMARY_PROVIDER: &str = "codex-cli";

#[derive(Debug, Error)]
pub enum SummaryError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("No transcript segments found for meeting {0}")]
    EmptyTranscript(String),
    #[error("Codex CLI failed with exit code {code:?}: {stderr}")]
    CodexFailed { code: Option<i32>, stderr: String },
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummaryResult {
    pub meeting_id: String,
    pub suggested_title: String,
    pub provider: String,
    pub model: String,
    pub language: String,
    pub overview: String,
    pub topics: Vec<String>,
    pub decisions: Vec<Decision>,
    pub action_items: Vec<ActionItem>,
    pub open_questions: Vec<OpenQuestion>,
    pub summary_outline: Vec<SummaryOutlineSection>,
    pub structured_notes: Vec<StructuredNote>,
    pub detailed_notes: Vec<DetailedNote>,
    pub raw_json: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexSummary {
    pub suggested_title: String,
    pub language: String,
    pub overview: String,
    pub topics: Vec<String>,
    pub decisions: Vec<Decision>,
    pub action_items: Vec<ActionItem>,
    pub open_questions: Vec<OpenQuestion>,
    #[serde(default)]
    pub summary_outline: Vec<SummaryOutlineSection>,
    #[serde(default)]
    pub structured_notes: Vec<StructuredNote>,
    #[serde(default)]
    pub detailed_notes: Vec<DetailedNote>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Decision {
    pub text: String,
    pub evidence: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionItem {
    pub task: String,
    pub owner: Option<String>,
    pub due_date: Option<String>,
    pub evidence: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenQuestion {
    pub text: String,
    pub evidence: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetailedNote {
    pub title: String,
    pub detail: String,
    pub evidence: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryOutlineSection {
    pub title: String,
    pub summary: String,
    pub items: Vec<SummaryOutlineItem>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SummaryOutlineItem {
    pub title: String,
    pub summary: String,
    pub detail: String,
    pub evidence: Option<String>,
    pub decisions: Vec<String>,
    pub action_items: Vec<String>,
    pub open_questions: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StructuredNote {
    pub title: String,
    pub category: String,
    pub summary: String,
    pub detail: String,
    pub evidence: Option<String>,
    pub decisions: Vec<String>,
    pub action_items: Vec<String>,
    pub open_questions: Vec<String>,
}

pub fn summarize_meeting_with_codex(
    database_path: &Path,
    work_dir: &Path,
    meeting_id: &str,
) -> Result<MeetingSummaryResult, SummaryError> {
    summarize_meeting_with_codex_model(database_path, work_dir, meeting_id, DEFAULT_CODEX_MODEL)
}

pub fn summarize_meeting_with_codex_model(
    database_path: &Path,
    work_dir: &Path,
    meeting_id: &str,
    model: &str,
) -> Result<MeetingSummaryResult, SummaryError> {
    initialize_database(database_path)?;
    fs::create_dir_all(work_dir)?;
    let segments = list_transcript_segments_for_meeting(database_path, meeting_id)?;
    if segments.is_empty() {
        return Err(SummaryError::EmptyTranscript(meeting_id.to_string()));
    }

    update_meeting_status(database_path, meeting_id, "summarizing")?;
    let transcript = render_transcript(&segments);
    let schema_path = work_dir.join("summary.schema.json");
    let output_path = work_dir.join(format!("summary-{meeting_id}.json"));
    fs::write(&schema_path, summary_schema_json())?;

    let prompt = build_summary_prompt(meeting_id, &transcript);
    let mut command = Command::new(codex_command_name());
    command
        .arg("exec")
        .arg("-m")
        .arg(model)
        .arg("--skip-git-repo-check")
        .arg("--ephemeral")
        .arg("--sandbox")
        .arg("read-only")
        .arg("--output-schema")
        .arg(&schema_path)
        .arg("--output-last-message")
        .arg(&output_path)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::process::suppress_console_window(&mut command);
    let mut child = command.spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())?;
    }
    let output = child.wait_with_output()?;

    if !output.status.success() {
        update_meeting_status(database_path, meeting_id, "summary_failed")?;
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();
        return Err(SummaryError::CodexFailed {
            code: output.status.code(),
            stderr: summarize_codex_stderr(&stderr),
        });
    }

    let raw_json = fs::read_to_string(&output_path)?;
    let summary: CodexSummary = serde_json::from_str(raw_json.trim())?;
    persist_summary(database_path, meeting_id, model, summary, raw_json.trim())
}

fn persist_summary(
    database_path: &Path,
    meeting_id: &str,
    model: &str,
    summary: CodexSummary,
    raw_json: &str,
) -> Result<MeetingSummaryResult, SummaryError> {
    let decisions_json = serde_json::to_string(&summary.decisions)?;
    let action_items_json = serde_json::to_string(&summary.action_items)?;
    let topics_json = serde_json::to_string(&summary.topics)?;
    let open_questions_json = serde_json::to_string(&summary.open_questions)?;
    upsert_meeting_summary(
        database_path,
        &NewMeetingSummary {
            meeting_id,
            suggested_title: &summary.suggested_title,
            provider: SUMMARY_PROVIDER,
            model,
            language: &summary.language,
            overview: &summary.overview,
            decisions_json: &decisions_json,
            action_items_json: &action_items_json,
            topics_json: &topics_json,
            risks_or_questions_json: &open_questions_json,
            raw_json,
        },
    )?;
    update_meeting_title(
        database_path,
        meeting_id,
        &summary.suggested_title,
        "ai_generated",
    )?;
    update_meeting_status(database_path, meeting_id, "summarized")?;

    Ok(MeetingSummaryResult {
        meeting_id: meeting_id.to_string(),
        suggested_title: summary.suggested_title,
        provider: SUMMARY_PROVIDER.to_string(),
        model: model.to_string(),
        language: summary.language,
        overview: summary.overview,
        topics: summary.topics,
        decisions: summary.decisions,
        action_items: summary.action_items,
        open_questions: summary.open_questions,
        summary_outline: summary.summary_outline,
        structured_notes: summary.structured_notes,
        detailed_notes: summary.detailed_notes,
        raw_json: raw_json.to_string(),
    })
}

fn render_transcript(segments: &[TranscriptSegmentRecord]) -> String {
    segments
        .iter()
        .map(|segment| {
            format!(
                "[{}-{}] {} / {}: {}",
                format_timecode(segment.start_ms),
                format_timecode(segment.end_ms),
                segment.source_kind,
                segment.speaker_label,
                segment.text
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn build_summary_prompt(meeting_id: &str, transcript: &str) -> String {
    format!(
        r#"You are summarizing a locally captured meeting transcript.

Return valid JSON matching the provided schema.

Goal:
- Produce one integrated, structured meeting record, not a short executive summary plus a separate detail dump.
- The user must be able to review the meeting without rereading the whole transcript.
- Do not worry that the output is long. Long meetings should produce long, structured notes.

Rules:
- The summary language should be Simplified Chinese when the meeting is mixed-language or unclear.
- The overview should be concise. summaryOutline is the comprehensive user-facing meeting record.
- Organize summaryOutline as a hierarchy: first-level sections are major meeting themes, and each section contains concrete child points that can be expanded for detail.
- Example structure: "房间占用" -> "房间用户识别目标", "以 PI 作为房间官方归属"; "research report 优化" -> "people 新增两类", "分成 3 个大区".
- Cover every distinct substantive point mentioned in the transcript. It is okay to merge repetitions, corrections, filler, and acknowledgements, but do not omit unique requirements, examples, edge cases, objections, or follow-up ideas.
- Pay special attention to concrete product/work artifacts: reports, research group views, tables, map/list views, copied table output, fields, filters, UI interactions, data-entry changes, modeling rules, and terminology decisions.
- If participants discuss a concrete change to a report/view/table/UI, include it as a summaryOutline item even if it was not a final decision.
- For a short meeting, write 5-10 summaryOutline child items. For a meeting around 30 minutes, prefer 18-35 child items when the transcript supports them. For longer meetings, add more items as needed.
- Each summaryOutline child item should capture one concrete discussion thread, tradeoff, requirement, decision context, unresolved point, or implementation detail.
- Group related child items under meaningful section titles. Prefer topic-flow order over a flat chronology when it improves reviewability.
- Put item-specific decisions, action items, and open questions inside the matching summaryOutline item. Also repeat the important storage/search rollups in the top-level decisions/actionItems/openQuestions arrays.
- Do not create a separate "Detailed notes" section in the content. The details belong inside summaryOutline child items.
- Use specific nouns from the transcript instead of generic labels. For example, if "research group report", "multi-floor map refresh", or "copyable table view" is discussed, name that artifact directly.
- Use action item owner and dueDate only when directly inferable from the transcript.
- Use null for unknown owner, dueDate, or evidence.
- Do not invent decisions, action items, outline items, or open questions.
- Use evidence for compact transcript references such as timestamps, short source labels, or short supporting phrases. Prefer timestamp ranges from the transcript.
- Keep suggestedTitle concise and suitable as a meeting title.

Coverage check before returning JSON:
- Scan the transcript from start to finish.
- Verify that every substantive topic or requested change appears in summaryOutline, with rollup copies in topics, decisions, actionItems, or openQuestions when appropriate.
- If a point does not fit decisions/action/openQuestions, put it in summaryOutline.

Meeting id: {meeting_id}

Transcript:
{transcript}
"#
    )
}

fn format_timecode(ms: i64) -> String {
    let total_seconds = ms.max(0) / 1000;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02}")
}

fn summarize_codex_stderr(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return "Codex CLI failed without stderr output.".to_string();
    }

    let mut messages = Vec::new();
    for chunk in trimmed.split("ERROR:").skip(1) {
        if let Some(json_text) = extract_first_json_object(chunk) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_text) {
                let message = value
                    .get("error")
                    .and_then(|error| error.get("message"))
                    .or_else(|| value.get("message"))
                    .and_then(|message| message.as_str())
                    .map(str::trim)
                    .filter(|message| !message.is_empty());
                if let Some(message) = message {
                    if !messages.iter().any(|existing| existing == message) {
                        messages.push(message.to_string());
                    }
                }
            }
        }
    }

    if !messages.is_empty() {
        return messages.join("; ");
    }

    let compact = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    truncate_for_ui(&compact, 800)
}

fn extract_first_json_object(value: &str) -> Option<String> {
    let start = value.find('{')?;
    let mut depth = 0usize;
    let mut in_string = false;
    let mut escaped = false;
    for (offset, character) in value[start..].char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match character {
            '\\' if in_string => escaped = true,
            '"' => in_string = !in_string,
            '{' if !in_string => depth += 1,
            '}' if !in_string => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = start + offset + character.len_utf8();
                    return Some(value[start..end].to_string());
                }
            }
            _ => {}
        }
    }
    None
}

fn truncate_for_ui(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut truncated = value.chars().take(max_chars).collect::<String>();
    truncated.push_str("...");
    truncated
}

fn summary_schema_json() -> &'static str {
    r#"{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "suggestedTitle": { "type": "string" },
    "language": { "type": "string" },
    "overview": { "type": "string" },
    "topics": {
      "type": "array",
      "items": { "type": "string" }
    },
    "decisions": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "text": { "type": "string" },
          "evidence": { "type": ["string", "null"] }
        },
        "required": ["text", "evidence"]
      }
    },
    "actionItems": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "task": { "type": "string" },
          "owner": { "type": ["string", "null"] },
          "dueDate": { "type": ["string", "null"] },
          "evidence": { "type": ["string", "null"] }
        },
        "required": ["task", "owner", "dueDate", "evidence"]
      }
    },
    "openQuestions": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "text": { "type": "string" },
          "evidence": { "type": ["string", "null"] }
        },
        "required": ["text", "evidence"]
      }
    },
    "summaryOutline": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "title": { "type": "string" },
          "summary": { "type": "string" },
          "items": {
            "type": "array",
            "items": {
              "type": "object",
              "additionalProperties": false,
              "properties": {
                "title": { "type": "string" },
                "summary": { "type": "string" },
                "detail": { "type": "string" },
                "evidence": { "type": ["string", "null"] },
                "decisions": {
                  "type": "array",
                  "items": { "type": "string" }
                },
                "actionItems": {
                  "type": "array",
                  "items": { "type": "string" }
                },
                "openQuestions": {
                  "type": "array",
                  "items": { "type": "string" }
                }
              },
              "required": ["title", "summary", "detail", "evidence", "decisions", "actionItems", "openQuestions"]
            }
          }
        },
        "required": ["title", "summary", "items"]
      }
    }
  },
  "required": ["suggestedTitle", "language", "overview", "topics", "decisions", "actionItems", "openQuestions", "summaryOutline"]
}"#
}

fn codex_command_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "codex.cmd"
    } else {
        "codex"
    }
}

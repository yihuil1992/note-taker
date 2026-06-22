use crate::storage::{
    get_app_settings, initialize_database, list_transcript_segments_for_meeting,
    update_meeting_status, update_meeting_title, upsert_meeting_summary, NewMeetingSummary,
    TranscriptSegmentRecord,
};
use crate::task_control::CancellationToken;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::Duration;
use thiserror::Error;

const DEFAULT_CODEX_MODEL: &str = "gpt-5.4";
const SUMMARY_PROVIDER: &str = "codex-cli";
const SUSPICIOUS_BOUNDARY_MAX_GAP_MS: i64 = 1_500;
const MAX_REFERENCE_FILE_BYTES: u64 = 80_000;
const MAX_REFERENCE_SCAN_FILES: usize = 600;

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
    #[error("Task control error: {0}")]
    TaskControl(#[from] crate::task_control::TaskControlError),
    #[error("Task cancelled by user")]
    Cancelled,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptNormalization {
    pub normalized_segments: Vec<NormalizedTranscriptSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NormalizedTranscriptSegment {
    pub operation: String,
    pub source_segment_ids: Vec<String>,
    pub source_kind: String,
    pub speaker_label: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub reason: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct TranscriptNormalizationInputSegment<'a> {
    id: &'a str,
    source_kind: &'a str,
    speaker_label: &'a str,
    start_ms: i64,
    end_ms: i64,
    text: &'a str,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceProject {
    pub id: String,
    pub display_name: String,
    pub path: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default = "default_selected")]
    pub default_selected: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceMatchingMode {
    AutoConservative,
    AutoBroad,
    ManualOnly,
}

impl Default for ReferenceMatchingMode {
    fn default() -> Self {
        Self::AutoConservative
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceContextDepth {
    ProjectSummaries,
    DocsAndRecentChanges,
    DocsAndMatchingSnippets,
}

impl Default for ReferenceContextDepth {
    fn default() -> Self {
        Self::DocsAndMatchingSnippets
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReferenceContextBudget {
    Small,
    Balanced,
    Deep,
}

impl Default for ReferenceContextBudget {
    fn default() -> Self {
        Self::Balanced
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SummaryRunOptions {
    pub model: Option<String>,
    pub language: Option<String>,
    #[serde(default)]
    pub matching_mode: ReferenceMatchingMode,
    #[serde(default)]
    pub context_depth: ReferenceContextDepth,
    #[serde(default)]
    pub context_budget: ReferenceContextBudget,
    #[serde(default = "default_include_git_changes")]
    pub include_git_changes: bool,
    #[serde(default)]
    pub selected_project_ids: Vec<String>,
    #[serde(default)]
    pub reference_projects: Vec<ReferenceProject>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceContextMetadata {
    pub mode: String,
    pub depth: String,
    pub budget: String,
    pub included_projects: Vec<ReferenceProjectMetadata>,
    pub skipped_projects: Vec<SkippedReferenceProjectMetadata>,
    pub truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReferenceProjectMetadata {
    pub id: String,
    pub display_name: String,
    pub match_level: String,
    pub reasons: Vec<String>,
    pub included_files: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkippedReferenceProjectMetadata {
    pub id: String,
    pub display_name: String,
    pub reason: String,
}

struct ReferenceContextPack {
    prompt_section: String,
    metadata: ReferenceContextMetadata,
}

struct ReferenceSnippet {
    relative_path: String,
    text: String,
    score: usize,
}

fn default_selected() -> bool {
    true
}

fn default_include_git_changes() -> bool {
    false
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
    summarize_meeting_with_codex_model_with_cancel(database_path, work_dir, meeting_id, model, None)
}

pub fn summarize_meeting_with_codex_model_with_cancel(
    database_path: &Path,
    work_dir: &Path,
    meeting_id: &str,
    model: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<MeetingSummaryResult, SummaryError> {
    summarize_meeting_with_options(
        database_path,
        work_dir,
        meeting_id,
        None,
        Some(model),
        cancellation,
    )
}

pub fn summarize_meeting_with_options(
    database_path: &Path,
    work_dir: &Path,
    meeting_id: &str,
    options: Option<SummaryRunOptions>,
    fallback_model: Option<&str>,
    cancellation: Option<&CancellationToken>,
) -> Result<MeetingSummaryResult, SummaryError> {
    initialize_database(database_path)?;
    fs::create_dir_all(work_dir)?;
    update_progress(cancellation, "preparing", "Loading transcript", 0, Some(6))?;
    let settings = get_app_settings(database_path)?;
    let options =
        options.unwrap_or_else(|| default_summary_options(&settings.reference_projects_json));
    let model = options
        .model
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(fallback_model)
        .unwrap_or(settings.summary_model.as_str());
    let summary_language = options
        .language
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(settings.summary_language.as_str());
    let segments = list_transcript_segments_for_meeting(database_path, meeting_id)?;
    if segments.is_empty() {
        return Err(SummaryError::EmptyTranscript(meeting_id.to_string()));
    }

    update_meeting_status(database_path, meeting_id, "summarizing")?;
    update_progress(
        cancellation,
        "normalizing",
        "Preparing transcript cleanup",
        1,
        Some(6),
    )?;
    let normalization = match normalize_transcript_with_codex(
        work_dir,
        meeting_id,
        &segments,
        model,
        &settings.custom_glossary,
        cancellation,
    ) {
        Ok(normalization) => normalization,
        Err(SummaryError::Cancelled) => {
            update_meeting_status(database_path, meeting_id, "summary_cancelled")?;
            return Err(SummaryError::Cancelled);
        }
        Err(error) => {
            update_meeting_status(database_path, meeting_id, "summary_failed")?;
            return Err(error);
        }
    };

    update_progress(
        cancellation,
        "preparing",
        "Preparing summary prompt",
        3,
        Some(6),
    )?;
    let transcript = render_normalized_transcript(&normalization.normalized_segments);
    let reference_context = build_reference_context_pack(&transcript, &options);
    let schema_path = work_dir.join("summary.schema.json");
    let output_path = work_dir.join(format!("summary-{meeting_id}.json"));
    fs::write(&schema_path, summary_schema_json())?;

    let prompt = build_summary_prompt(
        meeting_id,
        &transcript,
        &settings.custom_glossary,
        summary_language,
        reference_context
            .as_ref()
            .map(|context| context.prompt_section.as_str()),
    );
    if is_cancelled(cancellation) {
        update_meeting_status(database_path, meeting_id, "summary_cancelled")?;
        return Err(SummaryError::Cancelled);
    }
    update_progress(
        cancellation,
        "summarizing",
        "Generating summary with Codex",
        4,
        Some(6),
    )?;
    match run_codex_json(model, &schema_path, &output_path, &prompt, cancellation) {
        Ok(()) => {}
        Err(SummaryError::Cancelled) => {
            update_meeting_status(database_path, meeting_id, "summary_cancelled")?;
            return Err(SummaryError::Cancelled);
        }
        Err(error) => {
            update_meeting_status(database_path, meeting_id, "summary_failed")?;
            return Err(error);
        }
    }

    update_progress(cancellation, "saving", "Saving summary", 5, Some(6))?;
    let raw_json = fs::read_to_string(&output_path)?;
    let summary: CodexSummary = serde_json::from_str(raw_json.trim())?;
    let raw_json = attach_transcript_normalization(raw_json.trim(), &normalization)?;
    let raw_json = attach_reference_context(raw_json.trim(), reference_context.as_ref())?;
    let result = persist_summary(database_path, meeting_id, model, summary, &raw_json)?;
    update_progress(cancellation, "complete", "Summary complete", 6, Some(6))?;
    Ok(result)
}

fn default_summary_options(reference_projects_json: &str) -> SummaryRunOptions {
    let reference_projects =
        serde_json::from_str::<Vec<ReferenceProject>>(reference_projects_json).unwrap_or_default();
    let selected_project_ids = reference_projects
        .iter()
        .filter(|project| project.default_selected)
        .map(|project| project.id.clone())
        .collect();
    SummaryRunOptions {
        reference_projects,
        selected_project_ids,
        ..SummaryRunOptions::default()
    }
}

fn normalize_transcript_with_codex(
    work_dir: &Path,
    meeting_id: &str,
    segments: &[TranscriptSegmentRecord],
    model: &str,
    custom_glossary: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<TranscriptNormalization, SummaryError> {
    let candidate_segments = suspicious_normalization_segments(segments);
    if candidate_segments.is_empty() {
        return Ok(raw_segments_as_normalization(segments));
    }

    let schema_path = work_dir.join("transcript-normalization.schema.json");
    let output_path = work_dir.join(format!("transcript-normalization-{meeting_id}.json"));
    fs::write(&schema_path, transcript_normalization_schema_json())?;
    let prompt =
        build_transcript_normalization_prompt(meeting_id, &candidate_segments, custom_glossary)?;

    update_progress(
        cancellation,
        "normalizing",
        &format!(
            "Cleaning {} suspicious transcript segments with Codex",
            candidate_segments.len()
        ),
        2,
        Some(6),
    )?;
    run_codex_json(model, &schema_path, &output_path, &prompt, cancellation)?;

    let raw_json = fs::read_to_string(&output_path)?;
    let normalization: TranscriptNormalization = serde_json::from_str(raw_json.trim())?;
    Ok(sanitize_partial_transcript_normalization(
        normalization,
        segments,
        &candidate_segments,
    ))
}

fn run_codex_json(
    model: &str,
    schema_path: &Path,
    output_path: &Path,
    prompt: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<(), SummaryError> {
    let mut command = Command::new(codex_command_name());
    command
        .arg("exec")
        .arg("-m")
        .arg(model)
        .arg("--skip-git-repo-check")
        .arg("--ephemeral")
        .arg("--ignore-user-config")
        .arg("--sandbox")
        .arg("read-only")
        .arg("--output-schema")
        .arg(schema_path)
        .arg("--output-last-message")
        .arg(output_path)
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    crate::process::suppress_console_window(&mut command);
    let mut child = command.spawn()?;

    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(prompt.as_bytes())?;
    }
    let output = wait_for_codex_output(child, cancellation)?;
    if output.status.success() {
        return Ok(());
    }
    if is_cancelled(cancellation) {
        return Err(SummaryError::Cancelled);
    }
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    Err(SummaryError::CodexFailed {
        code: output.status.code(),
        stderr: summarize_codex_stderr(&stderr),
    })
}

fn wait_for_codex_output(
    mut child: std::process::Child,
    cancellation: Option<&CancellationToken>,
) -> Result<Output, SummaryError> {
    let mut stdout_reader = child.stdout.take().map(read_pipe_in_background);
    let mut stderr_reader = child.stderr.take().map(read_pipe_in_background);

    let status = loop {
        if is_cancelled(cancellation) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = collect_pipe_reader(stdout_reader.take());
            let _ = collect_pipe_reader(stderr_reader.take());
            return Err(SummaryError::Cancelled);
        }
        if let Some(status) = child.try_wait()? {
            break status;
        }
        thread::sleep(Duration::from_millis(150));
    };

    let stdout = collect_pipe_reader(stdout_reader)?;
    let stderr = collect_pipe_reader(stderr_reader)?;
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

fn read_pipe_in_background<R>(mut reader: R) -> thread::JoinHandle<std::io::Result<Vec<u8>>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut output = Vec::new();
        reader.read_to_end(&mut output)?;
        Ok(output)
    })
}

fn collect_pipe_reader(
    reader: Option<thread::JoinHandle<std::io::Result<Vec<u8>>>>,
) -> Result<Vec<u8>, SummaryError> {
    let Some(reader) = reader else {
        return Ok(Vec::new());
    };

    reader
        .join()
        .map_err(|_| {
            SummaryError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                "failed to join Codex output reader thread",
            ))
        })?
        .map_err(SummaryError::Io)
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

fn build_transcript_normalization_prompt(
    meeting_id: &str,
    segments: &[&TranscriptSegmentRecord],
    custom_glossary: &str,
) -> Result<String, SummaryError> {
    let glossary_section = render_glossary_section(custom_glossary);
    let input_segments = segments
        .iter()
        .map(|segment| TranscriptNormalizationInputSegment {
            id: &segment.id,
            source_kind: &segment.source_kind,
            speaker_label: &segment.speaker_label,
            start_ms: segment.start_ms,
            end_ms: segment.end_ms,
            text: &segment.text,
        })
        .collect::<Vec<_>>();
    let input_json = serde_json::to_string_pretty(&input_segments)?;
    Ok(format!(
        r#"You are cleaning suspicious transcript segment boundaries before meeting summarization.

Return valid JSON matching the provided schema.

Goal:
- Produce normalizedSegments for ONLY the suspicious local segment set below.
- The rest of the meeting transcript will be preserved by the app without being sent here.
- The returned normalizedSegments should read as natural sentences or short utterances.
- You may keep, merge, or split boundaries.
- This is boundary cleanup only. Do not summarize, translate, paraphrase, correct factual content, or invent missing words.

Rules:
- Preserve the original transcript wording as much as possible. Only trim extra whitespace at boundaries.
- Keep every substantive word from the input. Do not drop asides, corrections, examples, or partial terms.
- Cover every provided source segment id exactly once, either by keeping it, merging it with adjacent provided segments, or splitting it into multiple normalizedSegments.
- Use operation "keep" when one source segment is already a natural sentence/utterance.
- Use operation "merge" when adjacent segments from the same sourceKind and speakerLabel are clearly one unfinished sentence or thought.
- Use operation "split" when one source segment clearly contains two or more natural sentences/utterances.
- Do not merge across different sourceKind or speakerLabel values.
- Keep normalizedSegments in chronological order.
- Use sourceSegmentIds to identify the raw segment or adjacent raw segments that produced each normalized segment.
- For split segments, repeat the same sourceSegmentIds entry and estimate startMs/endMs within the raw segment range.
- For merge segments, startMs should be the first source segment startMs and endMs should be the last source segment endMs.
- If uncertain whether a boundary is wrong, keep the source segment unchanged.
- reason should be short and describe the boundary decision.
{glossary_section}

Meeting id: {meeting_id}

Suspicious transcript segments JSON:
{input_json}
"#
    ))
}

fn suspicious_normalization_segments(
    segments: &[TranscriptSegmentRecord],
) -> Vec<&TranscriptSegmentRecord> {
    let mut candidate_ids = HashSet::<&str>::new();
    for pair in segments.windows(2) {
        let left = &pair[0];
        let right = &pair[1];
        if is_suspicious_boundary(left, right) {
            candidate_ids.insert(left.id.as_str());
            candidate_ids.insert(right.id.as_str());
        }
    }

    segments
        .iter()
        .filter(|segment| candidate_ids.contains(segment.id.as_str()))
        .collect()
}

fn is_suspicious_boundary(left: &TranscriptSegmentRecord, right: &TranscriptSegmentRecord) -> bool {
    if left.source_kind != right.source_kind
        || left.speaker_label != right.speaker_label
        || left.provider != right.provider
        || left.language != right.language
    {
        return false;
    }
    let gap_ms = right.start_ms - left.end_ms;
    if gap_ms.abs() > SUSPICIOUS_BOUNDARY_MAX_GAP_MS {
        return false;
    }
    !has_sentence_boundary(&left.text)
}

fn has_sentence_boundary(text: &str) -> bool {
    text.trim_end()
        .chars()
        .last()
        .map(|character| matches!(character, '。' | '！' | '？' | '；' | '.' | '!' | '?' | ';'))
        .unwrap_or(false)
}

#[cfg(test)]
fn sanitize_transcript_normalization(
    normalization: TranscriptNormalization,
    raw_segments: &[TranscriptSegmentRecord],
) -> TranscriptNormalization {
    let mut normalized_segments =
        validate_normalized_segments(normalization.normalized_segments, raw_segments);
    if normalized_segments.is_empty() {
        return raw_segments_as_normalization(raw_segments);
    }

    normalized_segments.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
    TranscriptNormalization {
        normalized_segments,
    }
}

fn sanitize_partial_transcript_normalization(
    normalization: TranscriptNormalization,
    raw_segments: &[TranscriptSegmentRecord],
    candidate_segments: &[&TranscriptSegmentRecord],
) -> TranscriptNormalization {
    let candidate_ids = candidate_segments
        .iter()
        .map(|segment| segment.id.as_str())
        .collect::<HashSet<_>>();
    let mut normalized_segments =
        validate_normalized_segments(normalization.normalized_segments, raw_segments)
            .into_iter()
            .filter(|segment| {
                !segment.source_segment_ids.is_empty()
                    && segment
                        .source_segment_ids
                        .iter()
                        .all(|id| candidate_ids.contains(id.as_str()))
            })
            .collect::<Vec<_>>();
    if normalized_segments.is_empty() {
        return raw_segments_as_normalization(raw_segments);
    }

    normalized_segments.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
    merge_partial_normalization(raw_segments, normalized_segments)
}

fn validate_normalized_segments(
    segments: Vec<NormalizedTranscriptSegment>,
    raw_segments: &[TranscriptSegmentRecord],
) -> Vec<NormalizedTranscriptSegment> {
    let id_map = raw_segments
        .iter()
        .map(|segment| (segment.id.as_str(), segment))
        .collect::<HashMap<_, _>>();
    let id_index = raw_segments
        .iter()
        .enumerate()
        .map(|(index, segment)| (segment.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut normalized_segments = Vec::new();

    for segment in segments {
        let mut source_segment_ids = Vec::new();
        let mut source_records = Vec::new();
        for id in &segment.source_segment_ids {
            if source_segment_ids.iter().any(|existing| existing == id) {
                continue;
            }
            let Some(source) = id_map.get(id.as_str()) else {
                continue;
            };
            source_segment_ids.push(id.clone());
            source_records.push(*source);
        }
        let Some(first_source) = source_records.first() else {
            continue;
        };
        if source_records.iter().any(|source| {
            source.source_kind != first_source.source_kind
                || source.speaker_label != first_source.speaker_label
        }) {
            continue;
        }
        let source_indices = source_segment_ids
            .iter()
            .filter_map(|id| id_index.get(id.as_str()).copied())
            .collect::<Vec<_>>();
        let Some(min_index) = source_indices.iter().min().copied() else {
            continue;
        };
        let Some(max_index) = source_indices.iter().max().copied() else {
            continue;
        };
        if max_index - min_index + 1 != source_indices.len() {
            continue;
        }
        source_segment_ids
            .sort_by_key(|id| id_index.get(id.as_str()).copied().unwrap_or(usize::MAX));

        let min_start = source_records
            .iter()
            .map(|source| source.start_ms)
            .min()
            .unwrap_or(first_source.start_ms);
        let max_end = source_records
            .iter()
            .map(|source| source.end_ms)
            .max()
            .unwrap_or(first_source.end_ms);
        if max_end <= min_start {
            continue;
        }

        let mut start_ms = segment.start_ms.clamp(min_start, max_end - 1);
        let mut end_ms = segment.end_ms.clamp(start_ms + 1, max_end);
        if end_ms <= start_ms {
            start_ms = min_start;
            end_ms = max_end;
        }

        let text = compact_whitespace(&segment.text);
        if text.is_empty() {
            continue;
        }
        let operation = match segment.operation.as_str() {
            "keep" | "merge" | "split" => segment.operation,
            _ => "keep".to_string(),
        };
        normalized_segments.push(NormalizedTranscriptSegment {
            operation,
            source_segment_ids,
            source_kind: first_source.source_kind.clone(),
            speaker_label: first_source.speaker_label.clone(),
            start_ms,
            end_ms,
            text,
            reason: truncate_for_ui(&compact_whitespace(&segment.reason), 240),
        });
    }

    normalized_segments
}

fn merge_partial_normalization(
    raw_segments: &[TranscriptSegmentRecord],
    normalized_segments: Vec<NormalizedTranscriptSegment>,
) -> TranscriptNormalization {
    let id_index = raw_segments
        .iter()
        .enumerate()
        .map(|(index, segment)| (segment.id.as_str(), index))
        .collect::<HashMap<_, _>>();
    let mut replacements: HashMap<String, Vec<NormalizedTranscriptSegment>> = HashMap::new();
    let mut covered_ids = HashSet::<String>::new();

    for segment in normalized_segments {
        let Some(first_id) = segment
            .source_segment_ids
            .iter()
            .min_by_key(|id| id_index.get(id.as_str()).copied().unwrap_or(usize::MAX))
            .cloned()
        else {
            continue;
        };
        for id in &segment.source_segment_ids {
            covered_ids.insert(id.clone());
        }
        replacements.entry(first_id).or_default().push(segment);
    }

    let mut merged = Vec::new();
    for raw in raw_segments {
        if let Some(mut replacement) = replacements.remove(&raw.id) {
            replacement.sort_by_key(|segment| (segment.start_ms, segment.end_ms));
            merged.extend(replacement);
            continue;
        }
        if covered_ids.contains(&raw.id) {
            continue;
        }
        merged.push(NormalizedTranscriptSegment {
            operation: "keep".to_string(),
            source_segment_ids: vec![raw.id.clone()],
            source_kind: raw.source_kind.clone(),
            speaker_label: raw.speaker_label.clone(),
            start_ms: raw.start_ms,
            end_ms: raw.end_ms,
            text: compact_whitespace(&raw.text),
            reason: "Original ASR segment preserved.".to_string(),
        });
    }

    TranscriptNormalization {
        normalized_segments: merged,
    }
}

fn raw_segments_as_normalization(
    raw_segments: &[TranscriptSegmentRecord],
) -> TranscriptNormalization {
    TranscriptNormalization {
        normalized_segments: raw_segments
            .iter()
            .map(|segment| NormalizedTranscriptSegment {
                operation: "keep".to_string(),
                source_segment_ids: vec![segment.id.clone()],
                source_kind: segment.source_kind.clone(),
                speaker_label: segment.speaker_label.clone(),
                start_ms: segment.start_ms,
                end_ms: segment.end_ms,
                text: compact_whitespace(&segment.text),
                reason: "Original ASR segment preserved.".to_string(),
            })
            .collect(),
    }
}

fn render_normalized_transcript(segments: &[NormalizedTranscriptSegment]) -> String {
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

fn build_reference_context_pack(
    transcript: &str,
    options: &SummaryRunOptions,
) -> Option<ReferenceContextPack> {
    let selected_ids = options
        .selected_project_ids
        .iter()
        .map(|id| id.as_str())
        .collect::<HashSet<_>>();
    let candidate_projects = options
        .reference_projects
        .iter()
        .filter(|project| selected_ids.contains(project.id.as_str()))
        .collect::<Vec<_>>();

    if candidate_projects.is_empty() {
        return None;
    }

    let total_budget_chars = context_budget_chars(&options.context_budget);
    let per_project_chars = total_budget_chars
        .checked_div(candidate_projects.len())
        .unwrap_or(total_budget_chars)
        .max(1);
    let mut included_projects = Vec::new();
    let mut skipped_projects = Vec::new();
    let mut prompt_parts = Vec::new();
    let mut truncated = false;

    for project in candidate_projects {
        let project_path = PathBuf::from(&project.path);
        if !project_path.is_dir() {
            skipped_projects.push(SkippedReferenceProjectMetadata {
                id: project.id.clone(),
                display_name: project.display_name.clone(),
                reason: "Folder is missing or not readable".to_string(),
            });
            continue;
        }

        let (match_level, reasons) = score_project_match(transcript, project);
        let terms = reference_terms(transcript, project);
        let mut snippets =
            collect_reference_snippets(&project_path, &terms, &options.context_depth);
        if options.include_git_changes
            && matches!(
                options.context_depth,
                ReferenceContextDepth::DocsAndRecentChanges
                    | ReferenceContextDepth::DocsAndMatchingSnippets
            )
        {
            if let Some(git_summary) = collect_git_summary(&project_path) {
                snippets.push(git_summary);
            }
        }
        if snippets.is_empty() {
            skipped_projects.push(SkippedReferenceProjectMetadata {
                id: project.id.clone(),
                display_name: project.display_name.clone(),
                reason: "No eligible reference files found".to_string(),
            });
            continue;
        }

        let mut remaining_project_chars = per_project_chars;
        let mut project_text = format!(
            "Project: {}\nFolder: {}\nRelevance hint: {} ({})\n",
            project.display_name,
            project.path,
            match_level,
            if reasons.is_empty() {
                "selected by user; no exact lexical hit".to_string()
            } else {
                reasons.join("; ")
            }
        );
        let mut included_files = Vec::new();
        for snippet in snippets {
            if remaining_project_chars == 0 {
                truncated = true;
                break;
            }
            let block = format!(
                "\nFile: {}\n---\n{}\n---\n",
                snippet.relative_path, snippet.text
            );
            let allowed = take_char_budget(&block, remaining_project_chars);
            remaining_project_chars =
                remaining_project_chars.saturating_sub(allowed.chars().count());
            if allowed.len() < block.len() {
                truncated = true;
            }
            project_text.push_str(&allowed);
            included_files.push(snippet.relative_path);
            if allowed.len() < block.len() {
                break;
            }
        }

        if included_files.is_empty() {
            skipped_projects.push(SkippedReferenceProjectMetadata {
                id: project.id.clone(),
                display_name: project.display_name.clone(),
                reason: "Per-project reference context budget exhausted".to_string(),
            });
            continue;
        }

        prompt_parts.push(project_text);
        included_projects.push(ReferenceProjectMetadata {
            id: project.id.clone(),
            display_name: project.display_name.clone(),
            match_level,
            reasons,
            included_files,
        });
    }

    let metadata = ReferenceContextMetadata {
        mode: "codex-guided".to_string(),
        depth: context_depth_label(&options.context_depth).to_string(),
        budget: context_budget_label(&options.context_budget).to_string(),
        included_projects,
        skipped_projects,
        truncated,
    };

    let prompt_section = if prompt_parts.is_empty() {
        String::new()
    } else {
        format!(
            r#"
Reference project context:
The following local project context comes from the user-selected candidate project folders.
The app has intentionally not excluded projects by keyword matching. You must decide which selected project context is relevant to the transcript.
Use relevant context to disambiguate project names, modules, files, product terms, and implementation details mentioned in the transcript. Ignore irrelevant project context.
Do not add a reference-files section, bibliography, appendix, or source list to the summary content.
If the transcript does not support a claim, do not invent it from the reference context alone.

{}
"#,
            prompt_parts.join("\n\n")
        )
    };

    Some(ReferenceContextPack {
        prompt_section,
        metadata,
    })
}

fn score_project_match(transcript: &str, project: &ReferenceProject) -> (String, Vec<String>) {
    if matches!(project.id.as_str(), "") {
        return ("none".to_string(), Vec::new());
    }
    let transcript_lower = transcript.to_lowercase();
    let mut reasons = Vec::new();
    let mut score = 0usize;
    for term in std::iter::once(project.display_name.as_str())
        .chain(std::iter::once(project.id.as_str()))
        .chain(project.aliases.iter().map(String::as_str))
    {
        let normalized = term.trim().to_lowercase();
        if normalized.len() < 3 {
            continue;
        }
        if transcript_lower.contains(&normalized) {
            score += if normalized.split_whitespace().count() > 1 {
                3
            } else {
                2
            };
            reasons.push(format!("alias: {term}"));
        }
    }

    let level = if score >= 3 {
        "high"
    } else if score >= 1 {
        "medium"
    } else {
        "none"
    };
    (level.to_string(), reasons)
}

fn reference_terms(transcript: &str, project: &ReferenceProject) -> Vec<String> {
    let mut terms = Vec::new();
    for term in std::iter::once(project.display_name.as_str())
        .chain(std::iter::once(project.id.as_str()))
        .chain(project.aliases.iter().map(String::as_str))
    {
        let term = term.trim();
        if term.len() >= 3 && !terms.iter().any(|existing| existing == term) {
            terms.push(term.to_string());
        }
    }
    for word in transcript
        .split(|character: char| {
            !character.is_alphanumeric() && character != '-' && character != '_'
        })
        .map(str::trim)
        .filter(|word| word.len() >= 5)
        .take(80)
    {
        if !terms
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(word))
        {
            terms.push(word.to_string());
        }
    }
    terms
}

fn collect_reference_snippets(
    project_path: &Path,
    terms: &[String],
    depth: &ReferenceContextDepth,
) -> Vec<ReferenceSnippet> {
    let mut files = Vec::new();
    for relative in [
        "README.md",
        "PRODUCT.md",
        "DESIGN.md",
        "docs/SYSTEM_MAP.md",
        "agent-docs/SYSTEM_MAP.md",
        "AGENTS.md",
        "AGENT.md",
    ] {
        push_reference_file(project_path, relative, terms, 100, &mut files);
    }

    if matches!(
        depth,
        ReferenceContextDepth::DocsAndRecentChanges
            | ReferenceContextDepth::DocsAndMatchingSnippets
    ) {
        collect_files_from_dir(project_path, Path::new("docs"), terms, 25, &mut files);
        collect_files_from_dir(project_path, Path::new("agent-docs"), terms, 25, &mut files);
    }

    if matches!(depth, ReferenceContextDepth::DocsAndMatchingSnippets) {
        collect_files_from_dir(project_path, Path::new("src"), terms, 10, &mut files);
        collect_files_from_dir(project_path, Path::new("frontend"), terms, 10, &mut files);
        collect_files_from_dir(project_path, Path::new("backend"), terms, 10, &mut files);
        collect_files_from_dir(project_path, Path::new("app"), terms, 10, &mut files);
    }

    files.sort_by(|left, right| {
        right
            .score
            .cmp(&left.score)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });
    files.dedup_by(|left, right| left.relative_path == right.relative_path);
    files.truncate(10);
    files
}

fn collect_git_summary(project_path: &Path) -> Option<ReferenceSnippet> {
    let branch = run_git_text(project_path, &["branch", "--show-current"]).unwrap_or_default();
    let log =
        run_git_text(project_path, &["log", "-3", "--pretty=format:%h %cs %s"]).unwrap_or_default();
    let status = run_git_text(project_path, &["status", "--short"]).unwrap_or_default();
    if branch.trim().is_empty() && log.trim().is_empty() && status.trim().is_empty() {
        return None;
    }
    let mut text = String::new();
    if !branch.trim().is_empty() {
        text.push_str("Current branch: ");
        text.push_str(branch.trim());
        text.push('\n');
    }
    if !log.trim().is_empty() {
        text.push_str("Recent commits:\n");
        text.push_str(log.trim());
        text.push('\n');
    }
    if !status.trim().is_empty() {
        text.push_str("Working tree status:\n");
        text.push_str(status.trim());
        text.push('\n');
    }
    Some(ReferenceSnippet {
        relative_path: "git/recent-changes".to_string(),
        text,
        score: 95,
    })
}

fn run_git_text(project_path: &Path, args: &[&str]) -> Option<String> {
    let mut command = Command::new("git");
    command.args(args).current_dir(project_path);
    crate::process::suppress_console_window(&mut command);
    let output = command.output().ok()?;
    if !output.status.success() {
        return None;
    }
    Some(String::from_utf8_lossy(&output.stdout).to_string())
}

fn collect_files_from_dir(
    project_path: &Path,
    relative_dir: &Path,
    terms: &[String],
    default_score: usize,
    files: &mut Vec<ReferenceSnippet>,
) {
    let root = project_path.join(relative_dir);
    if !root.is_dir() {
        return;
    }
    let mut stack = vec![root];
    let mut scanned = 0usize;
    while let Some(dir) = stack.pop() {
        if scanned >= MAX_REFERENCE_SCAN_FILES {
            break;
        }
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            if scanned >= MAX_REFERENCE_SCAN_FILES {
                break;
            }
            let path = entry.path();
            let file_name = entry.file_name().to_string_lossy().to_string();
            if should_skip_reference_path(&file_name) {
                continue;
            }
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            scanned += 1;
            let Some(relative_path) = path
                .strip_prefix(project_path)
                .ok()
                .map(|value| value.to_string_lossy().replace('\\', "/"))
            else {
                continue;
            };
            if !is_reference_text_file(&path) {
                continue;
            }
            push_reference_file(project_path, &relative_path, terms, default_score, files);
        }
    }
}

fn push_reference_file(
    project_path: &Path,
    relative: &str,
    terms: &[String],
    base_score: usize,
    files: &mut Vec<ReferenceSnippet>,
) {
    if files.iter().any(|file| file.relative_path == relative) {
        return;
    }
    let path = project_path.join(relative);
    if !path.is_file() || !is_reference_text_file(&path) {
        return;
    }
    let Ok(metadata) = fs::metadata(&path) else {
        return;
    };
    if metadata.len() > MAX_REFERENCE_FILE_BYTES {
        return;
    }
    let Ok(raw) = fs::read_to_string(&path) else {
        return;
    };
    let score = base_score + score_text_against_terms(&raw, terms);
    if base_score < 50 && score == base_score {
        return;
    }
    files.push(ReferenceSnippet {
        relative_path: relative.replace('\\', "/"),
        text: truncate_for_ui(&raw, 5_000),
        score,
    });
}

fn score_text_against_terms(text: &str, terms: &[String]) -> usize {
    let text = text.to_lowercase();
    terms
        .iter()
        .map(|term| term.trim().to_lowercase())
        .filter(|term| term.len() >= 3 && text.contains(term))
        .count()
}

fn should_skip_reference_path(name: &str) -> bool {
    let name = name.to_lowercase();
    matches!(
        name.as_str(),
        ".git"
            | "node_modules"
            | ".next"
            | "dist"
            | "build"
            | "target"
            | "venv"
            | ".venv"
            | "__pycache__"
            | "uploads"
            | "logs"
            | ".pytest_cache"
    ) || name.ends_with(".log")
        || name.ends_with(".db")
        || name.ends_with(".sqlite")
        || name.ends_with(".sqlite3")
        || name.ends_with(".dump")
        || name.ends_with(".zip")
        || name.ends_with(".png")
        || name.ends_with(".jpg")
        || name.ends_with(".jpeg")
        || name.ends_with(".gif")
        || name.ends_with(".webp")
        || name.ends_with(".psd")
        || name == ".env"
}

fn is_reference_text_file(path: &Path) -> bool {
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("")
        .to_lowercase();
    matches!(
        extension.as_str(),
        "md" | "txt"
            | "html"
            | "ts"
            | "tsx"
            | "js"
            | "jsx"
            | "py"
            | "rs"
            | "json"
            | "yml"
            | "yaml"
            | "toml"
            | "css"
    )
}

fn context_budget_chars(budget: &ReferenceContextBudget) -> usize {
    match budget {
        ReferenceContextBudget::Small => 10_000,
        ReferenceContextBudget::Balanced => 24_000,
        ReferenceContextBudget::Deep => 45_000,
    }
}

fn take_char_budget(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars {
        return value.to_string();
    }
    let mut output = value
        .chars()
        .take(max_chars.saturating_sub(3))
        .collect::<String>();
    output.push_str("...");
    output
}

fn context_depth_label(depth: &ReferenceContextDepth) -> &'static str {
    match depth {
        ReferenceContextDepth::ProjectSummaries => "project-summaries",
        ReferenceContextDepth::DocsAndRecentChanges => "docs-and-recent-changes",
        ReferenceContextDepth::DocsAndMatchingSnippets => "docs-and-matching-snippets",
    }
}

fn context_budget_label(budget: &ReferenceContextBudget) -> &'static str {
    match budget {
        ReferenceContextBudget::Small => "small",
        ReferenceContextBudget::Balanced => "balanced",
        ReferenceContextBudget::Deep => "deep",
    }
}

fn compact_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn attach_transcript_normalization(
    raw_summary_json: &str,
    normalization: &TranscriptNormalization,
) -> Result<String, SummaryError> {
    let mut value: serde_json::Value = serde_json::from_str(raw_summary_json)?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "transcriptNormalization".to_string(),
            serde_json::to_value(normalization)?,
        );
    }
    Ok(serde_json::to_string(&value)?)
}

fn attach_reference_context(
    raw_summary_json: &str,
    reference_context: Option<&ReferenceContextPack>,
) -> Result<String, SummaryError> {
    let Some(reference_context) = reference_context else {
        return Ok(raw_summary_json.to_string());
    };
    let mut value: serde_json::Value = serde_json::from_str(raw_summary_json)?;
    if let Some(object) = value.as_object_mut() {
        object.insert(
            "referenceContext".to_string(),
            serde_json::to_value(&reference_context.metadata)?,
        );
    }
    Ok(serde_json::to_string(&value)?)
}

fn build_summary_prompt(
    meeting_id: &str,
    transcript: &str,
    custom_glossary: &str,
    summary_language: &str,
    reference_context: Option<&str>,
) -> String {
    let glossary_section = render_glossary_section(custom_glossary);
    let reference_section = reference_context.unwrap_or("");
    let language_rule = match summary_language {
        "zh" => "The summary language should be Simplified Chinese.",
        "ja" => "The summary language should be Japanese.",
        "en" => "The summary language should be English.",
        _ => "The summary language should be Simplified Chinese when the meeting is mixed-language or unclear.",
    };
    format!(
        r#"You are summarizing a locally captured meeting transcript.

Return valid JSON matching the provided schema.

Goal:
- Produce one integrated, structured meeting record, not a short executive summary plus a separate detail dump.
- The user must be able to review the meeting without rereading the whole transcript.
- Do not worry that the output is long. Long meetings should produce long, structured notes.

Rules:
- {language_rule}
- The transcript has already passed a boundary-normalization step. Treat each line as the best available transcript segment, while still tolerating minor ASR errors.
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
- Do not add a reference-files section, bibliography, appendix, or source list to the summary content. Reference project context is for disambiguation only.
- If the transcript does not support a claim, do not invent it from reference project context alone.
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
{glossary_section}
{reference_section}

Transcript:
{transcript}
"#
    )
}

fn render_glossary_section(custom_glossary: &str) -> String {
    let glossary = custom_glossary.trim();
    if glossary.is_empty() {
        return String::new();
    }

    format!(
        r#"
User glossary:
{glossary}

Glossary rules:
- Prefer the glossary spelling for matching people, products, teams, projects, acronyms, and internal terms.
- Treat explanations after ":" or "-" as context for understanding and summarizing the transcript.
- Do not invent glossary terms that are not supported by the transcript.
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

fn is_cancelled(cancellation: Option<&CancellationToken>) -> bool {
    cancellation
        .map(CancellationToken::is_cancelled)
        .unwrap_or(false)
}

fn update_progress(
    cancellation: Option<&CancellationToken>,
    phase: &str,
    message: &str,
    current: u32,
    total: Option<u32>,
) -> Result<(), SummaryError> {
    if let Some(token) = cancellation {
        token.update_progress(phase, message, current, total)?;
    }
    Ok(())
}

fn transcript_normalization_schema_json() -> &'static str {
    r#"{
  "type": "object",
  "additionalProperties": false,
  "properties": {
    "normalizedSegments": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "properties": {
          "operation": {
            "type": "string",
            "enum": ["keep", "merge", "split"]
          },
          "sourceSegmentIds": {
            "type": "array",
            "items": { "type": "string" }
          },
          "sourceKind": { "type": "string" },
          "speakerLabel": { "type": "string" },
          "startMs": { "type": "integer" },
          "endMs": { "type": "integer" },
          "text": { "type": "string" },
          "reason": { "type": "string" }
        },
        "required": ["operation", "sourceSegmentIds", "sourceKind", "speakerLabel", "startMs", "endMs", "text", "reason"]
      }
    }
  },
  "required": ["normalizedSegments"]
}"#
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

#[cfg(test)]
mod tests {
    use super::*;

    fn transcript_segment(
        id: &str,
        source_kind: &str,
        speaker_label: &str,
        start_ms: i64,
        end_ms: i64,
        text: &str,
    ) -> TranscriptSegmentRecord {
        TranscriptSegmentRecord {
            id: id.to_string(),
            meeting_id: "meeting-1".to_string(),
            source_kind: source_kind.to_string(),
            speaker_label: speaker_label.to_string(),
            language: "auto".to_string(),
            start_ms,
            end_ms,
            text: text.to_string(),
            provider: "openai-api:gpt-4o-transcribe".to_string(),
        }
    }

    #[test]
    fn summary_prompt_includes_custom_glossary() {
        let prompt = build_summary_prompt(
            "meeting-1",
            "[0:00-0:10] microphone / Me: We discussed RAG.",
            "RAG: retrieval augmented generation\nNote Taker: project name",
            "auto",
            None,
        );

        assert!(prompt.contains("User glossary"));
        assert!(prompt.contains("RAG: retrieval augmented generation"));
        assert!(prompt.contains("Do not invent glossary terms"));
    }

    #[test]
    fn summary_prompt_keeps_reference_context_out_of_summary_sections() {
        let prompt = build_summary_prompt(
            "meeting-1",
            "[0:00-0:10] microphone / Me: We discussed VisApp sync.",
            "",
            "zh",
            Some("Reference project context:\nProject: VisApp\nFile: docs/SYSTEM_MAP.md\n---\nSync API details\n---"),
        );

        assert!(prompt.contains("The summary language should be Simplified Chinese."));
        assert!(prompt.contains("Reference project context"));
        assert!(prompt.contains("Do not add a reference-files section"));
        assert!(prompt.contains("If the transcript does not support a claim"));
    }

    #[test]
    fn transcript_normalization_prompt_allows_keep_merge_and_split() {
        let segment = transcript_segment(
            "segment-a",
            "microphone",
            "Me",
            0,
            4_000,
            "第一句。第二句。",
        );
        let segments = vec![&segment];

        let prompt = build_transcript_normalization_prompt("meeting-1", &segments, "")
            .expect("build normalization prompt");

        assert!(prompt.contains("keep, merge, or split"));
        assert!(prompt.contains("Use operation \"merge\""));
        assert!(prompt.contains("Use operation \"split\""));
        assert!(prompt.contains("\"segment-a\""));
    }

    #[test]
    fn suspicious_normalization_segments_only_selects_boundary_candidates() {
        let raw_segments = vec![
            transcript_segment("segment-a", "microphone", "Me", 0, 4_000, "完整一句。"),
            transcript_segment("segment-b", "microphone", "Me", 5_000, 8_000, "这个是"),
            transcript_segment("segment-c", "microphone", "Me", 8_200, 12_000, "后半句。"),
            transcript_segment("segment-d", "system", "Others", 12_300, 16_000, "另一人。"),
        ];

        let candidates = suspicious_normalization_segments(&raw_segments);
        let ids = candidates
            .into_iter()
            .map(|segment| segment.id.as_str())
            .collect::<Vec<_>>();

        assert_eq!(ids, vec!["segment-b", "segment-c"]);
    }

    #[test]
    fn sanitize_transcript_normalization_rejects_cross_source_merge() {
        let raw_segments = vec![
            transcript_segment("segment-a", "microphone", "Me", 0, 4_000, "我先说这个"),
            transcript_segment("segment-b", "system", "Others", 4_000, 8_000, "对方回答"),
        ];
        let normalization = TranscriptNormalization {
            normalized_segments: vec![NormalizedTranscriptSegment {
                operation: "merge".to_string(),
                source_segment_ids: vec!["segment-a".to_string(), "segment-b".to_string()],
                source_kind: "microphone".to_string(),
                speaker_label: "Me".to_string(),
                start_ms: 0,
                end_ms: 8_000,
                text: "我先说这个 对方回答".to_string(),
                reason: "bad cross-source merge".to_string(),
            }],
        };

        let sanitized = sanitize_transcript_normalization(normalization, &raw_segments);

        assert_eq!(sanitized.normalized_segments.len(), 2);
        assert!(sanitized
            .normalized_segments
            .iter()
            .all(|segment| segment.operation == "keep"));
        assert_eq!(
            sanitized.normalized_segments[0].source_segment_ids,
            vec!["segment-a"]
        );
        assert_eq!(
            sanitized.normalized_segments[1].source_segment_ids,
            vec!["segment-b"]
        );
    }

    #[test]
    fn sanitize_transcript_normalization_rejects_non_adjacent_merge() {
        let raw_segments = vec![
            transcript_segment("segment-a", "microphone", "Me", 0, 4_000, "第一段"),
            transcript_segment("segment-b", "microphone", "Me", 4_000, 8_000, "中间段"),
            transcript_segment("segment-c", "microphone", "Me", 8_000, 12_000, "第三段"),
        ];
        let normalization = TranscriptNormalization {
            normalized_segments: vec![NormalizedTranscriptSegment {
                operation: "merge".to_string(),
                source_segment_ids: vec!["segment-a".to_string(), "segment-c".to_string()],
                source_kind: "microphone".to_string(),
                speaker_label: "Me".to_string(),
                start_ms: 0,
                end_ms: 12_000,
                text: "第一段 第三段".to_string(),
                reason: "bad non-adjacent merge".to_string(),
            }],
        };

        let sanitized = sanitize_transcript_normalization(normalization, &raw_segments);

        assert_eq!(sanitized.normalized_segments.len(), 3);
        assert!(sanitized
            .normalized_segments
            .iter()
            .all(|segment| segment.operation == "keep"));
    }

    #[test]
    fn sanitize_partial_transcript_normalization_replaces_only_candidate_segments() {
        let raw_segments = vec![
            transcript_segment("segment-a", "microphone", "Me", 0, 4_000, "完整一句。"),
            transcript_segment("segment-b", "microphone", "Me", 5_000, 8_000, "这个是"),
            transcript_segment("segment-c", "microphone", "Me", 8_200, 12_000, "后半句。"),
            transcript_segment("segment-d", "system", "Others", 12_300, 16_000, "另一人。"),
        ];
        let candidate_segments = vec![&raw_segments[1], &raw_segments[2]];
        let normalization = TranscriptNormalization {
            normalized_segments: vec![NormalizedTranscriptSegment {
                operation: "merge".to_string(),
                source_segment_ids: vec!["segment-b".to_string(), "segment-c".to_string()],
                source_kind: "microphone".to_string(),
                speaker_label: "Me".to_string(),
                start_ms: 5_000,
                end_ms: 12_000,
                text: "这个是后半句。".to_string(),
                reason: "unfinished boundary".to_string(),
            }],
        };

        let sanitized = sanitize_partial_transcript_normalization(
            normalization,
            &raw_segments,
            &candidate_segments,
        );

        assert_eq!(sanitized.normalized_segments.len(), 3);
        assert_eq!(sanitized.normalized_segments[0].text, "完整一句。");
        assert_eq!(sanitized.normalized_segments[1].text, "这个是后半句。");
        assert_eq!(
            sanitized.normalized_segments[1].source_segment_ids,
            vec!["segment-b", "segment-c"]
        );
        assert_eq!(sanitized.normalized_segments[2].text, "另一人。");
    }

    #[test]
    fn attach_transcript_normalization_preserves_summary_fields() {
        let normalization = TranscriptNormalization {
            normalized_segments: vec![NormalizedTranscriptSegment {
                operation: "split".to_string(),
                source_segment_ids: vec!["segment-a".to_string()],
                source_kind: "microphone".to_string(),
                speaker_label: "Me".to_string(),
                start_ms: 0,
                end_ms: 2_000,
                text: "第一句。".to_string(),
                reason: "source row contained multiple sentences".to_string(),
            }],
        };

        let raw = attach_transcript_normalization(
            r#"{"suggestedTitle":"Test","summaryOutline":[]}"#,
            &normalization,
        )
        .expect("attach normalization");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("parse attached JSON");

        assert_eq!(value["suggestedTitle"], "Test");
        assert_eq!(
            value["transcriptNormalization"]["normalizedSegments"][0]["operation"],
            "split"
        );
    }

    #[test]
    fn attach_reference_context_stores_metadata_only() {
        let metadata = ReferenceContextMetadata {
            mode: "codex-guided".to_string(),
            depth: "docs-and-matching-snippets".to_string(),
            budget: "balanced".to_string(),
            included_projects: vec![ReferenceProjectMetadata {
                id: "vis-app".to_string(),
                display_name: "VisApp".to_string(),
                match_level: "high".to_string(),
                reasons: vec!["alias: VisApp".to_string()],
                included_files: vec!["docs/SYSTEM_MAP.md".to_string()],
            }],
            skipped_projects: vec![SkippedReferenceProjectMetadata {
                id: "intranet".to_string(),
                display_name: "Lab Operation Intranet".to_string(),
                reason: "No strong transcript match".to_string(),
            }],
            truncated: false,
        };
        let context = ReferenceContextPack {
            prompt_section: "secret snippet that should not be stored".to_string(),
            metadata,
        };
        let raw = attach_reference_context(
            r#"{"suggestedTitle":"Test","summaryOutline":[]}"#,
            Some(&context),
        )
        .expect("attach reference context");
        let value: serde_json::Value = serde_json::from_str(&raw).expect("parse attached JSON");

        assert_eq!(
            value["referenceContext"]["includedProjects"][0]["id"],
            "vis-app"
        );
        assert_eq!(
            value["referenceContext"]["skippedProjects"][0]["id"],
            "intranet"
        );
        assert!(!raw.contains("secret snippet"));
    }

    #[test]
    fn reference_context_uses_fair_budget_for_selected_projects() {
        let root = std::env::temp_dir().join(format!(
            "note-taker-reference-test-{}",
            uuid::Uuid::new_v4()
        ));
        let access_dir = root.join("access-request-app");
        let vis_dir = root.join("vis-app");
        fs::create_dir_all(&access_dir).expect("create access dir");
        fs::create_dir_all(&vis_dir).expect("create vis dir");
        fs::write(
            access_dir.join("README.md"),
            format!(
                "# Access app\n\n{}",
                "access workflow grant revoke ".repeat(900)
            ),
        )
        .expect("write access readme");
        fs::write(
            vis_dir.join("README.md"),
            "# Vis App\n\nResearch group, department, people import, and PI binding UI.\n",
        )
        .expect("write vis readme");

        let options = SummaryRunOptions {
            context_budget: ReferenceContextBudget::Small,
            context_depth: ReferenceContextDepth::ProjectSummaries,
            include_git_changes: false,
            selected_project_ids: vec!["access".to_string(), "vis".to_string()],
            reference_projects: vec![
                ReferenceProject {
                    id: "access".to_string(),
                    display_name: "access app".to_string(),
                    path: access_dir.display().to_string(),
                    aliases: vec![],
                    default_selected: true,
                },
                ReferenceProject {
                    id: "vis".to_string(),
                    display_name: "vis app".to_string(),
                    path: vis_dir.display().to_string(),
                    aliases: vec!["research group".to_string(), "department".to_string()],
                    default_selected: true,
                },
            ],
            ..SummaryRunOptions::default()
        };

        let context = build_reference_context_pack(
            "Most of this meeting is about the vis app research group and department flows.",
            &options,
        )
        .expect("reference context");
        let included_ids = context
            .metadata
            .included_projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<Vec<_>>();

        assert!(included_ids.contains(&"access"));
        assert!(included_ids.contains(&"vis"));
        assert!(context.prompt_section.contains("Vis App"));
        assert_eq!(context.metadata.mode, "codex-guided");

        let _ = fs::remove_dir_all(root);
    }
}

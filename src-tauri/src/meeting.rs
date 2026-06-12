use crate::audio::{capture_spike, CaptureArtifact};
use crate::storage::{
    finish_meeting, get_app_settings, initialize_database, insert_audio_chunk, insert_audio_source,
    insert_meeting, insert_transcript_segment, list_audio_chunks_for_meeting,
    list_transcript_segments_for_meeting, reset_meeting_transcription,
    update_audio_chunk_transcription_status, update_meeting_status, NewAudioChunk, NewAudioSource,
    NewMeeting, NewTranscriptSegment, TranscriptSegmentRecord,
};
use crate::task_control::CancellationToken;
use chrono::{DateTime, Local, Utc};
use serde::Serialize;
use std::fs;
use std::path::Path;
use thiserror::Error;

const OPENAI_FALLBACK_LOCAL_MODEL: &str = "large-v3-turbo";

#[derive(Debug, Error)]
pub enum MeetingError {
    #[error("Audio capture error: {0}")]
    Audio(#[from] crate::audio::AudioError),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Sidecar transcription error: {0}")]
    Sidecar(#[from] crate::sidecar::SidecarError),
    #[error("OpenAI transcription error: {0}")]
    OpenAiTranscription(#[from] crate::openai_transcription::OpenAiTranscriptionError),
    #[error(
        "OpenAI transcription failed ({openai_error}); local fallback failed ({fallback_error})"
    )]
    OpenAiFallbackFailed {
        openai_error: String,
        fallback_error: String,
    },
    #[error("Smart chunk error: {0}")]
    SmartChunk(#[from] crate::smart_chunks::SmartChunkError),
    #[error("Task control error: {0}")]
    TaskControl(#[from] crate::task_control::TaskControlError),
    #[error("Task cancelled by user")]
    Cancelled,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkedMeetingResult {
    pub meeting_id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: String,
    pub status: String,
    pub chunk_seconds: u32,
    pub requested_seconds: u32,
    pub chunks: Vec<RecordedChunk>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordedChunk {
    pub id: String,
    pub source_kind: String,
    pub started_at_ms: i64,
    pub duration_ms: i64,
    pub path: String,
    pub status: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub rms: f32,
    pub non_zero_samples: usize,
    pub error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingTranscriptionResult {
    pub meeting_id: String,
    pub status: String,
    pub provider: String,
    pub processed_chunks: usize,
    pub transcribed_chunks: usize,
    pub empty_chunks: usize,
    pub failed_chunks: usize,
    pub segments: Vec<TranscriptSegmentResult>,
    pub failures: Vec<ChunkTranscriptionFailure>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegmentResult {
    pub id: String,
    pub chunk_id: String,
    pub source_kind: String,
    pub speaker_label: String,
    pub language: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub provider: String,
    pub output_json_path: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChunkTranscriptionFailure {
    pub chunk_id: String,
    pub source_kind: String,
    pub path: String,
    pub error: String,
}

pub fn record_chunked_demo(
    database_path: &Path,
    recordings_dir: &Path,
    requested_seconds: u32,
    chunk_seconds: u32,
) -> Result<ChunkedMeetingResult, MeetingError> {
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)?;
    }
    initialize_database(database_path)?;
    fs::create_dir_all(recordings_dir)?;

    let requested_seconds = requested_seconds.clamp(6, 14_400);
    let chunk_seconds = chunk_seconds.clamp(3, 60).min(requested_seconds);
    let meeting_id = uuid::Uuid::new_v4().to_string();
    let started_at = Utc::now();
    let local_started: DateTime<Local> = DateTime::from(started_at);
    let title = format!("Meeting {}", local_started.format("%Y-%m-%d %H:%M"));
    let meeting_dir = recordings_dir.join(&meeting_id);
    fs::create_dir_all(&meeting_dir)?;

    insert_meeting(
        database_path,
        &NewMeeting {
            id: &meeting_id,
            title: &title,
            title_source: "datetime_placeholder",
            started_at: &started_at.to_rfc3339(),
            status: "recording",
            language_hint: "auto",
            summary_language: "auto",
        },
    )?;

    let mut chunks = Vec::new();
    let mut offset_seconds = 0;
    while offset_seconds < requested_seconds {
        let seconds = chunk_seconds.min(requested_seconds - offset_seconds);
        let chunk_index = chunks.len() / 2;
        let chunk_dir = meeting_dir.join(format!("chunk-{chunk_index:04}"));
        fs::create_dir_all(&chunk_dir)?;
        let spike = capture_spike(&chunk_dir, seconds)?;
        persist_source_and_chunk(
            database_path,
            &meeting_id,
            "microphone",
            "Microphone",
            offset_seconds,
            &spike.mic,
            &mut chunks,
        )?;
        persist_source_and_chunk(
            database_path,
            &meeting_id,
            "system",
            "Computer audio",
            offset_seconds,
            &spike.system,
            &mut chunks,
        )?;
        offset_seconds += seconds;
    }

    let ended_at = Utc::now();
    finish_meeting(
        database_path,
        &meeting_id,
        &ended_at.to_rfc3339(),
        "recorded",
    )?;

    Ok(ChunkedMeetingResult {
        meeting_id,
        title,
        started_at: started_at.to_rfc3339(),
        ended_at: ended_at.to_rfc3339(),
        status: "recorded".to_string(),
        chunk_seconds,
        requested_seconds,
        chunks,
    })
}

pub fn transcribe_meeting_chunks(
    database_path: &Path,
    sidecar_dir: &Path,
    models_dir: &Path,
    transcriptions_dir: &Path,
    meeting_id: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<MeetingTranscriptionResult, MeetingError> {
    initialize_database(database_path)?;
    fs::create_dir_all(transcriptions_dir)?;
    update_meeting_status(database_path, meeting_id, "transcribing")?;
    update_progress(
        cancellation,
        "preparing",
        "Preparing transcription windows",
        0,
        None,
    )?;

    let settings = get_app_settings(database_path)?;
    let chunks = list_audio_chunks_for_meeting(database_path, meeting_id)?;
    let window_dir = transcriptions_dir.join("windows").join(meeting_id);
    let windows = crate::smart_chunks::build_transcription_windows(&chunks, &window_dir)?;
    let mut segments = Vec::new();
    let mut failures = Vec::new();
    let mut empty_chunks = 0;
    let mut processed_windows = 0;
    let mut cancelled = false;

    for chunk in &chunks {
        if chunk.status == "capture_failed" {
            failures.push(ChunkTranscriptionFailure {
                chunk_id: chunk.id.clone(),
                source_kind: chunk.source_kind.clone(),
                path: chunk.path.clone(),
                error: chunk
                    .transcription_error
                    .clone()
                    .unwrap_or_else(|| "capture failed before transcription".to_string()),
            });
            continue;
        }
    }

    for window in &windows {
        if is_cancelled(cancellation) {
            cancelled = true;
            break;
        }
        update_progress(
            cancellation,
            "transcribing",
            &format!(
                "Transcribing window {} of {}",
                processed_windows + 1,
                windows.len()
            ),
            processed_windows as u32,
            Some(windows.len() as u32),
        )?;
        update_window_chunk_status(database_path, window, "transcribing", None)?;
        match transcribe_one_window(
            database_path,
            sidecar_dir,
            models_dir,
            transcriptions_dir,
            window,
            &settings.transcription_provider,
            &settings.local_transcription_model,
            &settings.openai_transcription_model,
            &settings.language_hint,
            &settings.custom_glossary,
            cancellation,
        ) {
            Ok(segment_batch) if !segment_batch.is_empty() => {
                processed_windows += 1;
                update_progress(
                    cancellation,
                    "transcribing",
                    &format!(
                        "Transcribed {} of {} windows",
                        processed_windows,
                        windows.len()
                    ),
                    processed_windows as u32,
                    Some(windows.len() as u32),
                )?;
                segments.extend(segment_batch);
            }
            Ok(_) => {
                processed_windows += 1;
                empty_chunks += 1;
                update_progress(
                    cancellation,
                    "transcribing",
                    &format!(
                        "Processed {} of {} windows",
                        processed_windows,
                        windows.len()
                    ),
                    processed_windows as u32,
                    Some(windows.len() as u32),
                )?;
            }
            Err(MeetingError::Cancelled) => {
                update_window_chunk_status(database_path, window, "captured", None)?;
                cancelled = true;
                break;
            }
            Err(error) => {
                processed_windows += 1;
                update_progress(
                    cancellation,
                    "transcribing",
                    &format!(
                        "Processed {} of {} windows",
                        processed_windows,
                        windows.len()
                    ),
                    processed_windows as u32,
                    Some(windows.len() as u32),
                )?;
                let message = error.to_string();
                update_window_chunk_status(
                    database_path,
                    window,
                    "transcription_failed",
                    Some(&message),
                )?;
                failures.push(ChunkTranscriptionFailure {
                    chunk_id: window.id.clone(),
                    source_kind: window.source_kind.clone(),
                    path: window.path.clone(),
                    error: message,
                });
            }
        }
    }

    let status = if cancelled {
        update_progress(
            cancellation,
            "cancelled",
            "Transcription stopped",
            processed_windows as u32,
            Some(windows.len() as u32),
        )?;
        "transcription_cancelled"
    } else if failures.is_empty() {
        update_progress(
            cancellation,
            "complete",
            "Transcription complete",
            windows.len() as u32,
            Some(windows.len() as u32),
        )?;
        "transcribed"
    } else if segments.is_empty() && empty_chunks == 0 {
        "transcription_failed"
    } else {
        "partially_transcribed"
    };
    update_meeting_status(database_path, meeting_id, status)?;

    Ok(MeetingTranscriptionResult {
        meeting_id: meeting_id.to_string(),
        status: status.to_string(),
        provider: settings.transcription_provider,
        processed_chunks: processed_windows,
        transcribed_chunks: segments.len(),
        empty_chunks,
        failed_chunks: failures.len(),
        segments,
        failures,
    })
}

pub fn retranscribe_meeting_chunks(
    database_path: &Path,
    sidecar_dir: &Path,
    models_dir: &Path,
    transcriptions_dir: &Path,
    meeting_id: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<MeetingTranscriptionResult, MeetingError> {
    initialize_database(database_path)?;
    reset_meeting_transcription(database_path, meeting_id)?;
    transcribe_meeting_chunks(
        database_path,
        sidecar_dir,
        models_dir,
        transcriptions_dir,
        meeting_id,
        cancellation,
    )
}

fn transcribe_one_window(
    database_path: &Path,
    sidecar_dir: &Path,
    models_dir: &Path,
    transcriptions_dir: &Path,
    window: &crate::smart_chunks::TranscriptionWindow,
    transcription_provider: &str,
    local_model: &str,
    openai_model: &str,
    language_hint: &str,
    custom_glossary: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<Vec<TranscriptSegmentResult>, MeetingError> {
    if is_cancelled(cancellation) {
        return Err(MeetingError::Cancelled);
    }
    let end_ms = window.started_at_ms + window.duration_ms;
    let existing = existing_segments_for_window(database_path, window, end_ms)?;
    if !existing.is_empty() {
        update_window_chunk_status(database_path, window, "transcribed", None)?;
        return Ok(existing
            .into_iter()
            .map(|existing| TranscriptSegmentResult {
                id: existing.id,
                chunk_id: window.id.clone(),
                source_kind: existing.source_kind,
                speaker_label: existing.speaker_label,
                language: existing.language,
                start_ms: existing.start_ms,
                end_ms: existing.end_ms,
                text: existing.text,
                provider: existing.provider,
                output_json_path: String::new(),
            })
            .collect());
    }

    let output = match transcription_provider {
        "openai-api" => {
            if is_cancelled(cancellation) {
                return Err(MeetingError::Cancelled);
            }
            match crate::openai_transcription::transcribe_audio_file(
                transcriptions_dir,
                Path::new(&window.path),
                openai_model,
                language_hint,
                custom_glossary,
            ) {
                Ok(output) => TranscriptionOutput::from_openai(output),
                Err(openai_error) => {
                    if is_cancelled(cancellation) {
                        return Err(MeetingError::Cancelled);
                    }
                    transcribe_with_local_whisper(
                        sidecar_dir,
                        models_dir,
                        transcriptions_dir,
                        Path::new(&window.path),
                        language_hint,
                        OPENAI_FALLBACK_LOCAL_MODEL,
                        custom_glossary,
                        cancellation,
                    )
                    .map(|output| output.with_provider(openai_fallback_provider(openai_model)))
                    .map_err(|fallback_error| match fallback_error {
                        MeetingError::Cancelled => MeetingError::Cancelled,
                        other => MeetingError::OpenAiFallbackFailed {
                            openai_error: openai_error.to_string(),
                            fallback_error: other.to_string(),
                        },
                    })?
                }
            }
        }
        _ => transcribe_with_local_whisper(
            sidecar_dir,
            models_dir,
            transcriptions_dir,
            Path::new(&window.path),
            language_hint,
            local_model,
            custom_glossary,
            cancellation,
        )?,
    };
    if is_cancelled(cancellation) {
        return Err(MeetingError::Cancelled);
    }
    let normalized_text = crate::text_normalization::normalize_transcript_text(
        language_hint,
        output.transcript_text.trim(),
    );
    let text = normalized_text.trim();
    if text.is_empty() || is_likely_non_speech_hallucination(text) {
        update_window_chunk_status(database_path, window, "transcribed_empty", None)?;
        return Ok(Vec::new());
    }

    let speaker_label = speaker_label_for_source(&window.source_kind);
    let drafts = split_transcription_output(window, &output, language_hint);
    let mut inserted = Vec::new();
    for draft in drafts {
        let text = draft.text.trim();
        if text.is_empty() || is_likely_non_speech_hallucination(text) {
            continue;
        }
        let segment_id = uuid::Uuid::new_v4().to_string();
        insert_transcript_segment(
            database_path,
            &NewTranscriptSegment {
                id: &segment_id,
                meeting_id: &window.meeting_id,
                source_kind: &window.source_kind,
                speaker_label,
                language: "auto",
                start_ms: draft.start_ms,
                end_ms: draft.end_ms,
                text,
                confidence: None,
                provider: &output.provider,
            },
        )?;
        inserted.push(TranscriptSegmentResult {
            id: segment_id,
            chunk_id: window.id.clone(),
            source_kind: window.source_kind.clone(),
            speaker_label: speaker_label.to_string(),
            language: "auto".to_string(),
            start_ms: draft.start_ms,
            end_ms: draft.end_ms,
            text: text.to_string(),
            provider: output.provider.clone(),
            output_json_path: output.output_json_path.clone(),
        });
    }
    if inserted.is_empty() {
        update_window_chunk_status(database_path, window, "transcribed_empty", None)?;
        return Ok(Vec::new());
    }
    update_window_chunk_status(database_path, window, "transcribed", None)?;

    Ok(inserted)
}

fn transcribe_with_local_whisper(
    sidecar_dir: &Path,
    models_dir: &Path,
    transcriptions_dir: &Path,
    input_path: &Path,
    language_hint: &str,
    model_id: &str,
    custom_glossary: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<TranscriptionOutput, MeetingError> {
    let output = crate::sidecar::transcribe_smoke_with_language_model_glossary_and_cancel(
        sidecar_dir,
        models_dir,
        transcriptions_dir,
        input_path,
        language_hint,
        model_id,
        custom_glossary,
        cancellation,
    )
    .map_err(|error| match error {
        crate::sidecar::SidecarError::Cancelled => MeetingError::Cancelled,
        other => MeetingError::Sidecar(other),
    })?;
    Ok(TranscriptionOutput::from_sidecar(output))
}

fn openai_fallback_provider(openai_model: &str) -> String {
    format!("openai-api:{openai_model} fallback:local-whisper:{OPENAI_FALLBACK_LOCAL_MODEL}")
}

fn existing_segments_for_window(
    database_path: &Path,
    window: &crate::smart_chunks::TranscriptionWindow,
    window_end_ms: i64,
) -> Result<Vec<TranscriptSegmentRecord>, MeetingError> {
    let existing = list_transcript_segments_for_meeting(database_path, &window.meeting_id)?
        .into_iter()
        .filter(|segment| {
            segment.source_kind == window.source_kind
                && segment.start_ms >= window.started_at_ms
                && segment.end_ms <= window_end_ms
        })
        .collect::<Vec<_>>();
    Ok(existing)
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
) -> Result<(), MeetingError> {
    if let Some(token) = cancellation {
        token.update_progress(phase, message, current, total)?;
    }
    Ok(())
}

fn update_window_chunk_status(
    database_path: &Path,
    window: &crate::smart_chunks::TranscriptionWindow,
    status: &str,
    transcription_error: Option<&str>,
) -> Result<(), MeetingError> {
    for chunk_id in &window.chunk_ids {
        update_audio_chunk_transcription_status(
            database_path,
            chunk_id,
            status,
            transcription_error,
        )?;
    }
    Ok(())
}

struct TranscriptionOutput {
    provider: String,
    transcript_text: String,
    transcript_parts: Vec<ProviderTranscriptPart>,
    output_json_path: String,
}

impl TranscriptionOutput {
    fn from_sidecar(output: crate::sidecar::TranscriptionSmokeResult) -> Self {
        Self {
            provider: "local-whisper".to_string(),
            transcript_text: output.transcript_text,
            transcript_parts: output
                .transcript_parts
                .into_iter()
                .map(|part| ProviderTranscriptPart {
                    start_ms: part.start_ms,
                    end_ms: part.end_ms,
                    text: part.text,
                })
                .collect(),
            output_json_path: output.output_json_path,
        }
    }

    fn from_openai(output: crate::openai_transcription::OpenAiTranscriptionResult) -> Self {
        Self {
            provider: format!("openai-api:{}", output.model),
            transcript_text: output.transcript_text,
            transcript_parts: Vec::new(),
            output_json_path: output.output_json_path,
        }
    }

    fn with_provider(mut self, provider: String) -> Self {
        self.provider = provider;
        self
    }
}

#[derive(Debug, Clone)]
struct ProviderTranscriptPart {
    start_ms: i64,
    end_ms: i64,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SegmentDraft {
    start_ms: i64,
    end_ms: i64,
    text: String,
}

fn split_transcription_output(
    window: &crate::smart_chunks::TranscriptionWindow,
    output: &TranscriptionOutput,
    language_hint: &str,
) -> Vec<SegmentDraft> {
    let parts = if output.transcript_parts.is_empty() {
        vec![ProviderTranscriptPart {
            start_ms: 0,
            end_ms: window.duration_ms,
            text: output.transcript_text.clone(),
        }]
    } else {
        output.transcript_parts.clone()
    };

    let mut drafts = Vec::new();
    for part in parts {
        let normalized =
            crate::text_normalization::normalize_transcript_text(language_hint, &part.text);
        let text = normalized.trim();
        if text.is_empty() {
            continue;
        }
        let start_ms = window.started_at_ms + part.start_ms.clamp(0, window.duration_ms);
        let end_ms = window.started_at_ms + part.end_ms.clamp(0, window.duration_ms);
        if end_ms <= start_ms {
            continue;
        }
        drafts.extend(split_text_part(start_ms, end_ms, text));
    }
    drafts
}

fn split_text_part(start_ms: i64, end_ms: i64, text: &str) -> Vec<SegmentDraft> {
    const MAX_DISPLAY_SEGMENT_MS: i64 = 8_000;

    let sentence_parts = split_sentences(text);
    let total_chars = sentence_parts
        .iter()
        .map(|part| part.chars().count().max(1))
        .sum::<usize>()
        .max(1);
    let duration_ms = end_ms - start_ms;
    let mut cursor = start_ms;
    let mut drafts = Vec::new();

    for (index, sentence) in sentence_parts.iter().enumerate() {
        let chars = sentence.chars().count().max(1);
        let mut sentence_end = if index + 1 == sentence_parts.len() {
            end_ms
        } else {
            cursor + ((duration_ms as i128 * chars as i128) / total_chars as i128) as i64
        };
        if sentence_end <= cursor {
            sentence_end = (cursor + 1).min(end_ms);
        }
        drafts.extend(split_long_sentence(
            cursor,
            sentence_end,
            sentence,
            MAX_DISPLAY_SEGMENT_MS,
        ));
        cursor = sentence_end;
    }

    drafts
}

fn split_sentences(text: &str) -> Vec<String> {
    let mut sentences = Vec::new();
    let mut current = String::new();
    for character in text.chars() {
        current.push(character);
        if matches!(
            character,
            '。' | '！' | '？' | '；' | '.' | '!' | '?' | ';' | '\n'
        ) {
            push_sentence(&mut sentences, &mut current);
        }
    }
    push_sentence(&mut sentences, &mut current);
    if sentences.is_empty() {
        vec![text.trim().to_string()]
    } else {
        sentences
    }
}

fn push_sentence(sentences: &mut Vec<String>, current: &mut String) {
    let sentence = current.trim();
    if !sentence.is_empty() {
        sentences.push(sentence.to_string());
    }
    current.clear();
}

fn split_long_sentence(
    start_ms: i64,
    end_ms: i64,
    text: &str,
    max_segment_ms: i64,
) -> Vec<SegmentDraft> {
    let duration_ms = end_ms - start_ms;
    if duration_ms <= max_segment_ms {
        return vec![SegmentDraft {
            start_ms,
            end_ms,
            text: text.to_string(),
        }];
    }

    let parts = ((duration_ms + max_segment_ms - 1) / max_segment_ms).max(1) as usize;
    let chars = text.chars().collect::<Vec<_>>();
    let chars_per_part = (chars.len() + parts - 1) / parts;
    let mut drafts = Vec::new();
    let mut cursor = start_ms;
    for index in 0..parts {
        let char_start = index * chars_per_part;
        if char_start >= chars.len() {
            break;
        }
        let char_end = ((index + 1) * chars_per_part).min(chars.len());
        let segment_text = chars[char_start..char_end]
            .iter()
            .collect::<String>()
            .trim()
            .to_string();
        if segment_text.is_empty() {
            continue;
        }
        let segment_end = if index + 1 == parts {
            end_ms
        } else {
            start_ms + ((duration_ms as i128 * (index + 1) as i128) / parts as i128) as i64
        };
        drafts.push(SegmentDraft {
            start_ms: cursor,
            end_ms: segment_end.max(cursor + 1),
            text: segment_text,
        });
        cursor = segment_end;
    }
    drafts
}

fn speaker_label_for_source(source_kind: &str) -> &'static str {
    match source_kind {
        "microphone" => "Me",
        "system" => "Others",
        _ => "Unknown",
    }
}

fn is_likely_non_speech_hallucination(text: &str) -> bool {
    let compact = text.split_whitespace().collect::<String>();
    if compact.is_empty() {
        return true;
    }
    if compact.contains("字幕") && compact.contains("志愿者") {
        return true;
    }
    if compact.contains("谢谢观看") || compact.contains("感谢观看") {
        return true;
    }
    let phonetic_noise = compact
        .chars()
        .filter(|character| matches!(character, 'ɑ' | 'ː' | 'ə' | 'ʊ' | 'ɜ' | 'ɡ' | 'ɒ'))
        .count();
    phonetic_noise >= 4 && phonetic_noise * 2 >= compact.chars().count()
}

pub(crate) fn persist_source_and_chunk(
    database_path: &Path,
    meeting_id: &str,
    source_kind: &str,
    display_name: &str,
    offset_seconds: u32,
    artifact: &CaptureArtifact,
    chunks: &mut Vec<RecordedChunk>,
) -> Result<(), MeetingError> {
    let chunk_id = uuid::Uuid::new_v4().to_string();
    let source_id = format!("{meeting_id}-{source_kind}");
    let status = if artifact.error.is_some() {
        "capture_failed"
    } else {
        "captured"
    };
    let duration_ms = (artifact.duration_seconds.max(0.0) * 1000.0).round() as i64;
    let started_at_ms = i64::from(offset_seconds) * 1000;

    insert_audio_source(
        database_path,
        &NewAudioSource {
            id: &source_id,
            meeting_id,
            kind: source_kind,
            device_id: None,
            display_name,
            sample_rate: artifact.sample_rate,
            channels: artifact.channels,
        },
    )?;

    insert_audio_chunk(
        database_path,
        &NewAudioChunk {
            id: &chunk_id,
            meeting_id,
            source_kind,
            started_at_ms,
            duration_ms,
            path: &artifact.path,
            status,
            transcription_error: artifact.error.as_deref(),
        },
    )?;

    chunks.push(RecordedChunk {
        id: chunk_id,
        source_kind: source_kind.to_string(),
        started_at_ms,
        duration_ms,
        path: artifact.path.clone(),
        status: status.to_string(),
        sample_rate: artifact.sample_rate,
        channels: artifact.channels,
        rms: artifact.rms,
        non_zero_samples: artifact.non_zero_samples,
        error: artifact.error.clone(),
    });
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_window() -> crate::smart_chunks::TranscriptionWindow {
        crate::smart_chunks::TranscriptionWindow {
            id: "meeting-a:microphone:1000-21000".to_string(),
            meeting_id: "meeting-a".to_string(),
            source_kind: "microphone".to_string(),
            chunk_ids: vec!["chunk-a".to_string()],
            started_at_ms: 1_000,
            duration_ms: 20_000,
            path: "window.wav".to_string(),
        }
    }

    #[test]
    fn splits_timestamped_transcription_parts_into_sentence_segments() {
        let output = TranscriptionOutput {
            provider: "local-whisper".to_string(),
            transcript_text: "第一句。第二句？".to_string(),
            transcript_parts: vec![ProviderTranscriptPart {
                start_ms: 2_000,
                end_ms: 8_000,
                text: "第一句。第二句？".to_string(),
            }],
            output_json_path: "out.json".to_string(),
        };

        let segments = split_transcription_output(&test_window(), &output, "zh");

        assert_eq!(segments.len(), 2);
        assert_eq!(segments[0].start_ms, 3_000);
        assert_eq!(segments[1].end_ms, 9_000);
        assert_eq!(segments[0].text, "第一句。");
        assert_eq!(segments[1].text, "第二句？");
    }

    #[test]
    fn splits_long_unpunctuated_output_for_display() {
        let output = TranscriptionOutput {
            provider: "local-whisper".to_string(),
            transcript_text: "这是一个很长很长没有标点的测试文本用来模拟连续口语输出".to_string(),
            transcript_parts: vec![ProviderTranscriptPart {
                start_ms: 0,
                end_ms: 20_000,
                text: "这是一个很长很长没有标点的测试文本用来模拟连续口语输出".to_string(),
            }],
            output_json_path: "out.json".to_string(),
        };

        let segments = split_transcription_output(&test_window(), &output, "zh");

        assert!(segments.len() >= 3);
        assert!(segments
            .iter()
            .all(|segment| segment.end_ms - segment.start_ms <= 8_000));
        assert_eq!(
            segments.first().map(|segment| segment.start_ms),
            Some(1_000)
        );
        assert_eq!(segments.last().map(|segment| segment.end_ms), Some(21_000));
    }

    #[test]
    fn openai_fallback_provider_marks_fixed_local_model() {
        assert_eq!(
            openai_fallback_provider("gpt-4o-transcribe"),
            "openai-api:gpt-4o-transcribe fallback:local-whisper:large-v3-turbo"
        );
    }

    #[test]
    fn openai_fallback_failure_reports_both_causes() {
        let error = MeetingError::OpenAiFallbackFailed {
            openai_error: "OpenAI transcription failed with status 429: quota exceeded".to_string(),
            fallback_error: "Sidecar transcription error: large-v3-turbo is missing".to_string(),
        };
        let message = error.to_string();

        assert!(message.contains("quota exceeded"));
        assert!(message.contains("large-v3-turbo is missing"));
    }
}

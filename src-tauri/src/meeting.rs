use crate::audio::{capture_spike, CaptureArtifact};
use crate::storage::{
    find_transcript_segment_for_chunk, finish_meeting, get_app_settings, initialize_database,
    insert_audio_chunk, insert_audio_source, insert_meeting, insert_transcript_segment,
    list_audio_chunks_for_meeting, reset_meeting_transcription,
    update_audio_chunk_transcription_status, update_meeting_status, NewAudioChunk, NewAudioSource,
    NewMeeting, NewTranscriptSegment,
};
use chrono::{DateTime, Local, Utc};
use serde::Serialize;
use std::fs;
use std::path::Path;
use thiserror::Error;

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
    #[error("Smart chunk error: {0}")]
    SmartChunk(#[from] crate::smart_chunks::SmartChunkError),
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
) -> Result<MeetingTranscriptionResult, MeetingError> {
    initialize_database(database_path)?;
    fs::create_dir_all(transcriptions_dir)?;
    update_meeting_status(database_path, meeting_id, "transcribing")?;

    let settings = get_app_settings(database_path)?;
    let chunks = list_audio_chunks_for_meeting(database_path, meeting_id)?;
    let window_dir = transcriptions_dir.join("windows").join(meeting_id);
    let windows = crate::smart_chunks::build_transcription_windows(&chunks, &window_dir)?;
    let mut segments = Vec::new();
    let mut failures = Vec::new();
    let mut empty_chunks = 0;

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
        ) {
            Ok(Some(segment)) => segments.push(segment),
            Ok(None) => empty_chunks += 1,
            Err(error) => {
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

    let status = if failures.is_empty() {
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
        processed_chunks: windows.len(),
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
) -> Result<MeetingTranscriptionResult, MeetingError> {
    initialize_database(database_path)?;
    reset_meeting_transcription(database_path, meeting_id)?;
    transcribe_meeting_chunks(
        database_path,
        sidecar_dir,
        models_dir,
        transcriptions_dir,
        meeting_id,
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
) -> Result<Option<TranscriptSegmentResult>, MeetingError> {
    let end_ms = window.started_at_ms + window.duration_ms;
    if let Some(existing) = find_transcript_segment_for_chunk(
        database_path,
        &window.meeting_id,
        &window.source_kind,
        window.started_at_ms,
        end_ms,
    )? {
        update_window_chunk_status(database_path, window, "transcribed", None)?;
        return Ok(Some(TranscriptSegmentResult {
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
        }));
    }

    let output = match transcription_provider {
        "openai-api" => {
            TranscriptionOutput::from_openai(crate::openai_transcription::transcribe_audio_file(
                transcriptions_dir,
                Path::new(&window.path),
                openai_model,
                language_hint,
            )?)
        }
        _ => TranscriptionOutput::from_sidecar(
            crate::sidecar::transcribe_smoke_with_language_and_model(
                sidecar_dir,
                models_dir,
                transcriptions_dir,
                Path::new(&window.path),
                language_hint,
                local_model,
            )?,
        ),
    };
    let normalized_text = crate::text_normalization::normalize_transcript_text(
        language_hint,
        output.transcript_text.trim(),
    );
    let text = normalized_text.trim();
    if text.is_empty() || is_likely_non_speech_hallucination(text) {
        update_window_chunk_status(database_path, window, "transcribed_empty", None)?;
        return Ok(None);
    }

    let segment_id = uuid::Uuid::new_v4().to_string();
    let speaker_label = speaker_label_for_source(&window.source_kind);
    insert_transcript_segment(
        database_path,
        &NewTranscriptSegment {
            id: &segment_id,
            meeting_id: &window.meeting_id,
            source_kind: &window.source_kind,
            speaker_label,
            language: "auto",
            start_ms: window.started_at_ms,
            end_ms,
            text,
            confidence: None,
            provider: &output.provider,
        },
    )?;
    update_window_chunk_status(database_path, window, "transcribed", None)?;

    Ok(Some(TranscriptSegmentResult {
        id: segment_id,
        chunk_id: window.id.clone(),
        source_kind: window.source_kind.clone(),
        speaker_label: speaker_label.to_string(),
        language: "auto".to_string(),
        start_ms: window.started_at_ms,
        end_ms,
        text: text.to_string(),
        provider: output.provider,
        output_json_path: output.output_json_path,
    }))
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
    output_json_path: String,
}

impl TranscriptionOutput {
    fn from_sidecar(output: crate::sidecar::TranscriptionSmokeResult) -> Self {
        Self {
            provider: "local-whisper".to_string(),
            transcript_text: output.transcript_text,
            output_json_path: output.output_json_path,
        }
    }

    fn from_openai(output: crate::openai_transcription::OpenAiTranscriptionResult) -> Self {
        Self {
            provider: format!("openai-api:{}", output.model),
            transcript_text: output.transcript_text,
            output_json_path: output.output_json_path,
        }
    }
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

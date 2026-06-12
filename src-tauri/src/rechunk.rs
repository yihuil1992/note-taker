use crate::storage::{initialize_database, reset_meeting_transcription};
use chrono::Utc;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use rusqlite::{params, Connection};
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum RechunkError {
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("WAV error: {0}")]
    Wav(#[from] hound::Error),
    #[error("Meeting not found for selector: {0}")]
    MeetingNotFound(String),
    #[error("Meeting selector matched multiple meetings: {0}")]
    AmbiguousMeeting(String),
    #[error("No captured audio chunks found for meeting {0}")]
    NoCapturedChunks(String),
    #[error("Unsupported WAV format in {path}: {message}")]
    UnsupportedWav { path: String, message: String },
    #[error("Invalid chunk size: {0} seconds")]
    InvalidChunkSeconds(u32),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RechunkResult {
    pub meeting_id: String,
    pub meeting_title: String,
    pub chunk_seconds: u32,
    pub original_chunks: usize,
    pub generated_chunks: usize,
    pub skipped_chunks: usize,
    pub output_dir: String,
}

#[derive(Debug, Clone)]
struct MeetingRef {
    id: String,
    title: String,
}

#[derive(Debug, Clone)]
struct ExistingChunk {
    id: String,
    meeting_id: String,
    source_kind: String,
    started_at_ms: i64,
    path: String,
    status: String,
}

#[derive(Debug)]
struct GeneratedChunk {
    id: String,
    meeting_id: String,
    source_kind: String,
    started_at_ms: i64,
    duration_ms: i64,
    path: String,
}

pub fn rechunk_meeting_to_seconds(
    database_path: &Path,
    meeting_selector: &str,
    chunk_seconds: u32,
) -> Result<RechunkResult, RechunkError> {
    if !(1..=300).contains(&chunk_seconds) {
        return Err(RechunkError::InvalidChunkSeconds(chunk_seconds));
    }

    initialize_database(database_path)?;
    let connection = Connection::open(database_path)?;
    let meeting = resolve_meeting(&connection, meeting_selector)?;
    let chunks = list_existing_chunks(&connection, &meeting.id)?;
    let captured_chunks: Vec<_> = chunks
        .iter()
        .filter(|chunk| chunk.status != "capture_failed")
        .cloned()
        .collect();
    if captured_chunks.is_empty() {
        return Err(RechunkError::NoCapturedChunks(meeting.id));
    }

    let output_dir = output_directory(database_path, &meeting.id, chunk_seconds)?;
    fs::create_dir_all(&output_dir)?;

    let mut generated_chunks = Vec::new();
    for chunk in &captured_chunks {
        generated_chunks.extend(split_chunk(chunk, chunk_seconds, &output_dir)?);
    }

    let mut connection = Connection::open(database_path)?;
    let transaction = connection.transaction()?;
    transaction.execute(
        "DELETE FROM audio_chunks WHERE meeting_id = ?1",
        params![meeting.id],
    )?;
    for chunk in &generated_chunks {
        transaction.execute(
            r#"
            INSERT INTO audio_chunks (
                id, meeting_id, source_kind, started_at_ms, duration_ms, path, status, transcription_error
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'captured', NULL)
            "#,
            params![
                chunk.id,
                chunk.meeting_id,
                chunk.source_kind,
                chunk.started_at_ms,
                chunk.duration_ms,
                chunk.path
            ],
        )?;
    }
    transaction.commit()?;

    reset_meeting_transcription(database_path, &meeting.id)?;

    Ok(RechunkResult {
        meeting_id: meeting.id,
        meeting_title: meeting.title,
        chunk_seconds,
        original_chunks: chunks.len(),
        generated_chunks: generated_chunks.len(),
        skipped_chunks: chunks.len().saturating_sub(captured_chunks.len()),
        output_dir: output_dir.display().to_string(),
    })
}

fn resolve_meeting(connection: &Connection, selector: &str) -> Result<MeetingRef, RechunkError> {
    let selector = selector.trim();
    let exact = query_meetings(
        connection,
        "SELECT id, title FROM meetings WHERE id = ?1 OR title = ?1 ORDER BY started_at DESC",
        selector,
    )?;
    if let Some(meeting) = single_meeting(selector, exact)? {
        return Ok(meeting);
    }

    let like = format!("%{selector}%");
    let fuzzy = query_meetings(
        connection,
        "SELECT id, title FROM meetings WHERE title LIKE ?1 ORDER BY started_at DESC",
        &like,
    )?;
    single_meeting(selector, fuzzy)?
        .ok_or_else(|| RechunkError::MeetingNotFound(selector.to_string()))
}

fn query_meetings(
    connection: &Connection,
    sql: &str,
    selector: &str,
) -> Result<Vec<MeetingRef>, RechunkError> {
    let mut statement = connection.prepare(sql)?;
    let rows = statement.query_map(params![selector], |row| {
        Ok(MeetingRef {
            id: row.get(0)?,
            title: row.get(1)?,
        })
    })?;
    let mut meetings = Vec::new();
    for row in rows {
        meetings.push(row?);
    }
    Ok(meetings)
}

fn single_meeting(
    selector: &str,
    meetings: Vec<MeetingRef>,
) -> Result<Option<MeetingRef>, RechunkError> {
    match meetings.len() {
        0 => Ok(None),
        1 => Ok(meetings.into_iter().next()),
        _ => {
            let preview = meetings
                .iter()
                .take(5)
                .map(|meeting| format!("{} ({})", meeting.title, meeting.id))
                .collect::<Vec<_>>()
                .join(", ");
            Err(RechunkError::AmbiguousMeeting(format!(
                "{selector}: {preview}"
            )))
        }
    }
}

fn list_existing_chunks(
    connection: &Connection,
    meeting_id: &str,
) -> Result<Vec<ExistingChunk>, RechunkError> {
    let mut statement = connection.prepare(
        r#"
        SELECT id, meeting_id, source_kind, started_at_ms, path, status
        FROM audio_chunks
        WHERE meeting_id = ?1
        ORDER BY started_at_ms ASC, source_kind ASC
        "#,
    )?;
    let rows = statement.query_map(params![meeting_id], |row| {
        Ok(ExistingChunk {
            id: row.get(0)?,
            meeting_id: row.get(1)?,
            source_kind: row.get(2)?,
            started_at_ms: row.get(3)?,
            path: row.get(4)?,
            status: row.get(5)?,
        })
    })?;
    let mut chunks = Vec::new();
    for row in rows {
        chunks.push(row?);
    }
    Ok(chunks)
}

fn output_directory(
    database_path: &Path,
    meeting_id: &str,
    chunk_seconds: u32,
) -> Result<PathBuf, RechunkError> {
    let app_data_dir = database_path.parent().unwrap_or_else(|| Path::new("."));
    let run_id = format!("{}-{}", Utc::now().format("%Y%m%d%H%M%S"), Uuid::new_v4());
    Ok(app_data_dir
        .join("recordings")
        .join(meeting_id)
        .join(format!("rechunk-{chunk_seconds}s"))
        .join(run_id))
}

fn split_chunk(
    chunk: &ExistingChunk,
    chunk_seconds: u32,
    output_dir: &Path,
) -> Result<Vec<GeneratedChunk>, RechunkError> {
    let path = PathBuf::from(&chunk.path);
    let mut reader = WavReader::open(&path)?;
    let spec = reader.spec();
    validate_spec(&path, spec)?;
    let samples = reader
        .samples::<i16>()
        .collect::<Result<Vec<_>, hound::Error>>()?;
    let channels = usize::from(spec.channels);
    let total_frames = samples.len() / channels;
    let frames_per_chunk = usize::try_from(u64::from(chunk_seconds) * u64::from(spec.sample_rate))
        .unwrap_or(usize::MAX)
        .max(1);
    let mut generated = Vec::new();
    let mut start_frame = 0_usize;
    let mut part_index = 0_usize;

    while start_frame < total_frames {
        let end_frame = (start_frame + frames_per_chunk).min(total_frames);
        let started_at_ms = chunk.started_at_ms + frames_to_ms(start_frame, spec);
        let duration_ms = frames_to_ms(end_frame.saturating_sub(start_frame), spec);
        if duration_ms <= 0 {
            break;
        }

        let id = format!("{}-rechunk-{part_index:04}", chunk.id);
        let source = chunk.source_kind.replace(['\\', '/', ':'], "-");
        let output_path = output_dir.join(format!(
            "{}-{}-{}-{:04}.wav",
            source, chunk.started_at_ms, chunk.id, part_index
        ));
        write_samples(&samples, spec, start_frame, end_frame, &output_path)?;
        generated.push(GeneratedChunk {
            id,
            meeting_id: chunk.meeting_id.clone(),
            source_kind: chunk.source_kind.clone(),
            started_at_ms,
            duration_ms,
            path: output_path.display().to_string(),
        });

        start_frame = end_frame;
        part_index += 1;
    }

    Ok(generated)
}

fn write_samples(
    samples: &[i16],
    spec: WavSpec,
    start_frame: usize,
    end_frame: usize,
    output_path: &Path,
) -> Result<(), RechunkError> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let channels = usize::from(spec.channels);
    let start_sample = start_frame * channels;
    let end_sample = end_frame * channels;
    let mut writer = WavWriter::create(output_path, spec)?;
    for sample in &samples[start_sample..end_sample] {
        writer.write_sample(*sample)?;
    }
    writer.finalize()?;
    Ok(())
}

fn validate_spec(path: &Path, spec: WavSpec) -> Result<(), RechunkError> {
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err(RechunkError::UnsupportedWav {
            path: path.display().to_string(),
            message: "missing channels or sample rate".to_string(),
        });
    }
    if spec.sample_format != SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(RechunkError::UnsupportedWav {
            path: path.display().to_string(),
            message: format!(
                "expected 16-bit PCM, got {:?} {} bits",
                spec.sample_format, spec.bits_per_sample
            ),
        });
    }
    Ok(())
}

fn frames_to_ms(frames: usize, spec: WavSpec) -> i64 {
    ((frames as u128 * 1000) / u128::from(spec.sample_rate)) as i64
}

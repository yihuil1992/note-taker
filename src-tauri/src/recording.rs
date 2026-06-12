use crate::audio::capture_spike;
use crate::meeting::{persist_source_and_chunk, RecordedChunk};
use crate::storage::{finish_meeting, initialize_database, insert_meeting, NewMeeting};
use chrono::{DateTime, Local, Utc};
use serde::Serialize;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use thiserror::Error;

#[derive(Default)]
pub struct RecordingManager {
    active: Mutex<Option<ActiveRecording>>,
}

struct ActiveRecording {
    meeting_id: String,
    title: String,
    started_at: String,
    chunk_seconds: u32,
    stop_requested: Arc<AtomicBool>,
    captured_chunks: Arc<AtomicU32>,
    handle: Option<JoinHandle<Result<RecordingStopResult, RecordingError>>>,
}

#[derive(Debug, Error)]
pub enum RecordingError {
    #[error("A recording is already running")]
    AlreadyRunning,
    #[error("No recording is running")]
    NotRunning,
    #[error("Recording worker failed: {0}")]
    Worker(String),
    #[error("Database error: {0}")]
    Database(#[from] rusqlite::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Audio capture error: {0}")]
    Audio(#[from] crate::audio::AudioError),
    #[error("Meeting persistence error: {0}")]
    Meeting(#[from] crate::meeting::MeetingError),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActiveRecordingStatus {
    pub meeting_id: String,
    pub title: String,
    pub started_at: String,
    pub chunk_seconds: u32,
    pub captured_chunks: u32,
    pub stop_requested: bool,
    pub worker_finished: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RecordingStopResult {
    pub meeting_id: String,
    pub title: String,
    pub started_at: String,
    pub ended_at: String,
    pub status: String,
    pub captured_chunks: u32,
}

impl RecordingManager {
    pub fn start(
        &self,
        database_path: PathBuf,
        recordings_dir: PathBuf,
        requested_seconds: u32,
        chunk_seconds: u32,
    ) -> Result<ActiveRecordingStatus, RecordingError> {
        let mut guard = self
            .active
            .lock()
            .map_err(|error| RecordingError::Worker(error.to_string()))?;
        clear_finished_locked(&mut guard);
        if guard.is_some() {
            return Err(RecordingError::AlreadyRunning);
        }

        fs::create_dir_all(&recordings_dir)?;
        initialize_database(&database_path)?;

        let requested_seconds = requested_seconds.clamp(6, 14_400);
        let chunk_seconds = chunk_seconds.clamp(3, 60).min(requested_seconds);
        let meeting_id = uuid::Uuid::new_v4().to_string();
        let started_at = Utc::now();
        let local_started: DateTime<Local> = DateTime::from(started_at);
        let title = format!("Meeting {}", local_started.format("%Y-%m-%d %H:%M"));
        let meeting_dir = recordings_dir.join(&meeting_id);
        fs::create_dir_all(&meeting_dir)?;

        insert_meeting(
            &database_path,
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

        let stop_requested = Arc::new(AtomicBool::new(false));
        let captured_chunks = Arc::new(AtomicU32::new(0));
        let worker_stop = Arc::clone(&stop_requested);
        let worker_chunks = Arc::clone(&captured_chunks);
        let worker_meeting_id = meeting_id.clone();
        let worker_title = title.clone();
        let worker_started_at = started_at.to_rfc3339();
        let handle = thread::spawn(move || {
            run_recording_worker(
                database_path,
                meeting_dir,
                worker_meeting_id,
                worker_title,
                worker_started_at,
                requested_seconds,
                chunk_seconds,
                worker_stop,
                worker_chunks,
            )
        });

        let status = ActiveRecordingStatus {
            meeting_id: meeting_id.clone(),
            title: title.clone(),
            started_at: started_at.to_rfc3339(),
            chunk_seconds,
            captured_chunks: 0,
            stop_requested: false,
            worker_finished: false,
        };
        *guard = Some(ActiveRecording {
            meeting_id,
            title,
            started_at: started_at.to_rfc3339(),
            chunk_seconds,
            stop_requested,
            captured_chunks,
            handle: Some(handle),
        });
        Ok(status)
    }

    pub fn status(&self) -> Result<Option<ActiveRecordingStatus>, RecordingError> {
        let guard = self
            .active
            .lock()
            .map_err(|error| RecordingError::Worker(error.to_string()))?;
        Ok(guard.as_ref().map(ActiveRecording::status))
    }

    pub fn stop(&self) -> Result<RecordingStopResult, RecordingError> {
        let mut guard = self
            .active
            .lock()
            .map_err(|error| RecordingError::Worker(error.to_string()))?;
        let Some(mut active) = guard.take() else {
            return Err(RecordingError::NotRunning);
        };
        active.stop_requested.store(true, Ordering::SeqCst);
        let Some(handle) = active.handle.take() else {
            return Err(RecordingError::NotRunning);
        };
        handle
            .join()
            .map_err(|_| RecordingError::Worker("recording thread panicked".to_string()))?
    }
}

impl ActiveRecording {
    fn status(&self) -> ActiveRecordingStatus {
        ActiveRecordingStatus {
            meeting_id: self.meeting_id.clone(),
            title: self.title.clone(),
            started_at: self.started_at.clone(),
            chunk_seconds: self.chunk_seconds,
            captured_chunks: self.captured_chunks.load(Ordering::SeqCst),
            stop_requested: self.stop_requested.load(Ordering::SeqCst),
            worker_finished: self
                .handle
                .as_ref()
                .map(JoinHandle::is_finished)
                .unwrap_or(true),
        }
    }
}

fn clear_finished_locked(active: &mut Option<ActiveRecording>) {
    let should_clear = active
        .as_ref()
        .and_then(|recording| recording.handle.as_ref())
        .map(JoinHandle::is_finished)
        .unwrap_or(false);
    if !should_clear {
        return;
    }
    if let Some(mut recording) = active.take() {
        if let Some(handle) = recording.handle.take() {
            let _ = handle.join();
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn run_recording_worker(
    database_path: PathBuf,
    meeting_dir: PathBuf,
    meeting_id: String,
    title: String,
    started_at: String,
    requested_seconds: u32,
    chunk_seconds: u32,
    stop_requested: Arc<AtomicBool>,
    captured_chunks: Arc<AtomicU32>,
) -> Result<RecordingStopResult, RecordingError> {
    let mut chunks = Vec::<RecordedChunk>::new();
    let mut offset_seconds = 0;
    while offset_seconds < requested_seconds && !stop_requested.load(Ordering::SeqCst) {
        let seconds = chunk_seconds.min(requested_seconds - offset_seconds);
        let chunk_index = chunks.len() / 2;
        let chunk_dir = meeting_dir.join(format!("chunk-{chunk_index:04}"));
        fs::create_dir_all(&chunk_dir)?;
        let spike = capture_spike(&chunk_dir, seconds)?;
        persist_source_and_chunk(
            &database_path,
            &meeting_id,
            "microphone",
            "Microphone",
            offset_seconds,
            &spike.mic,
            &mut chunks,
        )?;
        persist_source_and_chunk(
            &database_path,
            &meeting_id,
            "system",
            "Computer audio",
            offset_seconds,
            &spike.system,
            &mut chunks,
        )?;
        captured_chunks.store(chunks.len() as u32, Ordering::SeqCst);
        offset_seconds += seconds;
    }

    let ended_at = Utc::now().to_rfc3339();
    finish_meeting(&database_path, &meeting_id, &ended_at, "recorded")?;
    Ok(RecordingStopResult {
        meeting_id,
        title,
        started_at,
        ended_at,
        status: "recorded".to_string(),
        captured_chunks: chunks.len() as u32,
    })
}

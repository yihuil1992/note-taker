pub mod audio;
pub mod exports;
pub mod meeting;
pub mod openai_transcription;
pub mod process;
pub mod rechunk;
pub mod recording;
pub mod sidecar;
pub mod smart_chunks;
pub mod storage;
pub mod summary;
pub mod text_normalization;

use audio::{capture_spike, list_devices, AudioDevice, SpikeResult};
use exports::{export_meeting_json, export_meeting_markdown, ExportResult};
use meeting::{
    record_chunked_demo, retranscribe_meeting_chunks, transcribe_meeting_chunks,
    ChunkedMeetingResult, MeetingTranscriptionResult,
};
use recording::{ActiveRecordingStatus, RecordingManager, RecordingStopResult};
use serde::Serialize;
use sidecar::{
    check_runtime, download_default_runtime, download_model, get_status, transcribe_smoke,
    ModelDownloadResult, RuntimeDownloadResult, SidecarRuntimeCheck, SidecarStatus,
    TranscriptionSmokeResult,
};
use std::fs;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use storage::{
    archive_meeting as archive_meeting_record, get_app_settings as load_app_settings,
    get_meeting_detail as load_meeting_detail,
    initialize_database, list_recent_meetings, search_meetings as search_meeting_records,
    set_app_setting, AppSettingsRecord, MeetingDetailRecord, MeetingListItem,
};
use summary::{summarize_meeting_with_codex, MeetingSummaryResult};
use tauri::Manager;

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AppStatus {
    app_data_dir: String,
    database_path: String,
    recordings_dir: String,
    sidecar_dir: String,
    models_dir: String,
    transcriptions_dir: String,
    summaries_dir: String,
    exports_dir: String,
    sidecar_configured: bool,
    default_model: String,
    raw_audio_retention_days: u8,
    sidecar: SidecarStatus,
    settings: AppSettingsRecord,
}

#[tauri::command]
fn get_app_status(app: tauri::AppHandle) -> Result<AppStatus, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    initialize_database(&paths.database_path).map_err(|error| error.to_string())?;

    let sidecar =
        get_status(&paths.sidecar_dir, &paths.models_dir).map_err(|error| error.to_string())?;
    let settings = load_app_settings(&paths.database_path).map_err(|error| error.to_string())?;

    Ok(AppStatus {
        app_data_dir: display_path(&paths.app_data_dir),
        database_path: display_path(&paths.database_path),
        recordings_dir: display_path(&paths.recordings_dir),
        sidecar_dir: display_path(&paths.sidecar_dir),
        models_dir: display_path(&paths.models_dir),
        transcriptions_dir: display_path(&paths.transcriptions_dir),
        summaries_dir: display_path(&paths.summaries_dir),
        exports_dir: display_path(&paths.exports_dir),
        sidecar_configured: sidecar.ready,
        default_model: sidecar.model.id.clone(),
        raw_audio_retention_days: settings.raw_audio_retention_days,
        sidecar,
        settings,
    })
}

#[tauri::command]
fn list_audio_devices() -> Result<Vec<AudioDevice>, String> {
    list_devices().map_err(|error| error.to_string())
}

#[tauri::command]
async fn run_audio_spike(app: tauri::AppHandle, seconds: u32) -> Result<SpikeResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    initialize_database(&paths.database_path).map_err(|error| error.to_string())?;

    let seconds = seconds.clamp(3, 30);
    let output_dir = paths
        .recordings_dir
        .join(format!("spike-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&output_dir).map_err(|error| error.to_string())?;

    tauri::async_runtime::spawn_blocking(move || capture_spike(&output_dir, seconds))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

#[tauri::command]
async fn record_chunked_meeting_demo(
    app: tauri::AppHandle,
    requested_seconds: u32,
    chunk_seconds: u32,
) -> Result<ChunkedMeetingResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    let database_path = paths.database_path;
    let recordings_dir = paths.recordings_dir;
    tauri::async_runtime::spawn_blocking(move || {
        record_chunked_demo(
            &database_path,
            &recordings_dir,
            requested_seconds,
            chunk_seconds,
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn start_recording(
    app: tauri::AppHandle,
    manager: tauri::State<'_, RecordingManager>,
    requested_seconds: u32,
    chunk_seconds: u32,
) -> Result<ActiveRecordingStatus, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    manager
        .start(
            paths.database_path,
            paths.recordings_dir,
            requested_seconds,
            chunk_seconds,
        )
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_active_recording(
    manager: tauri::State<'_, RecordingManager>,
) -> Result<Option<ActiveRecordingStatus>, String> {
    manager.status().map_err(|error| error.to_string())
}

#[tauri::command]
fn stop_recording(
    manager: tauri::State<'_, RecordingManager>,
) -> Result<RecordingStopResult, String> {
    manager.stop().map_err(|error| error.to_string())
}

#[tauri::command]
async fn transcribe_meeting_demo(
    app: tauri::AppHandle,
    meeting_id: String,
) -> Result<MeetingTranscriptionResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    let database_path = paths.database_path;
    let sidecar_dir = paths.sidecar_dir;
    let models_dir = paths.models_dir;
    let transcriptions_dir = paths.transcriptions_dir;
    tauri::async_runtime::spawn_blocking(move || {
        transcribe_meeting_chunks(
            &database_path,
            &sidecar_dir,
            &models_dir,
            &transcriptions_dir,
            &meeting_id,
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn retranscribe_meeting_demo(
    app: tauri::AppHandle,
    meeting_id: String,
) -> Result<MeetingTranscriptionResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    let database_path = paths.database_path;
    let sidecar_dir = paths.sidecar_dir;
    let models_dir = paths.models_dir;
    let transcriptions_dir = paths.transcriptions_dir;
    tauri::async_runtime::spawn_blocking(move || {
        retranscribe_meeting_chunks(
            &database_path,
            &sidecar_dir,
            &models_dir,
            &transcriptions_dir,
            &meeting_id,
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn summarize_meeting_demo(
    app: tauri::AppHandle,
    meeting_id: String,
) -> Result<MeetingSummaryResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    let database_path = paths.database_path;
    let summaries_dir = paths.summaries_dir;
    tauri::async_runtime::spawn_blocking(move || {
        summarize_meeting_with_codex(&database_path, &summaries_dir, &meeting_id)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn list_meetings(
    app: tauri::AppHandle,
    limit: Option<usize>,
) -> Result<Vec<MeetingListItem>, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    initialize_database(&paths.database_path).map_err(|error| error.to_string())?;
    list_recent_meetings(&paths.database_path, limit.unwrap_or(50))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_meeting_detail(
    app: tauri::AppHandle,
    meeting_id: String,
) -> Result<Option<MeetingDetailRecord>, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    initialize_database(&paths.database_path).map_err(|error| error.to_string())?;
    load_meeting_detail(&paths.database_path, &meeting_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn search_meetings(
    app: tauri::AppHandle,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<MeetingListItem>, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    initialize_database(&paths.database_path).map_err(|error| error.to_string())?;
    search_meeting_records(&paths.database_path, &query, limit.unwrap_or(50))
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn archive_meeting(app: tauri::AppHandle, meeting_id: String) -> Result<(), String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    initialize_database(&paths.database_path).map_err(|error| error.to_string())?;
    archive_meeting_record(&paths.database_path, &meeting_id).map_err(|error| error.to_string())
}

#[tauri::command]
fn export_meeting_as_markdown(
    app: tauri::AppHandle,
    meeting_id: String,
) -> Result<ExportResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    export_meeting_markdown(&paths.database_path, &paths.exports_dir, &meeting_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn export_meeting_as_json(
    app: tauri::AppHandle,
    meeting_id: String,
) -> Result<ExportResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    export_meeting_json(&paths.database_path, &paths.exports_dir, &meeting_id)
        .map_err(|error| error.to_string())
}

#[tauri::command]
fn get_app_settings(app: tauri::AppHandle) -> Result<AppSettingsRecord, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    initialize_database(&paths.database_path).map_err(|error| error.to_string())?;
    load_app_settings(&paths.database_path).map_err(|error| error.to_string())
}

#[tauri::command]
fn update_app_setting(
    app: tauri::AppHandle,
    key: String,
    value: String,
) -> Result<AppSettingsRecord, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    initialize_database(&paths.database_path).map_err(|error| error.to_string())?;
    validate_setting(&key, &value)?;
    set_app_setting(&paths.database_path, &key, &value).map_err(|error| error.to_string())?;
    load_app_settings(&paths.database_path).map_err(|error| error.to_string())
}

#[tauri::command]
async fn download_default_transcription_model(
    app: tauri::AppHandle,
) -> Result<ModelDownloadResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    let settings = load_app_settings(&paths.database_path).map_err(|error| error.to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        download_model(&paths.models_dir, &settings.local_transcription_model)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn download_default_sidecar_runtime(
    app: tauri::AppHandle,
) -> Result<RuntimeDownloadResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    tauri::async_runtime::spawn_blocking(move || {
        download_default_runtime(&paths.sidecar_dir, &paths.models_dir)
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
async fn run_sidecar_transcription_smoke(
    app: tauri::AppHandle,
    input_path: String,
) -> Result<TranscriptionSmokeResult, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    tauri::async_runtime::spawn_blocking(move || {
        transcribe_smoke(
            &paths.sidecar_dir,
            &paths.models_dir,
            &paths.transcriptions_dir,
            PathBuf::from(input_path).as_path(),
        )
    })
    .await
    .map_err(|error| error.to_string())?
    .map_err(|error| error.to_string())
}

#[tauri::command]
fn open_sidecar_folder(app: tauri::AppHandle) -> Result<(), String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    open_folder(&paths.sidecar_dir)
}

#[tauri::command]
fn open_exports_folder(app: tauri::AppHandle) -> Result<(), String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    open_folder(&paths.exports_dir)
}

#[tauri::command]
fn check_sidecar_runtime(app: tauri::AppHandle) -> Result<SidecarRuntimeCheck, String> {
    let paths = AppPaths::resolve(&app)?;
    paths.ensure()?;
    check_runtime(&paths.sidecar_dir).map_err(|error| error.to_string())
}

struct AppPaths {
    app_data_dir: PathBuf,
    database_path: PathBuf,
    recordings_dir: PathBuf,
    sidecar_dir: PathBuf,
    models_dir: PathBuf,
    transcriptions_dir: PathBuf,
    summaries_dir: PathBuf,
    exports_dir: PathBuf,
}

impl AppPaths {
    fn resolve(app: &tauri::AppHandle) -> Result<Self, String> {
        let app_data_dir = app
            .path()
            .app_data_dir()
            .map_err(|error| format!("Failed to resolve app data directory: {error}"))?;
        Ok(Self {
            database_path: app_data_dir.join("note-taker.sqlite3"),
            recordings_dir: app_data_dir.join("recordings"),
            sidecar_dir: app_data_dir.join("sidecars"),
            models_dir: app_data_dir.join("models"),
            transcriptions_dir: app_data_dir.join("transcriptions"),
            summaries_dir: app_data_dir.join("summaries"),
            exports_dir: app_data_dir.join("exports"),
            app_data_dir,
        })
    }

    fn ensure(&self) -> Result<(), String> {
        for path in [
            &self.app_data_dir,
            &self.recordings_dir,
            &self.sidecar_dir,
            &self.models_dir,
            &self.transcriptions_dir,
            &self.summaries_dir,
            &self.exports_dir,
        ] {
            fs::create_dir_all(path)
                .map_err(|error| format!("Failed to create {}: {error}", display_path(path)))?;
        }
        Ok(())
    }
}

fn validate_setting(key: &str, value: &str) -> Result<(), String> {
    let valid = match key {
        "raw_audio_retention_days" => matches!(value, "0" | "7" | "30" | "365"),
        "transcription_provider" => matches!(value, "local-whisper" | "openai-api"),
        "summary_provider" => matches!(value, "codex-cli" | "openai-api" | "local-llm"),
        "local_transcription_model" => matches!(value, "large-v3-turbo" | "large-v3"),
        "openai_transcription_model" => matches!(
            value,
            "gpt-4o-mini-transcribe" | "gpt-4o-transcribe" | "whisper-1"
        ),
        "language_hint" => matches!(value, "auto" | "zh" | "ja" | "en"),
        "summary_language" => matches!(value, "auto" | "zh" | "ja" | "en"),
        "recording_consent_reminder_dismissed" => matches!(value, "true" | "false"),
        _ => false,
    };
    if valid {
        Ok(())
    } else {
        Err(format!("Unsupported setting {key}={value}"))
    }
}

fn display_path(path: &PathBuf) -> String {
    path.display().to_string()
}

fn open_folder(path: &PathBuf) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        ProcessCommand::new("explorer.exe")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Failed to open {}: {error}", display_path(path)))?;
        return Ok(());
    }

    #[cfg(target_os = "macos")]
    {
        ProcessCommand::new("open")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Failed to open {}: {error}", display_path(path)))?;
        return Ok(());
    }

    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    {
        ProcessCommand::new("xdg-open")
            .arg(path)
            .spawn()
            .map_err(|error| format!("Failed to open {}: {error}", display_path(path)))?;
        Ok(())
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(RecordingManager::default())
        .invoke_handler(tauri::generate_handler![
            get_app_status,
            list_audio_devices,
            run_audio_spike,
            record_chunked_meeting_demo,
            start_recording,
            get_active_recording,
            stop_recording,
            transcribe_meeting_demo,
            retranscribe_meeting_demo,
            summarize_meeting_demo,
            list_meetings,
            get_meeting_detail,
            search_meetings,
            archive_meeting,
            export_meeting_as_markdown,
            export_meeting_as_json,
            get_app_settings,
            update_app_setting,
            download_default_transcription_model,
            download_default_sidecar_runtime,
            run_sidecar_transcription_smoke,
            open_sidecar_folder,
            open_exports_folder,
            check_sidecar_runtime
        ])
        .run(tauri::generate_context!())
        .expect("error while running Note Taker");
}

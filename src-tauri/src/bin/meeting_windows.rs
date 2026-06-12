use note_taker_lib::smart_chunks::build_transcription_windows;
use note_taker_lib::storage::{initialize_database, list_audio_chunks_for_meeting};
use serde::Serialize;
use std::env;
use std::path::PathBuf;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct WindowPreview {
    meeting_id: String,
    windows: usize,
    microphone_windows: usize,
    system_windows: usize,
    min_ms: i64,
    max_ms: i64,
    average_ms: i64,
}

fn main() {
    let meeting_id = match env::args().nth(1) {
        Some(value) => value,
        None => {
            eprintln!("Usage: pnpm meeting:windows <meeting-id> [app-data-dir]");
            std::process::exit(2);
        }
    };
    let app_data_dir = env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("meeting-demo"));
    let database_path = app_data_dir.join("note-taker.sqlite3");
    let output_dir = app_data_dir
        .join("transcriptions")
        .join("windows")
        .join(&meeting_id);

    let result = initialize_database(&database_path)
        .and_then(|_| list_audio_chunks_for_meeting(&database_path, &meeting_id))
        .map_err(|error| error.to_string())
        .and_then(|chunks| {
            build_transcription_windows(&chunks, &output_dir).map_err(|error| error.to_string())
        });

    match result {
        Ok(windows) => {
            let durations = windows
                .iter()
                .map(|window| window.duration_ms)
                .collect::<Vec<_>>();
            let total: i64 = durations.iter().sum();
            let preview = WindowPreview {
                meeting_id,
                windows: windows.len(),
                microphone_windows: windows
                    .iter()
                    .filter(|window| window.source_kind == "microphone")
                    .count(),
                system_windows: windows
                    .iter()
                    .filter(|window| window.source_kind == "system")
                    .count(),
                min_ms: durations.iter().copied().min().unwrap_or(0),
                max_ms: durations.iter().copied().max().unwrap_or(0),
                average_ms: if windows.is_empty() {
                    0
                } else {
                    total / windows.len() as i64
                },
            };
            println!(
                "{}",
                serde_json::to_string_pretty(&preview).expect("serialize window preview")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

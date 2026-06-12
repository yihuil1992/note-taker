use note_taker_lib::meeting::transcribe_meeting_chunks;
use std::env;
use std::path::PathBuf;

fn main() {
    let meeting_id = match env::args().nth(1) {
        Some(value) => value,
        None => {
            eprintln!("Usage: pnpm meeting:transcribe <meeting-id> [app-data-dir]");
            std::process::exit(2);
        }
    };
    let app_data_dir = env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("meeting-demo"));
    let database_path = app_data_dir.join("note-taker.sqlite3");
    let sidecar_dir = app_data_dir.join("sidecars");
    let models_dir = app_data_dir.join("models");
    let transcriptions_dir = app_data_dir.join("transcriptions");

    match transcribe_meeting_chunks(
        &database_path,
        &sidecar_dir,
        &models_dir,
        &transcriptions_dir,
        &meeting_id,
    ) {
        Ok(result) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result)
                    .expect("serialize meeting transcription result")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

use note_taker_lib::sidecar::transcribe_smoke;
use std::path::PathBuf;

fn main() {
    let input_path = match std::env::args().nth(1) {
        Some(value) => PathBuf::from(value),
        None => {
            eprintln!("Usage: pnpm transcribe:smoke <input-wav> [app-data-dir]");
            std::process::exit(2);
        }
    };
    let app_data_dir = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("sidecar-smoke"));
    let sidecar_dir = app_data_dir.join("sidecars");
    let models_dir = app_data_dir.join("models");
    let output_dir = app_data_dir.join("transcriptions");

    match transcribe_smoke(&sidecar_dir, &models_dir, &output_dir, &input_path) {
        Ok(result) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).expect("serialize transcription result")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

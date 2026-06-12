use note_taker_lib::rechunk::rechunk_meeting_to_seconds;
use std::env;
use std::path::PathBuf;

fn main() {
    let meeting_selector = match env::args().nth(1) {
        Some(value) => value,
        None => {
            eprintln!("Usage: pnpm meeting:rechunk <meeting-id-or-title> [app-data-dir] [seconds]");
            std::process::exit(2);
        }
    };
    let app_data_dir = env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(default_app_data_dir);
    let chunk_seconds = env::args()
        .nth(3)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(10);
    let database_path = app_data_dir.join("note-taker.sqlite3");

    match rechunk_meeting_to_seconds(&database_path, &meeting_selector, chunk_seconds) {
        Ok(result) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).expect("serialize meeting rechunk result")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

fn default_app_data_dir() -> PathBuf {
    env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("meeting-demo"))
        .join("com.yihui.notetaker")
}

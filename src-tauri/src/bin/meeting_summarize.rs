use note_taker_lib::summary::summarize_meeting_with_codex;
use std::env;
use std::path::PathBuf;

fn main() {
    let meeting_id = match env::args().nth(1) {
        Some(value) => value,
        None => {
            eprintln!("Usage: pnpm meeting:summarize <meeting-id> [app-data-dir]");
            std::process::exit(2);
        }
    };
    let app_data_dir = env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("meeting-demo"));
    let database_path = app_data_dir.join("note-taker.sqlite3");
    let summaries_dir = app_data_dir.join("summaries");

    match summarize_meeting_with_codex(&database_path, &summaries_dir, &meeting_id) {
        Ok(result) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).expect("serialize meeting summary result")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

use note_taker_lib::meeting::record_chunked_demo;
use std::env;
use std::path::PathBuf;

fn main() {
    let requested_seconds = env::args()
        .nth(1)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(6);
    let chunk_seconds = env::args()
        .nth(2)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(3);
    let root = env::args()
        .nth(3)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("meeting-demo"));
    let database_path = root.join("note-taker.sqlite3");
    let recordings_dir = root.join("recordings");

    match record_chunked_demo(
        &database_path,
        &recordings_dir,
        requested_seconds,
        chunk_seconds,
    ) {
        Ok(result) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).expect("serialize meeting demo result")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

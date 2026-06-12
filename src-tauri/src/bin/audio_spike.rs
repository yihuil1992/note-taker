use note_taker_lib::audio::capture_spike;
use std::path::PathBuf;

fn main() {
    let seconds = std::env::args()
        .nth(1)
        .and_then(|value| value.parse::<u32>().ok())
        .unwrap_or(10)
        .clamp(3, 30);
    let output_dir = std::env::args()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("audio-spike"));

    match capture_spike(&output_dir, seconds) {
        Ok(result) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).expect("serialize spike result")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

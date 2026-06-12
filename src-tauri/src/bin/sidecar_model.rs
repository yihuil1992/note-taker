use note_taker_lib::sidecar::download_default_model;
use std::env;
use std::path::PathBuf;

fn main() {
    let root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("sidecar-runtime"));
    let models_dir = root.join("models");

    match download_default_model(&models_dir) {
        Ok(result) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).expect("serialize model download result")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

use note_taker_lib::sidecar::download_default_runtime;
use std::env;
use std::path::PathBuf;

fn main() {
    let root = env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("target").join("sidecar-runtime"));
    let sidecar_dir = root.join("sidecars");
    let models_dir = root.join("models");

    match download_default_runtime(&sidecar_dir, &models_dir) {
        Ok(result) => {
            println!(
                "{}",
                serde_json::to_string_pretty(&result).expect("serialize runtime install result")
            );
        }
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(1);
        }
    }
}

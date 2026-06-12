use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

const TRANSCRIPTIONS_URL: &str = "https://api.openai.com/v1/audio/transcriptions";
const DEFAULT_TRANSCRIPTION_MODEL: &str = "gpt-4o-mini-transcribe";

#[derive(Debug, Error)]
pub enum OpenAiTranscriptionError {
    #[error("OPENAI_API_KEY is not set. Set it before using OpenAI API transcription.")]
    MissingApiKey,
    #[error("Input audio file does not exist: {0}")]
    MissingInput(String),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("HTTP error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("OpenAI transcription failed with status {status}: {body}")]
    Api { status: u16, body: String },
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug)]
pub struct OpenAiTranscriptionResult {
    pub model: String,
    pub transcript_text: String,
    pub output_json_path: String,
}

#[derive(Debug, Deserialize)]
struct TranscriptionResponse {
    text: Option<String>,
}

pub fn default_model() -> &'static str {
    DEFAULT_TRANSCRIPTION_MODEL
}

pub fn transcribe_audio_file(
    output_dir: &Path,
    input_path: &Path,
    model: &str,
    language_hint: &str,
) -> Result<OpenAiTranscriptionResult, OpenAiTranscriptionError> {
    if !input_path.exists() {
        return Err(OpenAiTranscriptionError::MissingInput(
            input_path.display().to_string(),
        ));
    }
    let api_key =
        std::env::var("OPENAI_API_KEY").map_err(|_| OpenAiTranscriptionError::MissingApiKey)?;
    fs::create_dir_all(output_dir)?;

    let model = normalize_model(model);
    let mut form = reqwest::blocking::multipart::Form::new()
        .file("file", input_path)?
        .text("model", model.to_string())
        .text("response_format", "json");

    if let Some(language) = normalize_language_hint(language_hint) {
        form = form.text("language", language.to_string());
    }
    if let Some(prompt) = initial_prompt_for_language(language_hint) {
        form = form.text("prompt", prompt.to_string());
    }

    let response = reqwest::blocking::Client::new()
        .post(TRANSCRIPTIONS_URL)
        .bearer_auth(api_key)
        .multipart(form)
        .send()?;
    let status = response.status();
    let body = response.text()?;
    if !status.is_success() {
        return Err(OpenAiTranscriptionError::Api {
            status: status.as_u16(),
            body,
        });
    }

    let output_json_path = output_path(output_dir, input_path);
    fs::write(&output_json_path, &body)?;
    let parsed: TranscriptionResponse = serde_json::from_str(&body)?;
    let transcript_text = parsed.text.unwrap_or_default();

    Ok(OpenAiTranscriptionResult {
        model: model.to_string(),
        transcript_text,
        output_json_path: output_json_path.display().to_string(),
    })
}

fn normalize_model(model: &str) -> &'static str {
    match model {
        "gpt-4o-transcribe" => "gpt-4o-transcribe",
        "whisper-1" => "whisper-1",
        _ => DEFAULT_TRANSCRIPTION_MODEL,
    }
}

fn normalize_language_hint(language_hint: &str) -> Option<&'static str> {
    match language_hint {
        "zh" | "zh-CN" | "Chinese" | "chinese" => Some("zh"),
        "ja" | "Japanese" | "japanese" => Some("ja"),
        "en" | "English" | "english" => Some("en"),
        _ => None,
    }
}

fn initial_prompt_for_language(language_hint: &str) -> Option<&'static str> {
    match normalize_language_hint(language_hint) {
        Some("zh") => {
            Some("以下是普通话会议转录。请使用简体中文输出，不要使用繁体中文。内容包含口语、产品讨论、颜色和形状描述。")
        }
        Some("ja") => Some("以下は日本語の会議文字起こしです。"),
        Some("en") => Some("The following is an English meeting transcript."),
        _ => None,
    }
}

fn output_path(output_dir: &Path, input_path: &Path) -> PathBuf {
    let stem = input_path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("audio");
    output_dir.join(format!(
        "openai-transcript-{stem}-{}.json",
        uuid::Uuid::new_v4()
    ))
}

#[cfg(test)]
mod tests {
    use super::{default_model, normalize_model};

    #[test]
    fn defaults_to_mini_transcribe_for_unknown_models() {
        assert_eq!(default_model(), "gpt-4o-mini-transcribe");
        assert_eq!(normalize_model("not-a-model"), "gpt-4o-mini-transcribe");
        assert_eq!(normalize_model("gpt-4o-transcribe"), "gpt-4o-transcribe");
    }
}

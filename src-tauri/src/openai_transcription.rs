use serde::Deserialize;
use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use thiserror::Error;

const TRANSCRIPTIONS_URL: &str = "https://api.openai.com/v1/audio/transcriptions";
const DEFAULT_TRANSCRIPTION_MODEL: &str = "gpt-4o-transcribe";
const MAX_TRANSCRIPTION_ATTEMPTS: usize = 3;

#[derive(Debug, Error)]
pub enum OpenAiTranscriptionError {
    #[error("OPENAI_API_KEY is not set. Set it before using OpenAI API transcription.")]
    MissingApiKey,
    #[error("OpenAI credential error: {0}")]
    Credential(#[from] crate::openai_credentials::OpenAiCredentialError),
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
    custom_glossary: &str,
) -> Result<OpenAiTranscriptionResult, OpenAiTranscriptionError> {
    if !input_path.exists() {
        return Err(OpenAiTranscriptionError::MissingInput(
            input_path.display().to_string(),
        ));
    }
    let api_key = crate::openai_credentials::load_api_key()?
        .ok_or(OpenAiTranscriptionError::MissingApiKey)?;
    fs::create_dir_all(output_dir)?;

    let model = normalize_model(model);
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(300))
        .build()?;

    let mut final_error: Option<OpenAiTranscriptionError> = None;
    for attempt in 0..MAX_TRANSCRIPTION_ATTEMPTS {
        let form = build_transcription_form(input_path, model, language_hint, custom_glossary)?;
        let response = match client
            .post(TRANSCRIPTIONS_URL)
            .bearer_auth(&api_key)
            .multipart(form)
            .send()
        {
            Ok(response) => response,
            Err(error)
                if is_retryable_http_error(&error) && attempt + 1 < MAX_TRANSCRIPTION_ATTEMPTS =>
            {
                sleep_before_retry(attempt);
                continue;
            }
            Err(error) => return Err(OpenAiTranscriptionError::Http(error)),
        };
        let status = response.status();
        let body = response.text()?;
        if status.is_success() {
            let output_json_path = output_path(output_dir, input_path);
            fs::write(&output_json_path, &body)?;
            let parsed: TranscriptionResponse = serde_json::from_str(&body)?;
            let transcript_text = parsed.text.unwrap_or_default();

            return Ok(OpenAiTranscriptionResult {
                model: model.to_string(),
                transcript_text,
                output_json_path: output_json_path.display().to_string(),
            });
        }

        let api_error = OpenAiTranscriptionError::Api {
            status: status.as_u16(),
            body,
        };
        if is_retryable_status(status.as_u16()) && attempt + 1 < MAX_TRANSCRIPTION_ATTEMPTS {
            final_error = Some(api_error);
            sleep_before_retry(attempt);
            continue;
        }
        return Err(api_error);
    }

    Err(
        final_error.unwrap_or_else(|| OpenAiTranscriptionError::Api {
            status: 0,
            body: "OpenAI transcription failed after retries".to_string(),
        }),
    )
}

fn build_transcription_form(
    input_path: &Path,
    model: &str,
    language_hint: &str,
    custom_glossary: &str,
) -> Result<reqwest::blocking::multipart::Form, OpenAiTranscriptionError> {
    let mut form = reqwest::blocking::multipart::Form::new()
        .file("file", input_path)?
        .text("model", model.to_string())
        .text("response_format", "json");

    if use_server_chunking(model) {
        form = form.text("chunking_strategy", "auto");
    }
    if let Some(language) = normalize_language_hint(language_hint) {
        form = form.text("language", language.to_string());
    }
    if let Some(prompt) = transcription_prompt(language_hint, custom_glossary) {
        form = form.text("prompt", prompt);
    }
    Ok(form)
}

fn use_server_chunking(model: &str) -> bool {
    matches!(model, "gpt-4o-transcribe" | "gpt-4o-mini-transcribe")
}

fn is_retryable_status(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

fn is_retryable_http_error(error: &reqwest::Error) -> bool {
    error.is_connect() || error.is_timeout()
}

fn sleep_before_retry(attempt: usize) {
    let millis = match attempt {
        0 => 600,
        1 => 1_500,
        _ => 3_000,
    };
    thread::sleep(Duration::from_millis(millis));
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

fn transcription_prompt(language_hint: &str, custom_glossary: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(prompt) = initial_prompt_for_language(language_hint) {
        parts.push(prompt.to_string());
    }

    let glossary = custom_glossary.trim();
    if !glossary.is_empty() {
        parts.push(format!(
            "可能出现的专有名词、缩写或内部术语:\n{glossary}\n如果音频中出现相近发音，请优先使用以上写法；不要凭空加入未听到的词。"
        ));
    }

    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n\n"))
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
    use super::{
        default_model, is_retryable_status, normalize_model, transcription_prompt,
        use_server_chunking,
    };

    #[test]
    fn defaults_to_quality_transcribe_for_unknown_models() {
        assert_eq!(default_model(), "gpt-4o-transcribe");
        assert_eq!(normalize_model("not-a-model"), "gpt-4o-transcribe");
        assert_eq!(normalize_model("gpt-4o-transcribe"), "gpt-4o-transcribe");
    }

    #[test]
    fn uses_server_chunking_for_4o_transcription_models() {
        assert!(use_server_chunking("gpt-4o-transcribe"));
        assert!(use_server_chunking("gpt-4o-mini-transcribe"));
        assert!(!use_server_chunking("whisper-1"));
    }

    #[test]
    fn retries_rate_limit_and_server_errors() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(503));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
    }

    #[test]
    fn transcription_prompt_includes_custom_glossary() {
        let prompt = transcription_prompt("zh", "RAG: 检索增强生成\nNote Taker")
            .expect("prompt with glossary");

        assert!(prompt.contains("普通话会议转录"));
        assert!(prompt.contains("RAG: 检索增强生成"));
        assert!(prompt.contains("不要凭空加入"));
    }

    #[test]
    fn transcription_prompt_stays_empty_without_language_or_glossary() {
        assert!(transcription_prompt("auto", "").is_none());
    }
}

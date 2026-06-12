use crate::task_control::CancellationToken;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::fs;
use std::fs::File;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::thread;
use std::time::Duration;
use thiserror::Error;
use zip::ZipArchive;

const DEFAULT_MODEL_ID: &str = "large-v3-turbo";
const MODEL_LARGE_V3_TURBO: ModelDefinition = ModelDefinition {
    id: "large-v3-turbo",
    file_name: "ggml-large-v3-turbo.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3-turbo.bin",
    sha256: "1fc70f774d38eb169993ac391eea357ef47c88757ef72ee5943879b7e8e2bc69",
    bytes: 1_624_555_275,
};
const MODEL_LARGE_V3: ModelDefinition = ModelDefinition {
    id: "large-v3",
    file_name: "ggml-large-v3.bin",
    url: "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-large-v3.bin",
    sha256: "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2",
    bytes: 3_095_033_483,
};
const DEFAULT_RUNTIME_VERSION: &str = "v1.8.6";
const DEFAULT_RUNTIME_FILE: &str = "whisper-bin-x64.zip";
const DEFAULT_RUNTIME_URL: &str =
    "https://github.com/ggml-org/whisper.cpp/releases/download/v1.8.6/whisper-bin-x64.zip";
const DEFAULT_RUNTIME_SHA256: &str =
    "b07ea0b1b4115a38e1a7b07debf581f0b77d999925f8acb8f39d322b0ba0a822";
const DEFAULT_RUNTIME_BYTES: u64 = 4_093_849;

#[derive(Debug, Clone, Copy)]
struct ModelDefinition {
    id: &'static str,
    file_name: &'static str,
    url: &'static str,
    sha256: &'static str,
    bytes: u64,
}

#[derive(Debug, Error)]
pub enum SidecarError {
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Download error: {0}")]
    Download(#[from] reqwest::Error),
    #[error("Archive error: {0}")]
    Archive(#[from] zip::result::ZipError),
    #[error("Checksum mismatch for {path}: expected {expected}, got {actual}")]
    Checksum {
        path: String,
        expected: String,
        actual: String,
    },
    #[error("Invalid sidecar archive: {0}")]
    InvalidArchive(String),
    #[error("Sidecar is not ready: {0}")]
    NotReady(String),
    #[error("Input audio file does not exist: {0}")]
    MissingInput(String),
    #[error("whisper-cli failed with exit code {code:?}: {stderr}")]
    CommandFailed { code: Option<i32>, stderr: String },
    #[error("Task cancelled by user")]
    Cancelled,
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarRuntimeCheck {
    pub executable_path: String,
    pub executable_exists: bool,
    pub ok: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub message: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SidecarStatus {
    pub executable_path: String,
    pub executable_exists: bool,
    pub model: ModelStatus,
    pub ready: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelStatus {
    pub id: String,
    pub file_name: String,
    pub path: String,
    pub url: String,
    pub expected_sha256: String,
    pub expected_bytes: u64,
    pub exists: bool,
    pub actual_bytes: Option<u64>,
    pub actual_sha256: Option<String>,
    pub verified: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelDownloadResult {
    pub model: ModelStatus,
    pub downloaded: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDownloadResult {
    pub status: SidecarStatus,
    pub downloaded: bool,
    pub version: String,
    pub file_name: String,
    pub url: String,
    pub expected_sha256: String,
    pub actual_sha256: String,
    pub expected_bytes: u64,
    pub installed_files: Vec<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionSmokeResult {
    pub input_path: String,
    pub output_json_path: String,
    pub output_prefix: String,
    pub transcript_text: String,
    pub transcript_parts: Vec<TranscriptPart>,
    pub stdout: String,
    pub stderr: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptPart {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

pub fn get_status(sidecar_dir: &Path, models_dir: &Path) -> Result<SidecarStatus, SidecarError> {
    fs::create_dir_all(sidecar_dir)?;
    fs::create_dir_all(models_dir)?;
    let executable_path = sidecar_dir.join(executable_name());
    let model = model_status(models_dir, DEFAULT_MODEL_ID)?;
    let executable_exists = executable_path.exists();
    let ready = executable_exists && model.verified;
    Ok(SidecarStatus {
        executable_path: executable_path.display().to_string(),
        executable_exists,
        model,
        ready,
    })
}

pub fn check_runtime(sidecar_dir: &Path) -> Result<SidecarRuntimeCheck, SidecarError> {
    fs::create_dir_all(sidecar_dir)?;
    let executable_path = sidecar_dir.join(executable_name());
    let executable_exists = executable_path.exists();
    if !executable_exists {
        return Ok(SidecarRuntimeCheck {
            executable_path: executable_path.display().to_string(),
            executable_exists,
            ok: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            message: "whisper-cli executable is missing".to_string(),
        });
    }

    let mut command = Command::new(&executable_path);
    command.arg("-h");
    crate::process::suppress_console_window(&mut command);
    let output = command.output()?;
    let stdout = truncate_output(String::from_utf8_lossy(&output.stdout).to_string());
    let stderr = truncate_output(String::from_utf8_lossy(&output.stderr).to_string());
    let combined = format!("{stdout}\n{stderr}").to_lowercase();
    let ok = output.status.success() || combined.contains("usage") || combined.contains("whisper");
    let message = if ok {
        "whisper-cli started and returned help output".to_string()
    } else {
        "whisper-cli launched but did not return recognizable help output".to_string()
    };

    Ok(SidecarRuntimeCheck {
        executable_path: executable_path.display().to_string(),
        executable_exists,
        ok,
        exit_code: output.status.code(),
        stdout,
        stderr,
        message,
    })
}

pub fn transcribe_smoke(
    sidecar_dir: &Path,
    models_dir: &Path,
    output_dir: &Path,
    input_path: &Path,
) -> Result<TranscriptionSmokeResult, SidecarError> {
    transcribe_smoke_with_language(sidecar_dir, models_dir, output_dir, input_path, "auto")
}

pub fn transcribe_smoke_with_language(
    sidecar_dir: &Path,
    models_dir: &Path,
    output_dir: &Path,
    input_path: &Path,
    language_hint: &str,
) -> Result<TranscriptionSmokeResult, SidecarError> {
    transcribe_smoke_with_language_and_model(
        sidecar_dir,
        models_dir,
        output_dir,
        input_path,
        language_hint,
        DEFAULT_MODEL_ID,
    )
}

pub fn transcribe_smoke_with_language_and_model(
    sidecar_dir: &Path,
    models_dir: &Path,
    output_dir: &Path,
    input_path: &Path,
    language_hint: &str,
    model_id: &str,
) -> Result<TranscriptionSmokeResult, SidecarError> {
    transcribe_smoke_with_language_model_and_glossary(
        sidecar_dir,
        models_dir,
        output_dir,
        input_path,
        language_hint,
        model_id,
        "",
    )
}

pub fn transcribe_smoke_with_language_model_and_glossary(
    sidecar_dir: &Path,
    models_dir: &Path,
    output_dir: &Path,
    input_path: &Path,
    language_hint: &str,
    model_id: &str,
    custom_glossary: &str,
) -> Result<TranscriptionSmokeResult, SidecarError> {
    transcribe_smoke_with_language_model_glossary_and_cancel(
        sidecar_dir,
        models_dir,
        output_dir,
        input_path,
        language_hint,
        model_id,
        custom_glossary,
        None,
    )
}

pub fn transcribe_smoke_with_language_model_glossary_and_cancel(
    sidecar_dir: &Path,
    models_dir: &Path,
    output_dir: &Path,
    input_path: &Path,
    language_hint: &str,
    model_id: &str,
    custom_glossary: &str,
    cancellation: Option<&CancellationToken>,
) -> Result<TranscriptionSmokeResult, SidecarError> {
    fs::create_dir_all(sidecar_dir)?;
    fs::create_dir_all(models_dir)?;
    let executable_path = sidecar_dir.join(executable_name());
    let model = model_status(models_dir, model_id)?;
    if !input_path.exists() {
        return Err(SidecarError::MissingInput(input_path.display().to_string()));
    }
    if !model.verified {
        return Err(SidecarError::NotReady(format!(
            "{} is missing or failed checksum verification",
            model.id
        )));
    }
    if !executable_path.exists() {
        return Err(SidecarError::NotReady(format!(
            "{} is missing",
            executable_path.display()
        )));
    }

    fs::create_dir_all(output_dir)?;
    let output_prefix = output_dir.join(format!("transcript-{}", uuid::Uuid::new_v4()));
    let model_path = Path::new(&model.path);
    let language = normalize_language_hint(language_hint);
    let mut command = Command::new(&executable_path);
    command
        .arg("-m")
        .arg(model_path)
        .arg("-f")
        .arg(input_path)
        .arg("-l")
        .arg(language)
        .arg("--suppress-nst")
        .arg("-oj")
        .arg("-of")
        .arg(&output_prefix);
    if let Some(prompt) = transcription_prompt(language, custom_glossary) {
        command
            .arg("--prompt")
            .arg(&prompt)
            .arg("--carry-initial-prompt");
    }
    crate::process::suppress_console_window(&mut command);
    let output = run_transcription_command(command, cancellation)?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    if !output.status.success() {
        return Err(SidecarError::CommandFailed {
            code: output.status.code(),
            stderr,
        });
    }

    let output_json_path = output_prefix.with_extension("json");
    let (transcript_text, transcript_parts) = if output_json_path.exists() {
        extract_transcript_output(&fs::read_to_string(&output_json_path)?)?
    } else {
        (stdout.trim().to_string(), Vec::new())
    };

    Ok(TranscriptionSmokeResult {
        input_path: input_path.display().to_string(),
        output_json_path: output_json_path.display().to_string(),
        output_prefix: output_prefix.display().to_string(),
        transcript_text,
        transcript_parts,
        stdout,
        stderr,
    })
}

fn run_transcription_command(
    mut command: Command,
    cancellation: Option<&CancellationToken>,
) -> Result<std::process::Output, SidecarError> {
    command.stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = command.spawn()?;
    loop {
        if cancellation
            .map(CancellationToken::is_cancelled)
            .unwrap_or(false)
        {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SidecarError::Cancelled);
        }
        if child.try_wait()?.is_some() {
            return child.wait_with_output().map_err(SidecarError::Io);
        }
        thread::sleep(Duration::from_millis(120));
    }
}

fn normalize_language_hint(language_hint: &str) -> &'static str {
    match language_hint {
        "zh" | "zh-CN" | "Chinese" | "chinese" => "zh",
        "ja" | "Japanese" | "japanese" => "ja",
        "en" | "English" | "english" => "en",
        _ => "auto",
    }
}

fn initial_prompt_for_language(language: &str) -> Option<&'static str> {
    match language {
        "zh" => Some("以下是普通话会议转录。请使用简体中文输出，不要使用繁体中文。内容包含口语、产品讨论、颜色和形状描述。"),
        "ja" => Some("以下は日本語の会議文字起こしです。"),
        "en" => Some("The following is an English meeting transcript."),
        _ => None,
    }
}

fn transcription_prompt(language: &str, custom_glossary: &str) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(prompt) = initial_prompt_for_language(language) {
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

pub fn download_default_runtime(
    sidecar_dir: &Path,
    models_dir: &Path,
) -> Result<RuntimeDownloadResult, SidecarError> {
    fs::create_dir_all(sidecar_dir)?;
    fs::create_dir_all(models_dir)?;
    let current = get_status(sidecar_dir, models_dir)?;
    if current.executable_exists {
        return Ok(RuntimeDownloadResult {
            status: current,
            downloaded: false,
            version: DEFAULT_RUNTIME_VERSION.to_string(),
            file_name: DEFAULT_RUNTIME_FILE.to_string(),
            url: DEFAULT_RUNTIME_URL.to_string(),
            expected_sha256: DEFAULT_RUNTIME_SHA256.to_string(),
            actual_sha256: String::new(),
            expected_bytes: DEFAULT_RUNTIME_BYTES,
            installed_files: Vec::new(),
        });
    }

    let archive_path = sidecar_dir.join(format!("{DEFAULT_RUNTIME_FILE}.download"));
    if archive_path.exists() {
        fs::remove_file(&archive_path)?;
    }
    download_to_file(DEFAULT_RUNTIME_URL, &archive_path)?;

    let actual_sha256 = sha256_file(&archive_path)?;
    if actual_sha256 != DEFAULT_RUNTIME_SHA256 {
        return Err(SidecarError::Checksum {
            path: archive_path.display().to_string(),
            expected: DEFAULT_RUNTIME_SHA256.to_string(),
            actual: actual_sha256,
        });
    }

    let extract_dir = sidecar_dir.join(format!("runtime-{DEFAULT_RUNTIME_VERSION}-x64"));
    if extract_dir.exists() {
        fs::remove_dir_all(&extract_dir)?;
    }
    fs::create_dir_all(&extract_dir)?;
    extract_zip(&archive_path, &extract_dir)?;
    let executable_dir = find_file_dir(&extract_dir, executable_name())?.ok_or_else(|| {
        SidecarError::InvalidArchive(format!(
            "{} was not found in {DEFAULT_RUNTIME_FILE}",
            executable_name()
        ))
    })?;
    let installed_files = copy_runtime_dir_files(&executable_dir, sidecar_dir)?;
    fs::remove_file(&archive_path)?;
    fs::remove_dir_all(&extract_dir)?;

    Ok(RuntimeDownloadResult {
        status: get_status(sidecar_dir, models_dir)?,
        downloaded: true,
        version: DEFAULT_RUNTIME_VERSION.to_string(),
        file_name: DEFAULT_RUNTIME_FILE.to_string(),
        url: DEFAULT_RUNTIME_URL.to_string(),
        expected_sha256: DEFAULT_RUNTIME_SHA256.to_string(),
        actual_sha256,
        expected_bytes: DEFAULT_RUNTIME_BYTES,
        installed_files,
    })
}

pub fn download_default_model(models_dir: &Path) -> Result<ModelDownloadResult, SidecarError> {
    download_model(models_dir, DEFAULT_MODEL_ID)
}

pub fn download_model(
    models_dir: &Path,
    model_id: &str,
) -> Result<ModelDownloadResult, SidecarError> {
    fs::create_dir_all(models_dir)?;
    let definition = model_definition(model_id);
    let current = model_status(models_dir, definition.id)?;
    if current.verified {
        return Ok(ModelDownloadResult {
            model: current,
            downloaded: false,
        });
    }

    let final_path = models_dir.join(definition.file_name);
    let temp_path = models_dir.join(format!("{}.download", definition.file_name));
    if temp_path.exists() {
        fs::remove_file(&temp_path)?;
    }

    download_to_file(definition.url, &temp_path)?;

    let actual_sha256 = sha256_file(&temp_path)?;
    if actual_sha256 != definition.sha256 {
        return Err(SidecarError::Checksum {
            path: temp_path.display().to_string(),
            expected: definition.sha256.to_string(),
            actual: actual_sha256,
        });
    }

    fs::rename(&temp_path, &final_path)?;
    write_model_verification_marker(models_dir, definition)?;
    Ok(ModelDownloadResult {
        model: model_status(models_dir, definition.id)?,
        downloaded: true,
    })
}

fn model_status(models_dir: &Path, model_id: &str) -> Result<ModelStatus, SidecarError> {
    let definition = model_definition(model_id);
    let path = models_dir.join(definition.file_name);
    let exists = path.exists();
    let actual_bytes = if exists {
        Some(fs::metadata(&path)?.len())
    } else {
        None
    };
    let actual_sha256 = if exists
        && actual_bytes == Some(definition.bytes)
        && model_marker_verified(models_dir, definition)
    {
        Some(definition.sha256.to_string())
    } else if exists {
        let hash = sha256_file(&path)?;
        if actual_bytes == Some(definition.bytes) && hash == definition.sha256 {
            write_model_verification_marker(models_dir, definition)?;
        }
        Some(hash)
    } else {
        None
    };
    let verified = actual_bytes == Some(definition.bytes)
        && actual_sha256.as_deref() == Some(definition.sha256);
    Ok(ModelStatus {
        id: definition.id.to_string(),
        file_name: definition.file_name.to_string(),
        path: path.display().to_string(),
        url: definition.url.to_string(),
        expected_sha256: definition.sha256.to_string(),
        expected_bytes: definition.bytes,
        exists,
        actual_bytes,
        actual_sha256,
        verified,
    })
}

fn model_definition(model_id: &str) -> ModelDefinition {
    match model_id {
        "large-v3" => MODEL_LARGE_V3,
        _ => MODEL_LARGE_V3_TURBO,
    }
}

fn model_marker_verified(models_dir: &Path, definition: ModelDefinition) -> bool {
    fs::read_to_string(model_verification_marker_path(models_dir, definition))
        .map(|value| value.trim() == definition.sha256)
        .unwrap_or(false)
}

fn write_model_verification_marker(
    models_dir: &Path,
    definition: ModelDefinition,
) -> Result<(), SidecarError> {
    fs::write(
        model_verification_marker_path(models_dir, definition),
        definition.sha256,
    )?;
    Ok(())
}

fn model_verification_marker_path(models_dir: &Path, definition: ModelDefinition) -> PathBuf {
    models_dir.join(format!("{}.sha256.verified", definition.file_name))
}

fn sha256_file(path: &Path) -> Result<String, SidecarError> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn download_to_file(url: &str, path: &Path) -> Result<(), SidecarError> {
    let client = reqwest::blocking::Client::builder()
        .user_agent("note-taker-sidecar-setup/0.1")
        .build()?;
    let mut response = client.get(url).send()?.error_for_status()?;
    let mut file = File::create(path)?;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = response.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        file.write_all(&buffer[..read])?;
    }
    file.flush()?;
    Ok(())
}

fn extract_zip(archive_path: &Path, destination_dir: &Path) -> Result<(), SidecarError> {
    let archive_file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(archive_file)?;
    for index in 0..archive.len() {
        let mut file = archive.by_index(index)?;
        if file.is_dir() {
            continue;
        }
        let enclosed_name = file.enclosed_name().ok_or_else(|| {
            SidecarError::InvalidArchive(format!("Unsafe archive path: {}", file.name()))
        })?;
        let output_path = destination_dir.join(enclosed_name);
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut output = File::create(&output_path)?;
        std::io::copy(&mut file, &mut output)?;
    }
    Ok(())
}

fn find_file_dir(root: &Path, file_name: &str) -> Result<Option<PathBuf>, SidecarError> {
    for entry in fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            if let Some(found) = find_file_dir(&path, file_name)? {
                return Ok(Some(found));
            }
        } else if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.eq_ignore_ascii_case(file_name))
        {
            return Ok(path.parent().map(Path::to_path_buf));
        }
    }
    Ok(None)
}

fn copy_runtime_dir_files(
    source_dir: &Path,
    sidecar_dir: &Path,
) -> Result<Vec<String>, SidecarError> {
    let mut installed_files = Vec::new();
    for entry in fs::read_dir(source_dir)? {
        let entry = entry?;
        let source_path = entry.path();
        if !source_path.is_file() {
            continue;
        }
        let file_name = source_path.file_name().ok_or_else(|| {
            SidecarError::InvalidArchive(format!(
                "Runtime file has no name: {}",
                source_path.display()
            ))
        })?;
        let destination_path = sidecar_dir.join(file_name);
        fs::copy(&source_path, &destination_path)?;
        installed_files.push(destination_path.display().to_string());
    }
    installed_files.sort();
    Ok(installed_files)
}

fn extract_transcript_output(
    raw_json: &str,
) -> Result<(String, Vec<TranscriptPart>), SidecarError> {
    let value: serde_json::Value = serde_json::from_str(raw_json)?;
    let parts = extract_transcript_parts(&value);
    if !parts.is_empty() {
        let text = parts
            .iter()
            .map(|part| part.text.as_str())
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ");
        return Ok((text, parts));
    }

    let mut parts = Vec::new();
    collect_text_fields(&value, &mut parts);
    let text = parts
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    Ok((text, Vec::new()))
}

fn extract_transcript_parts(value: &serde_json::Value) -> Vec<TranscriptPart> {
    let Some(items) = value
        .get("transcription")
        .and_then(|value| value.as_array())
    else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            let text = item.get("text")?.as_str()?.trim();
            if text.is_empty() {
                return None;
            }
            let offsets = item.get("offsets")?;
            let start_ms = offsets.get("from")?.as_i64()?;
            let end_ms = offsets.get("to")?.as_i64()?;
            if end_ms <= start_ms {
                return None;
            }
            Some(TranscriptPart {
                start_ms,
                end_ms,
                text: text.to_string(),
            })
        })
        .collect()
}

fn collect_text_fields(value: &serde_json::Value, parts: &mut Vec<String>) {
    match value {
        serde_json::Value::Object(map) => {
            for (key, child) in map {
                if key == "text" {
                    if let Some(text) = child.as_str() {
                        if !text.trim().is_empty() {
                            parts.push(text.trim().to_string());
                        }
                    }
                } else {
                    collect_text_fields(child, parts);
                }
            }
        }
        serde_json::Value::Array(items) => {
            for item in items {
                collect_text_fields(item, parts);
            }
        }
        _ => {}
    }
}

fn truncate_output(output: String) -> String {
    const MAX_OUTPUT_CHARS: usize = 4_000;
    if output.chars().count() <= MAX_OUTPUT_CHARS {
        return output.trim().to_string();
    }
    output
        .chars()
        .take(MAX_OUTPUT_CHARS)
        .collect::<String>()
        .trim()
        .to_string()
}

fn executable_name() -> &'static str {
    if cfg!(target_os = "windows") {
        "whisper-cli.exe"
    } else {
        "whisper-cli"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_transcript_text_collects_nested_text_fields() {
        let raw_json = r#"{
          "transcription": [{"text": " hello "}, {"ignored": true}],
          "segments": [{"text": "world"}, {"tokens": [{"text": "again"}]}]
        }"#;

        let (text, parts) = extract_transcript_output(raw_json).expect("extract transcript text");

        assert!(parts.is_empty());
        assert!(text.contains("hello"));
        assert!(text.contains("world"));
        assert!(text.contains("again"));
        assert!(!text.contains("  "));
    }

    #[test]
    fn extract_transcript_output_keeps_whisper_offsets() {
        let raw_json = r#"{
          "transcription": [
            {"offsets": {"from": 0, "to": 2200}, "text": " hello "},
            {"offsets": {"from": 2300, "to": 5900}, "text": "world"}
          ]
        }"#;

        let (text, parts) = extract_transcript_output(raw_json).expect("extract transcript output");

        assert_eq!(text, "hello world");
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0].start_ms, 0);
        assert_eq!(parts[0].end_ms, 2200);
        assert_eq!(parts[0].text, "hello");
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
    fn runtime_file_copy_installs_files_from_executable_directory() {
        let root =
            std::env::temp_dir().join(format!("note-taker-sidecar-test-{}", uuid::Uuid::new_v4()));
        let archive_dir = root.join("archive").join("bin");
        let sidecar_dir = root.join("sidecars");
        fs::create_dir_all(&archive_dir).expect("create archive dir");
        fs::create_dir_all(&sidecar_dir).expect("create sidecar dir");
        fs::write(archive_dir.join(executable_name()), b"exe").expect("write exe");
        fs::write(archive_dir.join("ggml.dll"), b"dll").expect("write dll");

        let found = find_file_dir(&root.join("archive"), executable_name())
            .expect("find executable dir")
            .expect("executable dir exists");
        let installed = copy_runtime_dir_files(&found, &sidecar_dir).expect("copy runtime files");

        assert_eq!(found, archive_dir);
        assert_eq!(installed.len(), 2);
        assert!(sidecar_dir.join(executable_name()).exists());
        assert!(sidecar_dir.join("ggml.dll").exists());

        fs::remove_dir_all(root).expect("remove temp root");
    }
}

//! Discover summary models through the same local CLI used for summaries.
use rusqlite::{Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

const CACHE_KEY: &str = "codex_model_catalog_v1";
const TIMEOUT: Duration = Duration::from_secs(30);
const FALLBACK: &[&str] = &[
    "gpt-5.4",
    "gpt-5.4-mini",
    "gpt-5.5",
    "gpt-5.6-sol",
    "gpt-5.6-terra",
    "gpt-5.6-luna",
];

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq)]
pub struct ModelOption {
    pub value: String,
    pub label: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalog {
    pub models: Vec<ModelOption>,
    pub refreshed_at: Option<String>,
}

pub fn valid_model_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.as_bytes()[0].is_ascii_alphanumeric()
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || b"-_.".contains(&c))
}

pub fn cached_models(database: &Path) -> Result<ModelCatalog, String> {
    let connection = Connection::open(database).map_err(|e| e.to_string())?;
    let cached: Option<String> = connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            [CACHE_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(catalog) = cached.and_then(|raw| serde_json::from_str::<ModelCatalog>(&raw).ok()) {
        if !catalog.models.is_empty() && catalog.models.iter().all(|m| valid_model_id(&m.value)) {
            return Ok(catalog);
        }
    }
    Ok(ModelCatalog {
        models: FALLBACK
            .iter()
            .map(|id| ModelOption {
                value: id.to_string(),
                label: id.to_string(),
            })
            .collect(),
        refreshed_at: None,
    })
}

pub fn refresh_models(database: &Path) -> Result<ModelCatalog, String> {
    let models = discover_models()?;
    let catalog = ModelCatalog {
        models,
        refreshed_at: Some(chrono::Utc::now().to_rfc3339()),
    };
    crate::storage::set_app_setting(
        database,
        CACHE_KEY,
        &serde_json::to_string(&catalog).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("Models fetched, but the local cache could not be saved: {e}"))?;
    Ok(catalog)
}

// The CLI shim can spawn children on Windows. Always clean up the entire owned
// process tree, including timeout/error paths, without affecting other Codex jobs.
struct ServerProcess(Child);
impl Drop for ServerProcess {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            #[cfg(target_os = "windows")]
            {
                let mut command = Command::new("taskkill");
                command
                    .args(["/PID", &self.0.id().to_string(), "/T", "/F"])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null());
                crate::process::suppress_console_window(&mut command);
                let _ = command.status();
            }
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}

fn send(input: &mut impl Write, message: Value) -> Result<(), String> {
    writeln!(input, "{message}")
        .and_then(|_| input.flush())
        .map_err(|_| {
            "Could not communicate with Codex. Update the Codex CLI and retry.".to_string()
        })
}

fn response(
    messages: &Receiver<Result<Value, String>>,
    id: u64,
    deadline: Instant,
) -> Result<Value, String> {
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("Codex model refresh timed out. Check your connection and retry.")?;
        let message = messages.recv_timeout(remaining).map_err(|error| {
            match error {
                mpsc::RecvTimeoutError::Timeout => {
                    "Codex model refresh timed out. Check your connection and retry."
                }
                mpsc::RecvTimeoutError::Disconnected => {
                    "Codex exited before returning models. Check your Codex login and CLI version."
                }
            }
            .to_string()
        })??;
        if message.get("id").and_then(Value::as_u64) != Some(id) {
            continue;
        }
        if message.get("error").is_some() {
            // Don't forward arbitrary process output or server diagnostics to UI.
            return Err("Codex rejected the model request. Check your Codex login and update the CLI, then retry.".to_string());
        }
        return message
            .get("result")
            .cloned()
            .ok_or_else(|| "Codex returned an invalid model response.".to_string());
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ModelPage {
    data: Vec<CodexModel>,
    next_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct CodexModel {
    model: String,
    #[serde(default)]
    display_name: String,
    #[serde(default)]
    hidden: bool,
    input_modalities: Option<Vec<String>>,
}

fn append_page(value: Value, models: &mut Vec<ModelOption>) -> Result<Option<String>, String> {
    let page: ModelPage = serde_json::from_value(value).map_err(|_| {
        "Codex returned an invalid model list. Update the CLI and retry.".to_string()
    })?;
    for model in page.data {
        if model.hidden
            || model
                .input_modalities
                .as_ref()
                .is_some_and(|m| !m.iter().any(|v| v == "text"))
        {
            continue;
        }
        if !valid_model_id(&model.model) {
            return Err("Codex returned an invalid model identifier.".to_string());
        }
        if models.iter().any(|m| m.value == model.model) {
            continue;
        }
        let label = if model.display_name.trim().is_empty() {
            model.model.clone()
        } else {
            model.display_name
        };
        models.push(ModelOption {
            value: model.model,
            label,
        });
    }
    Ok(page.next_cursor)
}

pub fn discover_models() -> Result<Vec<ModelOption>, String> {
    let mut command = Command::new(if cfg!(target_os = "windows") {
        "codex.cmd"
    } else {
        "codex"
    });
    command
        .args([
            "app-server",
            "--listen",
            "stdio://",
            "-c",
            "model_provider=\"openai\"",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    crate::process::suppress_console_window(&mut command);
    let mut server = ServerProcess(command.spawn().map_err(|_| {
        "Could not start Codex. Install the Codex CLI, run codex login, then retry.".to_string()
    })?);
    let mut input = server.0.stdin.take().ok_or("Could not open Codex input.")?;
    let output = server
        .0
        .stdout
        .take()
        .ok_or("Could not read Codex output.")?;
    let (sender, messages) = mpsc::sync_channel(32);
    std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            let parsed = line
                .map_err(|_| "Could not read Codex response.".to_string())
                .and_then(|line| {
                    serde_json::from_str(&line).map_err(|_| {
                        "Codex returned invalid JSON. Update the CLI and retry.".to_string()
                    })
                });
            if sender.send(parsed).is_err() {
                break;
            }
        }
    });
    let deadline = Instant::now() + TIMEOUT;
    send(
        &mut input,
        json!({"id": 1, "method": "initialize", "params": {
            "clientInfo": {"name": "note_taker", "title": "Note Taker", "version": env!("CARGO_PKG_VERSION")}
        }}),
    )?;
    response(&messages, 1, deadline)?;
    send(&mut input, json!({"method": "initialized", "params": {}}))?;
    let mut cursor: Option<String> = None;
    let mut cursors = HashSet::new();
    let mut models = Vec::new();
    for id in 2..102 {
        send(
            &mut input,
            json!({"id": id, "method": "model/list", "params": {
                "limit": 100, "includeHidden": false, "cursor": cursor
            }}),
        )?;
        cursor = append_page(response(&messages, id, deadline)?, &mut models)?;
        match &cursor {
            None if models.is_empty() => {
                return Err(
                    "Codex returned no summary models. Check your Codex login and retry."
                        .to_string(),
                )
            }
            None => return Ok(models),
            Some(next) if !cursors.insert(next.clone()) => {
                return Err("Codex repeated a model page. Update the CLI and retry.".to_string())
            }
            _ => {}
        }
    }
    Err("Codex returned too many model pages.".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_visible_text_models_preserves_ids_and_deduplicates() {
        let mut models = Vec::new();
        let cursor = append_page(
            json!({"data": [
            {"model": "gpt-future", "displayName": "Future", "inputModalities": ["text"]},
            {"model": "gpt-hidden", "hidden": true},
            {"model": "image-only", "inputModalities": ["image"]},
            {"model": "gpt-older"}, {"model": "gpt-future"}
        ], "nextCursor": "page2"}),
            &mut models,
        )
        .unwrap();
        assert_eq!(cursor.as_deref(), Some("page2"));
        assert_eq!(
            models.iter().map(|m| m.value.as_str()).collect::<Vec<_>>(),
            ["gpt-future", "gpt-older"]
        );
        assert_eq!(models[0].label, "Future");
        assert!(append_page(json!({"data": [{"model": "--bad"}]}), &mut models).is_err());
        assert!(append_page(json!({}), &mut models).is_err());
    }

    #[test]
    fn validates_dynamic_model_identifiers() {
        for valid in ["gpt-6-sol", "gpt-future-2026-09-23", "o3", "model_test.1"] {
            assert!(valid_model_id(valid));
        }
        for invalid in ["", "--model", "a b", "a\nb", "a&b", "a/b"] {
            assert!(!valid_model_id(invalid));
        }
        assert!(!valid_model_id(&"a".repeat(129)));
    }

    #[test]
    fn restores_cache_and_falls_back_on_corrupt_cache_without_changing_selection() {
        let path = std::env::temp_dir().join(format!(
            "note-taker-models-{}.sqlite3",
            uuid::Uuid::new_v4()
        ));
        crate::storage::initialize_database(&path).unwrap();
        let original = crate::storage::get_app_settings(&path)
            .unwrap()
            .summary_model;
        assert!(cached_models(&path).unwrap().refreshed_at.is_none());
        let catalog = ModelCatalog {
            models: vec![ModelOption {
                value: "gpt-future".into(),
                label: "Future".into(),
            }],
            refreshed_at: Some("2026-09-23T12:00:00Z".into()),
        };
        crate::storage::set_app_setting(
            &path,
            CACHE_KEY,
            &serde_json::to_string(&catalog).unwrap(),
        )
        .unwrap();
        let restored = cached_models(&path).unwrap();
        assert_eq!(restored.models, catalog.models);
        assert_eq!(restored.refreshed_at, catalog.refreshed_at);
        assert_eq!(
            crate::storage::get_app_settings(&path)
                .unwrap()
                .summary_model,
            original
        );
        for invalid in ["invalid json", r#"{"models":[],"refreshedAt":null}"#] {
            crate::storage::set_app_setting(&path, CACHE_KEY, invalid).unwrap();
            assert!(!cached_models(&path).unwrap().models.is_empty());
            assert!(cached_models(&path).unwrap().refreshed_at.is_none());
        }
        std::fs::remove_file(path).unwrap();
    }

    #[test]
    fn handles_notifications_errors_disconnects_and_timeout() {
        let (sender, receiver) = mpsc::channel();
        sender.send(Ok(json!({"method": "notification"}))).unwrap();
        sender
            .send(Ok(json!({"id": 2, "result": {"data": []}})))
            .unwrap();
        assert!(response(&receiver, 2, Instant::now() + TIMEOUT).is_ok());
        sender
            .send(Ok(
                json!({"id": 3, "error": {"message": "private diagnostics"}}),
            ))
            .unwrap();
        assert!(!response(&receiver, 3, Instant::now() + TIMEOUT)
            .unwrap_err()
            .contains("private diagnostics"));
        assert!(response(&receiver, 4, Instant::now())
            .unwrap_err()
            .contains("timed out"));
        drop(sender);
        assert!(response(&receiver, 4, Instant::now() + TIMEOUT)
            .unwrap_err()
            .contains("exited"));
    }

    #[test]
    #[ignore = "requires installed Codex CLI and existing login; performs model discovery only"]
    fn live_codex_model_discovery() {
        let models = discover_models().unwrap();
        assert!(!models.is_empty());
        println!(
            "Discovered models: {}",
            models
                .iter()
                .map(|m| m.value.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        );
    }
}

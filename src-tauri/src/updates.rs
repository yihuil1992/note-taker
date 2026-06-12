use reqwest::blocking::Client;
use semver::Version;
use serde::{Deserialize, Serialize};
use thiserror::Error;

const GITHUB_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/yihuil1992/note-taker/releases/latest";
const USER_AGENT: &str = "note-taker-update-check";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateCheck {
    pub current_version: String,
    pub latest_version: Option<String>,
    pub update_available: bool,
    pub release_name: Option<String>,
    pub release_url: Option<String>,
    pub published_at: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Error)]
pub enum UpdateCheckError {
    #[error("failed to request latest GitHub release: {0}")]
    Request(#[from] reqwest::Error),
    #[error("failed to parse bundled version {0}")]
    InvalidCurrentVersion(String),
}

#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
    name: Option<String>,
    html_url: String,
    published_at: Option<String>,
    body: Option<String>,
}

pub fn check_latest_release(current_version: &str) -> Result<AppUpdateCheck, UpdateCheckError> {
    let current = parse_version(current_version)
        .ok_or_else(|| UpdateCheckError::InvalidCurrentVersion(current_version.to_string()))?;
    let release = Client::new()
        .get(GITHUB_LATEST_RELEASE_URL)
        .header(reqwest::header::USER_AGENT, USER_AGENT)
        .header(reqwest::header::ACCEPT, "application/vnd.github+json")
        .send()?
        .error_for_status()?
        .json::<GitHubRelease>()?;

    let latest = parse_version(&release.tag_name);
    let update_available = latest
        .as_ref()
        .map(|version| version > &current)
        .unwrap_or(false);

    Ok(AppUpdateCheck {
        current_version: current_version.to_string(),
        latest_version: Some(release.tag_name),
        update_available,
        release_name: release.name,
        release_url: Some(release.html_url),
        published_at: release.published_at,
        notes: release.body.map(|body| trim_notes(&body)),
    })
}

fn parse_version(value: &str) -> Option<Version> {
    Version::parse(value.trim().trim_start_matches('v')).ok()
}

fn trim_notes(value: &str) -> String {
    let collapsed = value
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    const MAX_CHARS: usize = 220;
    if collapsed.chars().count() <= MAX_CHARS {
        return collapsed;
    }
    format!(
        "{}...",
        collapsed.chars().take(MAX_CHARS).collect::<String>()
    )
}

#[cfg(test)]
mod tests {
    use super::{parse_version, trim_notes};

    #[test]
    fn parses_plain_and_prefixed_versions() {
        assert_eq!(parse_version("0.2.0").unwrap().to_string(), "0.2.0");
        assert_eq!(parse_version("v1.4.3").unwrap().to_string(), "1.4.3");
    }

    #[test]
    fn trims_notes_without_breaking_empty_lines() {
        let notes = trim_notes("  First line\n\nSecond line  ");
        assert_eq!(notes, "First line Second line");
    }
}

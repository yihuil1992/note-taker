use note_taker_lib::storage::*;
use rusqlite::{params, Connection};
use std::path::PathBuf;

struct TestDatabase {
    root: PathBuf,
    path: PathBuf,
}

impl TestDatabase {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "note-taker-sqlite-validation-{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir(&root).unwrap();
        Self {
            path: root.join("synthetic.sqlite3"),
            root,
        }
    }
}

impl Drop for TestDatabase {
    fn drop(&mut self) {
        // The directory is created uniquely above and contains only synthetic test data.
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

#[test]
fn sqlite_runtime_is_patched() {
    let connection = Connection::open_in_memory().unwrap();
    let runtime: String = connection
        .query_row("SELECT sqlite_version()", [], |row| row.get(0))
        .unwrap();
    println!(
        "SQLite runtime: {runtime}; rusqlite reported: {}",
        rusqlite::version()
    );
    assert_eq!(runtime, rusqlite::version());
    assert!(
        rusqlite::version_number() >= 3_054_000,
        "RUSTSEC-2026-0459 requires SQLite >= 3.54.0, got {runtime}"
    );
}

#[test]
fn initialization_is_idempotent_and_settings_persist() {
    let db = TestDatabase::new();
    initialize_database(&db.path).unwrap();
    set_app_setting(&db.path, "custom_glossary", "SQLite, 中文, 日本語").unwrap();
    set_app_setting(&db.path, "raw_audio_retention_days", "14").unwrap();
    initialize_database(&db.path).unwrap();
    let settings = get_app_settings(&db.path).unwrap();
    assert_eq!(settings.custom_glossary, "SQLite, 中文, 日本語");
    assert_eq!(settings.raw_audio_retention_days, 14);
    let connection = Connection::open(&db.path).unwrap();
    let journal: String = connection
        .query_row("PRAGMA journal_mode", [], |row| row.get(0))
        .unwrap();
    assert_eq!(journal, "wal");
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
}

#[test]
fn legacy_meeting_schema_migrates_without_losing_rows() {
    let db = TestDatabase::new();
    let connection = Connection::open(&db.path).unwrap();
    connection
        .execute_batch(
            "CREATE TABLE meetings (
        id TEXT PRIMARY KEY, title TEXT NOT NULL,
        title_source TEXT NOT NULL DEFAULT 'datetime_placeholder',
        started_at TEXT NOT NULL, ended_at TEXT, status TEXT NOT NULL,
        language_hint TEXT NOT NULL DEFAULT 'auto',
        summary_language TEXT NOT NULL DEFAULT 'auto',
        created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
        updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP);
        INSERT INTO meetings (id,title,started_at,status)
        VALUES ('legacy','Legacy synthetic meeting','2026-11-09T00:00:00Z','recorded');",
        )
        .unwrap();
    drop(connection);
    initialize_database(&db.path).unwrap();
    initialize_database(&db.path).unwrap();
    let meeting = get_meeting(&db.path, "legacy").unwrap().unwrap();
    assert_eq!(meeting.title, "Legacy synthetic meeting");
    assert!(meeting.archived_at.is_none());
    archive_meeting(&db.path, "legacy").unwrap();
    restore_meeting(&db.path, "legacy").unwrap();
    assert_eq!(list_recent_meetings(&db.path, 20).unwrap().len(), 1);
}

#[test]
fn transcript_summary_search_archive_and_restore_round_trip() {
    let db = TestDatabase::new();
    initialize_database(&db.path).unwrap();
    insert_meeting(
        &db.path,
        &NewMeeting {
            id: "meeting",
            title: "Synthetic project meeting",
            title_source: "user",
            started_at: "2026-11-09T00:00:00Z",
            status: "recorded",
            language_hint: "zh",
            summary_language: "auto",
        },
    )
    .unwrap();
    insert_transcript_segment(
        &db.path,
        &NewTranscriptSegment {
            id: "segment",
            meeting_id: "meeting",
            source_kind: "microphone",
            speaker_label: "Me",
            language: "zh",
            start_ms: 0,
            end_ms: 1000,
            text: "Regression searchable transcript 中文 日本語",
            confidence: Some(0.9),
            provider: "synthetic",
        },
    )
    .unwrap();
    upsert_meeting_summary(
        &db.path,
        &NewMeetingSummary {
            meeting_id: "meeting",
            suggested_title: "Synthetic project meeting",
            provider: "synthetic",
            model: "test",
            language: "en",
            overview: "Compatibility overview",
            decisions_json: "[]",
            action_items_json: "[{\"task\":\"Test migration\"}]",
            topics_json: "[\"SQLite\"]",
            risks_or_questions_json: "[]",
            raw_json: "{}",
        },
    )
    .unwrap();
    assert_eq!(search_meetings(&db.path, "中文", 20).unwrap().len(), 1);
    assert_eq!(
        search_meetings(&db.path, "Compatibility", 20)
            .unwrap()
            .len(),
        1
    );
    let connection = Connection::open(&db.path).unwrap();
    let matches: i64 = connection
        .query_row(
            "SELECT count(*) FROM meeting_search WHERE meeting_search MATCH ?1",
            params!["Regression"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(matches, 1);
    drop(connection);
    archive_meeting(&db.path, "meeting").unwrap();
    assert!(search_meetings(&db.path, "Regression", 20)
        .unwrap()
        .is_empty());
    assert_eq!(list_archived_meetings(&db.path, 20).unwrap().len(), 1);
    restore_meeting(&db.path, "meeting").unwrap();
    restore_meeting(&db.path, "meeting").unwrap();
    let detail = get_meeting_detail(&db.path, "meeting").unwrap().unwrap();
    assert_eq!(detail.transcript_segments.len(), 1);
    assert_eq!(detail.summary.unwrap().overview, "Compatibility overview");
    assert_eq!(
        search_meetings(&db.path, "Regression", 20).unwrap().len(),
        1
    );
    let connection = Connection::open(&db.path).unwrap();
    let matches: i64 = connection
        .query_row(
            "SELECT count(*) FROM meeting_search WHERE meeting_search MATCH ?1",
            params!["Regression"],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(matches, 1);
    let integrity: String = connection
        .query_row("PRAGMA integrity_check", [], |row| row.get(0))
        .unwrap();
    assert_eq!(integrity, "ok");
}

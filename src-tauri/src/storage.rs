use rusqlite::{params, Connection, OptionalExtension, Result};
use serde::Serialize;
use std::path::Path;

pub fn initialize_database(path: &Path) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA foreign_keys = ON;

        CREATE TABLE IF NOT EXISTS meetings (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            title_source TEXT NOT NULL DEFAULT 'datetime_placeholder',
            started_at TEXT NOT NULL,
            ended_at TEXT,
            status TEXT NOT NULL,
            language_hint TEXT NOT NULL DEFAULT 'auto',
            summary_language TEXT NOT NULL DEFAULT 'auto',
            archived_at TEXT,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS audio_sources (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            kind TEXT NOT NULL,
            device_id TEXT,
            display_name TEXT NOT NULL,
            sample_rate INTEGER NOT NULL,
            channels INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS audio_chunks (
            id TEXT PRIMARY KEY,
            meeting_id TEXT REFERENCES meetings(id) ON DELETE CASCADE,
            source_kind TEXT NOT NULL,
            started_at_ms INTEGER NOT NULL,
            duration_ms INTEGER NOT NULL,
            path TEXT NOT NULL,
            status TEXT NOT NULL,
            transcription_error TEXT
        );

        CREATE TABLE IF NOT EXISTS transcript_segments (
            id TEXT PRIMARY KEY,
            meeting_id TEXT NOT NULL REFERENCES meetings(id) ON DELETE CASCADE,
            source_kind TEXT NOT NULL,
            speaker_label TEXT NOT NULL,
            language TEXT NOT NULL,
            start_ms INTEGER NOT NULL,
            end_ms INTEGER NOT NULL,
            text TEXT NOT NULL,
            confidence REAL,
            provider TEXT NOT NULL
        );

        CREATE TABLE IF NOT EXISTS meeting_summaries (
            meeting_id TEXT PRIMARY KEY REFERENCES meetings(id) ON DELETE CASCADE,
            suggested_title TEXT NOT NULL,
            provider TEXT NOT NULL,
            model TEXT NOT NULL,
            language TEXT NOT NULL,
            overview TEXT NOT NULL,
            decisions_json TEXT NOT NULL,
            action_items_json TEXT NOT NULL,
            topics_json TEXT NOT NULL,
            risks_or_questions_json TEXT NOT NULL,
            raw_json TEXT NOT NULL,
            generated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE TABLE IF NOT EXISTS app_settings (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        );

        CREATE VIRTUAL TABLE IF NOT EXISTS meeting_search USING fts5(
            meeting_id UNINDEXED,
            title,
            summary,
            action_items,
            topics,
            transcript
        );

        INSERT OR IGNORE INTO app_settings (key, value) VALUES
            ('raw_audio_retention_days', '7'),
            ('transcription_provider', 'local-whisper'),
            ('summary_provider', 'codex-cli'),
            ('summary_model', 'gpt-5.4'),
            ('local_transcription_model', 'large-v3-turbo'),
            ('openai_transcription_model', 'gpt-4o-mini-transcribe'),
            ('language_hint', 'zh'),
            ('summary_language', 'auto'),
            ('custom_glossary', ''),
            ('recording_consent_reminder_dismissed', 'false');
        "#,
    )?;
    connection.execute(
        "UPDATE app_settings SET value = 'large-v3-turbo', updated_at = CURRENT_TIMESTAMP WHERE key = 'local_transcription_model' AND value IN ('tiny', 'large-v3')",
        [],
    )?;
    ensure_column(&connection, "meetings", "archived_at", "TEXT")?;
    connection.execute(
        "DELETE FROM meeting_search WHERE meeting_id IN (SELECT id FROM meetings WHERE archived_at IS NOT NULL)",
        [],
    )?;
    Ok(())
}

fn ensure_column(
    connection: &Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> Result<()> {
    let mut statement = connection.prepare(&format!("PRAGMA table_info({table})"))?;
    let rows = statement.query_map([], |row| row.get::<_, String>(1))?;
    for row in rows {
        if row? == column {
            return Ok(());
        }
    }
    connection.execute(
        &format!("ALTER TABLE {table} ADD COLUMN {column} {definition}"),
        [],
    )?;
    Ok(())
}

pub struct NewMeeting<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub title_source: &'a str,
    pub started_at: &'a str,
    pub status: &'a str,
    pub language_hint: &'a str,
    pub summary_language: &'a str,
}

pub struct NewAudioSource<'a> {
    pub id: &'a str,
    pub meeting_id: &'a str,
    pub kind: &'a str,
    pub device_id: Option<&'a str>,
    pub display_name: &'a str,
    pub sample_rate: u32,
    pub channels: u16,
}

pub struct NewAudioChunk<'a> {
    pub id: &'a str,
    pub meeting_id: &'a str,
    pub source_kind: &'a str,
    pub started_at_ms: i64,
    pub duration_ms: i64,
    pub path: &'a str,
    pub status: &'a str,
    pub transcription_error: Option<&'a str>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingRecord {
    pub id: String,
    pub title: String,
    pub title_source: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: String,
    pub language_hint: String,
    pub summary_language: String,
    pub archived_at: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingListItem {
    pub id: String,
    pub title: String,
    pub title_source: String,
    pub started_at: String,
    pub ended_at: Option<String>,
    pub status: String,
    pub summary_overview: Option<String>,
    pub chunk_count: i64,
    pub segment_count: i64,
    pub action_item_count: usize,
    pub topic_count: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioChunkRecord {
    pub id: String,
    pub meeting_id: String,
    pub source_kind: String,
    pub started_at_ms: i64,
    pub duration_ms: i64,
    pub path: String,
    pub status: String,
    pub transcription_error: Option<String>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptSegmentRecord {
    pub id: String,
    pub meeting_id: String,
    pub source_kind: String,
    pub speaker_label: String,
    pub language: String,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub provider: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingSummaryRecord {
    pub meeting_id: String,
    pub suggested_title: String,
    pub provider: String,
    pub model: String,
    pub language: String,
    pub overview: String,
    pub decisions_json: String,
    pub action_items_json: String,
    pub topics_json: String,
    pub risks_or_questions_json: String,
    pub raw_json: String,
    pub generated_at: String,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MeetingDetailRecord {
    pub meeting: MeetingRecord,
    pub chunks: Vec<AudioChunkRecord>,
    pub transcript_segments: Vec<TranscriptSegmentRecord>,
    pub summary: Option<MeetingSummaryRecord>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppSettingsRecord {
    pub raw_audio_retention_days: u8,
    pub transcription_provider: String,
    pub summary_provider: String,
    pub summary_model: String,
    pub local_transcription_model: String,
    pub openai_transcription_model: String,
    pub language_hint: String,
    pub summary_language: String,
    pub custom_glossary: String,
    pub recording_consent_reminder_dismissed: bool,
}

pub struct NewTranscriptSegment<'a> {
    pub id: &'a str,
    pub meeting_id: &'a str,
    pub source_kind: &'a str,
    pub speaker_label: &'a str,
    pub language: &'a str,
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: &'a str,
    pub confidence: Option<f32>,
    pub provider: &'a str,
}

pub struct NewMeetingSummary<'a> {
    pub meeting_id: &'a str,
    pub suggested_title: &'a str,
    pub provider: &'a str,
    pub model: &'a str,
    pub language: &'a str,
    pub overview: &'a str,
    pub decisions_json: &'a str,
    pub action_items_json: &'a str,
    pub topics_json: &'a str,
    pub risks_or_questions_json: &'a str,
    pub raw_json: &'a str,
}

pub fn insert_meeting(path: &Path, meeting: &NewMeeting<'_>) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        INSERT INTO meetings (
            id, title, title_source, started_at, status, language_hint, summary_language
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            meeting.id,
            meeting.title,
            meeting.title_source,
            meeting.started_at,
            meeting.status,
            meeting.language_hint,
            meeting.summary_language
        ],
    )?;
    Ok(())
}

pub fn get_meeting(path: &Path, meeting_id: &str) -> Result<Option<MeetingRecord>> {
    let connection = Connection::open(path)?;
    connection
        .query_row(
            r#"
            SELECT id, title, title_source, started_at, ended_at, status,
                   language_hint, summary_language, archived_at, created_at, updated_at
            FROM meetings
            WHERE id = ?1
            "#,
            params![meeting_id],
            row_to_meeting,
        )
        .optional()
}

pub fn get_meeting_detail(path: &Path, meeting_id: &str) -> Result<Option<MeetingDetailRecord>> {
    let Some(meeting) = get_meeting(path, meeting_id)? else {
        return Ok(None);
    };
    let chunks = list_audio_chunks_for_meeting(path, meeting_id)?;
    let transcript_segments = list_transcript_segments_for_meeting(path, meeting_id)?;
    let summary = get_meeting_summary(path, meeting_id)?;

    Ok(Some(MeetingDetailRecord {
        meeting,
        chunks,
        transcript_segments,
        summary,
    }))
}

pub fn list_recent_meetings(path: &Path, limit: usize) -> Result<Vec<MeetingListItem>> {
    let connection = Connection::open(path)?;
    let limit = limit.clamp(1, 200) as i64;
    let mut statement = connection.prepare(
        r#"
        SELECT
            m.id,
            m.title,
            m.title_source,
            m.started_at,
            m.ended_at,
            m.status,
            s.overview,
            COALESCE(chunk_counts.count, 0),
            COALESCE(segment_counts.count, 0),
            s.action_items_json,
            s.topics_json
        FROM meetings m
        LEFT JOIN meeting_summaries s ON s.meeting_id = m.id
        LEFT JOIN (
            SELECT meeting_id, COUNT(*) AS count FROM audio_chunks GROUP BY meeting_id
        ) chunk_counts ON chunk_counts.meeting_id = m.id
        LEFT JOIN (
            SELECT meeting_id, COUNT(*) AS count
            FROM (
                SELECT meeting_id, source_kind, speaker_label, language, start_ms, text, provider
                FROM transcript_segments
                GROUP BY meeting_id, source_kind, speaker_label, language, start_ms, text, provider
            )
            GROUP BY meeting_id
        ) segment_counts ON segment_counts.meeting_id = m.id
        WHERE m.archived_at IS NULL
        ORDER BY m.started_at DESC
        LIMIT ?1
        "#,
    )?;

    let rows = statement.query_map(params![limit], row_to_meeting_list_item)?;
    collect_rows(rows)
}

pub fn insert_audio_source(path: &Path, source: &NewAudioSource<'_>) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        INSERT OR IGNORE INTO audio_sources (
            id, meeting_id, kind, device_id, display_name, sample_rate, channels
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
        "#,
        params![
            source.id,
            source.meeting_id,
            source.kind,
            source.device_id,
            source.display_name,
            i64::from(source.sample_rate),
            i64::from(source.channels)
        ],
    )?;
    Ok(())
}

pub fn insert_audio_chunk(path: &Path, chunk: &NewAudioChunk<'_>) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        INSERT INTO audio_chunks (
            id, meeting_id, source_kind, started_at_ms, duration_ms, path, status, transcription_error
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
        "#,
        params![
            chunk.id,
            chunk.meeting_id,
            chunk.source_kind,
            chunk.started_at_ms,
            chunk.duration_ms,
            chunk.path,
            chunk.status,
            chunk.transcription_error
        ],
    )?;
    Ok(())
}

pub fn list_audio_chunks_for_meeting(
    path: &Path,
    meeting_id: &str,
) -> Result<Vec<AudioChunkRecord>> {
    let connection = Connection::open(path)?;
    let mut statement = connection.prepare(
        r#"
        SELECT id, meeting_id, source_kind, started_at_ms, duration_ms, path, status, transcription_error
        FROM audio_chunks
        WHERE meeting_id = ?1
        ORDER BY started_at_ms ASC, source_kind ASC
        "#,
    )?;
    let rows = statement.query_map(params![meeting_id], |row| {
        Ok(AudioChunkRecord {
            id: row.get(0)?,
            meeting_id: row.get(1)?,
            source_kind: row.get(2)?,
            started_at_ms: row.get(3)?,
            duration_ms: row.get(4)?,
            path: row.get(5)?,
            status: row.get(6)?,
            transcription_error: row.get(7)?,
        })
    })?;

    let mut chunks = Vec::new();
    for row in rows {
        chunks.push(row?);
    }
    Ok(chunks)
}

pub fn update_audio_chunk_transcription_status(
    path: &Path,
    chunk_id: &str,
    status: &str,
    transcription_error: Option<&str>,
) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        UPDATE audio_chunks
        SET status = ?2, transcription_error = ?3
        WHERE id = ?1
        "#,
        params![chunk_id, status, transcription_error],
    )?;
    Ok(())
}

pub fn reset_meeting_transcription(path: &Path, meeting_id: &str) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        "DELETE FROM transcript_segments WHERE meeting_id = ?1",
        params![meeting_id],
    )?;
    connection.execute(
        "DELETE FROM meeting_summaries WHERE meeting_id = ?1",
        params![meeting_id],
    )?;
    connection.execute(
        r#"
        UPDATE audio_chunks
        SET status = CASE
                WHEN status = 'capture_failed' THEN status
                ELSE 'captured'
            END,
            transcription_error = NULL
        WHERE meeting_id = ?1
        "#,
        params![meeting_id],
    )?;
    connection.execute(
        r#"
        UPDATE meetings
        SET status = 'recorded', updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
        params![meeting_id],
    )?;
    rebuild_meeting_search_with_connection(&connection, meeting_id)?;
    Ok(())
}

pub fn find_transcript_segment_for_chunk(
    path: &Path,
    meeting_id: &str,
    source_kind: &str,
    start_ms: i64,
    end_ms: i64,
) -> Result<Option<TranscriptSegmentRecord>> {
    let connection = Connection::open(path)?;
    let mut statement = connection.prepare(
        r#"
        SELECT id, meeting_id, source_kind, speaker_label, language, start_ms, end_ms, text, provider
        FROM transcript_segments
        WHERE meeting_id = ?1 AND source_kind = ?2 AND start_ms = ?3 AND end_ms = ?4
        ORDER BY id ASC
        LIMIT 1
        "#,
    )?;
    let mut rows = statement.query(params![meeting_id, source_kind, start_ms, end_ms])?;
    if let Some(row) = rows.next()? {
        return Ok(Some(TranscriptSegmentRecord {
            id: row.get(0)?,
            meeting_id: row.get(1)?,
            source_kind: row.get(2)?,
            speaker_label: row.get(3)?,
            language: row.get(4)?,
            start_ms: row.get(5)?,
            end_ms: row.get(6)?,
            text: row.get(7)?,
            provider: row.get(8)?,
        }));
    }
    Ok(None)
}

pub fn list_transcript_segments_for_meeting(
    path: &Path,
    meeting_id: &str,
) -> Result<Vec<TranscriptSegmentRecord>> {
    let connection = Connection::open(path)?;
    let mut statement = connection.prepare(
        r#"
        SELECT MIN(id), meeting_id, source_kind, speaker_label, language, start_ms, MAX(end_ms), text, provider
        FROM transcript_segments
        WHERE meeting_id = ?1
        GROUP BY meeting_id, source_kind, speaker_label, language, start_ms, text, provider
        ORDER BY start_ms ASC, source_kind ASC
        "#,
    )?;
    let rows = statement.query_map(params![meeting_id], |row| {
        Ok(TranscriptSegmentRecord {
            id: row.get(0)?,
            meeting_id: row.get(1)?,
            source_kind: row.get(2)?,
            speaker_label: row.get(3)?,
            language: row.get(4)?,
            start_ms: row.get(5)?,
            end_ms: row.get(6)?,
            text: row.get(7)?,
            provider: row.get(8)?,
        })
    })?;

    let mut segments = Vec::new();
    for row in rows {
        segments.push(row?);
    }
    Ok(segments)
}

pub fn insert_transcript_segment(path: &Path, segment: &NewTranscriptSegment<'_>) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        INSERT INTO transcript_segments (
            id, meeting_id, source_kind, speaker_label, language, start_ms, end_ms, text, confidence, provider
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
        "#,
        params![
            segment.id,
            segment.meeting_id,
            segment.source_kind,
            segment.speaker_label,
            segment.language,
            segment.start_ms,
            segment.end_ms,
            segment.text,
            segment.confidence,
            segment.provider
        ],
    )?;
    rebuild_meeting_search_with_connection(&connection, segment.meeting_id)?;
    Ok(())
}

pub fn upsert_meeting_summary(path: &Path, summary: &NewMeetingSummary<'_>) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        INSERT INTO meeting_summaries (
            meeting_id, suggested_title, provider, model, language, overview,
            decisions_json, action_items_json, topics_json, risks_or_questions_json, raw_json
        ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
        ON CONFLICT(meeting_id) DO UPDATE SET
            suggested_title = excluded.suggested_title,
            provider = excluded.provider,
            model = excluded.model,
            language = excluded.language,
            overview = excluded.overview,
            decisions_json = excluded.decisions_json,
            action_items_json = excluded.action_items_json,
            topics_json = excluded.topics_json,
            risks_or_questions_json = excluded.risks_or_questions_json,
            raw_json = excluded.raw_json,
            generated_at = CURRENT_TIMESTAMP
        "#,
        params![
            summary.meeting_id,
            summary.suggested_title,
            summary.provider,
            summary.model,
            summary.language,
            summary.overview,
            summary.decisions_json,
            summary.action_items_json,
            summary.topics_json,
            summary.risks_or_questions_json,
            summary.raw_json
        ],
    )?;
    rebuild_meeting_search_with_connection(&connection, summary.meeting_id)?;
    Ok(())
}

pub fn get_meeting_summary(path: &Path, meeting_id: &str) -> Result<Option<MeetingSummaryRecord>> {
    let connection = Connection::open(path)?;
    connection
        .query_row(
            r#"
            SELECT meeting_id, suggested_title, provider, model, language, overview,
                   decisions_json, action_items_json, topics_json,
                   risks_or_questions_json, raw_json, generated_at
            FROM meeting_summaries
            WHERE meeting_id = ?1
            "#,
            params![meeting_id],
            |row| {
                Ok(MeetingSummaryRecord {
                    meeting_id: row.get(0)?,
                    suggested_title: row.get(1)?,
                    provider: row.get(2)?,
                    model: row.get(3)?,
                    language: row.get(4)?,
                    overview: row.get(5)?,
                    decisions_json: row.get(6)?,
                    action_items_json: row.get(7)?,
                    topics_json: row.get(8)?,
                    risks_or_questions_json: row.get(9)?,
                    raw_json: row.get(10)?,
                    generated_at: row.get(11)?,
                })
            },
        )
        .optional()
}

pub fn search_meetings(path: &Path, query: &str, limit: usize) -> Result<Vec<MeetingListItem>> {
    let query = query.trim();
    if query.is_empty() {
        return list_recent_meetings(path, limit);
    }

    let connection = Connection::open(path)?;
    let limit = limit.clamp(1, 200) as i64;
    let like = format!("%{query}%");
    let mut statement = connection.prepare(
        r#"
        SELECT DISTINCT
            m.id,
            m.title,
            m.title_source,
            m.started_at,
            m.ended_at,
            m.status,
            s.overview,
            COALESCE(chunk_counts.count, 0),
            COALESCE(segment_counts.count, 0),
            s.action_items_json,
            s.topics_json
        FROM meetings m
        LEFT JOIN meeting_summaries s ON s.meeting_id = m.id
        LEFT JOIN transcript_segments ts ON ts.meeting_id = m.id
        LEFT JOIN (
            SELECT meeting_id, COUNT(*) AS count FROM audio_chunks GROUP BY meeting_id
        ) chunk_counts ON chunk_counts.meeting_id = m.id
        LEFT JOIN (
            SELECT meeting_id, COUNT(*) AS count
            FROM (
                SELECT meeting_id, source_kind, speaker_label, language, start_ms, text, provider
                FROM transcript_segments
                GROUP BY meeting_id, source_kind, speaker_label, language, start_ms, text, provider
            )
            GROUP BY meeting_id
        ) segment_counts ON segment_counts.meeting_id = m.id
        WHERE m.archived_at IS NULL
          AND (
            m.title LIKE ?1
            OR COALESCE(s.overview, '') LIKE ?1
            OR COALESCE(s.action_items_json, '') LIKE ?1
            OR COALESCE(s.topics_json, '') LIKE ?1
            OR COALESCE(ts.text, '') LIKE ?1
          )
        ORDER BY m.started_at DESC
        LIMIT ?2
        "#,
    )?;
    let rows = statement.query_map(params![like, limit], row_to_meeting_list_item)?;
    collect_rows(rows)
}

pub fn get_app_settings(path: &Path) -> Result<AppSettingsRecord> {
    let raw_audio_retention_days = get_setting(path, "raw_audio_retention_days")?
        .and_then(|value| value.parse::<u8>().ok())
        .unwrap_or(7);
    let transcription_provider =
        get_setting(path, "transcription_provider")?.unwrap_or_else(|| "local-whisper".to_string());
    let summary_provider =
        get_setting(path, "summary_provider")?.unwrap_or_else(|| "codex-cli".to_string());
    let summary_model =
        get_setting(path, "summary_model")?.unwrap_or_else(|| "gpt-5.4".to_string());
    let local_transcription_model = get_setting(path, "local_transcription_model")?
        .unwrap_or_else(|| "large-v3-turbo".to_string());
    let openai_transcription_model = get_setting(path, "openai_transcription_model")?
        .unwrap_or_else(|| crate::openai_transcription::default_model().to_string());
    let language_hint = get_setting(path, "language_hint")?.unwrap_or_else(|| "zh".to_string());
    let summary_language =
        get_setting(path, "summary_language")?.unwrap_or_else(|| "auto".to_string());
    let custom_glossary = get_setting(path, "custom_glossary")?.unwrap_or_default();
    let recording_consent_reminder_dismissed =
        get_setting(path, "recording_consent_reminder_dismissed")?
            .map(|value| value == "true")
            .unwrap_or(false);

    Ok(AppSettingsRecord {
        raw_audio_retention_days,
        transcription_provider,
        summary_provider,
        summary_model,
        local_transcription_model,
        openai_transcription_model,
        language_hint,
        summary_language,
        custom_glossary,
        recording_consent_reminder_dismissed,
    })
}

pub fn set_app_setting(path: &Path, key: &str, value: &str) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        INSERT INTO app_settings (key, value, updated_at)
        VALUES (?1, ?2, CURRENT_TIMESTAMP)
        ON CONFLICT(key) DO UPDATE SET
            value = excluded.value,
            updated_at = CURRENT_TIMESTAMP
        "#,
        params![key, value],
    )?;
    Ok(())
}

pub fn finish_meeting(path: &Path, meeting_id: &str, ended_at: &str, status: &str) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        UPDATE meetings
        SET ended_at = ?2, status = ?3, updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
        params![meeting_id, ended_at, status],
    )?;
    Ok(())
}

pub fn update_meeting_title(
    path: &Path,
    meeting_id: &str,
    title: &str,
    title_source: &str,
) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        UPDATE meetings
        SET title = ?2, title_source = ?3, updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
        params![meeting_id, title, title_source],
    )?;
    rebuild_meeting_search_with_connection(&connection, meeting_id)?;
    Ok(())
}

pub fn update_meeting_status(path: &Path, meeting_id: &str, status: &str) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        UPDATE meetings
        SET status = ?2, updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1
        "#,
        params![meeting_id, status],
    )?;
    Ok(())
}

pub fn archive_meeting(path: &Path, meeting_id: &str) -> Result<()> {
    let connection = Connection::open(path)?;
    connection.execute(
        r#"
        UPDATE meetings
        SET archived_at = CURRENT_TIMESTAMP, updated_at = CURRENT_TIMESTAMP
        WHERE id = ?1 AND archived_at IS NULL
        "#,
        params![meeting_id],
    )?;
    connection.execute(
        "DELETE FROM meeting_search WHERE meeting_id = ?1",
        params![meeting_id],
    )?;
    Ok(())
}

fn get_setting(path: &Path, key: &str) -> Result<Option<String>> {
    let connection = Connection::open(path)?;
    connection
        .query_row(
            "SELECT value FROM app_settings WHERE key = ?1",
            params![key],
            |row| row.get(0),
        )
        .optional()
}

fn rebuild_meeting_search_with_connection(connection: &Connection, meeting_id: &str) -> Result<()> {
    let title: Option<String> = connection
        .query_row(
            "SELECT title FROM meetings WHERE id = ?1",
            params![meeting_id],
            |row| row.get(0),
        )
        .optional()?;
    let Some(title) = title else {
        return Ok(());
    };
    let summary: Option<(String, String, String)> = connection
        .query_row(
            r#"
            SELECT overview, action_items_json, topics_json
            FROM meeting_summaries
            WHERE meeting_id = ?1
            "#,
            params![meeting_id],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .optional()?;
    let transcript = collect_transcript_text(connection, meeting_id)?;
    let (summary_text, action_items, topics) =
        summary.unwrap_or_else(|| (String::new(), String::new(), String::new()));

    connection.execute(
        "DELETE FROM meeting_search WHERE meeting_id = ?1",
        params![meeting_id],
    )?;
    connection.execute(
        r#"
        INSERT INTO meeting_search (meeting_id, title, summary, action_items, topics, transcript)
        VALUES (?1, ?2, ?3, ?4, ?5, ?6)
        "#,
        params![
            meeting_id,
            title,
            summary_text,
            action_items,
            topics,
            transcript
        ],
    )?;
    Ok(())
}

fn collect_transcript_text(connection: &Connection, meeting_id: &str) -> Result<String> {
    let mut statement = connection.prepare(
        r#"
        SELECT text
        FROM transcript_segments
        WHERE meeting_id = ?1
        GROUP BY meeting_id, source_kind, speaker_label, language, start_ms, text, provider
        ORDER BY start_ms ASC, source_kind ASC
        "#,
    )?;
    let rows = statement.query_map(params![meeting_id], |row| row.get::<_, String>(0))?;
    let mut parts = Vec::new();
    for row in rows {
        parts.push(row?);
    }
    Ok(parts.join("\n"))
}

fn row_to_meeting(row: &rusqlite::Row<'_>) -> Result<MeetingRecord> {
    Ok(MeetingRecord {
        id: row.get(0)?,
        title: row.get(1)?,
        title_source: row.get(2)?,
        started_at: row.get(3)?,
        ended_at: row.get(4)?,
        status: row.get(5)?,
        language_hint: row.get(6)?,
        summary_language: row.get(7)?,
        archived_at: row.get(8)?,
        created_at: row.get(9)?,
        updated_at: row.get(10)?,
    })
}

fn row_to_meeting_list_item(row: &rusqlite::Row<'_>) -> Result<MeetingListItem> {
    let action_items_json: Option<String> = row.get(9)?;
    let topics_json: Option<String> = row.get(10)?;
    Ok(MeetingListItem {
        id: row.get(0)?,
        title: row.get(1)?,
        title_source: row.get(2)?,
        started_at: row.get(3)?,
        ended_at: row.get(4)?,
        status: row.get(5)?,
        summary_overview: row.get(6)?,
        chunk_count: row.get(7)?,
        segment_count: row.get(8)?,
        action_item_count: json_array_len(action_items_json.as_deref()),
        topic_count: json_array_len(topics_json.as_deref()),
    })
}

fn collect_rows<T>(rows: impl Iterator<Item = Result<T>>) -> Result<Vec<T>> {
    let mut items = Vec::new();
    for row in rows {
        items.push(row?);
    }
    Ok(items)
}

fn json_array_len(raw: Option<&str>) -> usize {
    raw.and_then(|value| serde_json::from_str::<serde_json::Value>(value).ok())
        .and_then(|value| value.as_array().map(Vec::len))
        .unwrap_or(0)
}

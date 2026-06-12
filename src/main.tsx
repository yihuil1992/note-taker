import React from "react";
import ReactDOM from "react-dom/client";
import { createPortal } from "react-dom";
import {
  Activity,
  Archive,
  ChevronDown,
  CheckCircle2,
  Clock3,
  Database,
  Download,
  FileJson,
  FileText,
  Filter,
  FolderOpen,
  Mic,
  MonitorSpeaker,
  Moon,
  Play,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  Square,
  Sun
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

type AppStatus = {
  appDataDir: string;
  databasePath: string;
  recordingsDir: string;
  sidecarDir: string;
  modelsDir: string;
  transcriptionsDir: string;
  summariesDir: string;
  exportsDir: string;
  sidecarConfigured: boolean;
  defaultModel: string;
  rawAudioRetentionDays: number;
  sidecar: SidecarStatus;
  settings: AppSettings;
};

type AppSettings = {
  rawAudioRetentionDays: number;
  transcriptionProvider: string;
  summaryProvider: string;
  localTranscriptionModel: string;
  openaiTranscriptionModel: string;
  languageHint: string;
  summaryLanguage: string;
  recordingConsentReminderDismissed: boolean;
};

type SidecarStatus = {
  executablePath: string;
  executableExists: boolean;
  model: ModelStatus;
  ready: boolean;
};

type ModelStatus = {
  id: string;
  fileName: string;
  path: string;
  url: string;
  expectedSha256: string;
  expectedBytes: number;
  exists: boolean;
  actualBytes?: number | null;
  actualSha256?: string | null;
  verified: boolean;
};

type AudioDevice = {
  id: string;
  name: string;
  kind: "input" | "output";
  isDefault: boolean;
};

type MeetingListItem = {
  id: string;
  title: string;
  titleSource: string;
  startedAt: string;
  endedAt?: string | null;
  status: string;
  summaryOverview?: string | null;
  chunkCount: number;
  segmentCount: number;
  actionItemCount: number;
  topicCount: number;
};

type MeetingDetail = {
  meeting: MeetingRecord;
  chunks: AudioChunkRecord[];
  transcriptSegments: TranscriptSegmentRecord[];
  summary?: MeetingSummaryRecord | null;
};

type MeetingRecord = {
  id: string;
  title: string;
  titleSource: string;
  startedAt: string;
  endedAt?: string | null;
  status: string;
  languageHint: string;
  summaryLanguage: string;
  archivedAt?: string | null;
  createdAt: string;
  updatedAt: string;
};

type AudioChunkRecord = {
  id: string;
  meetingId: string;
  sourceKind: string;
  startedAtMs: number;
  durationMs: number;
  path: string;
  status: string;
  transcriptionError?: string | null;
};

type TranscriptSegmentRecord = {
  id: string;
  meetingId: string;
  sourceKind: string;
  speakerLabel: string;
  language: string;
  startMs: number;
  endMs: number;
  text: string;
  provider: string;
};

type MeetingSummaryRecord = {
  meetingId: string;
  suggestedTitle: string;
  provider: string;
  model: string;
  language: string;
  overview: string;
  decisionsJson: string;
  actionItemsJson: string;
  topicsJson: string;
  risksOrQuestionsJson: string;
  rawJson: string;
  generatedAt: string;
};

type ChunkedMeetingResult = {
  meetingId: string;
  title: string;
  startedAt: string;
  endedAt: string;
  status: string;
  chunkSeconds: number;
  requestedSeconds: number;
  chunks: RecordedChunk[];
};

type RecordedChunk = {
  id: string;
  sourceKind: string;
  startedAtMs: number;
  durationMs: number;
  path: string;
  status: string;
  sampleRate: number;
  channels: number;
  rms: number;
  nonZeroSamples: number;
  error?: string | null;
};

type MeetingTranscriptionResult = {
  meetingId: string;
  status: string;
  provider: string;
  processedChunks: number;
  transcribedChunks: number;
  emptyChunks: number;
  failedChunks: number;
  segments: TranscriptSegmentRecord[];
  failures: Array<{ chunkId: string; sourceKind: string; path: string; error: string }>;
};

type MeetingSummaryResult = {
  meetingId: string;
  suggestedTitle: string;
  provider: string;
  model: string;
  language: string;
  overview: string;
  topics: string[];
  decisions: Array<{ text: string; evidence?: string | null }>;
  actionItems: Array<{ task: string; owner?: string | null; dueDate?: string | null; evidence?: string | null }>;
  openQuestions: Array<{ text: string; evidence?: string | null }>;
  rawJson: string;
};

type ExportResult = {
  meetingId: string;
  format: string;
  path: string;
  bytes: number;
};

type ActiveRecordingStatus = {
  meetingId: string;
  title: string;
  startedAt: string;
  chunkSeconds: number;
  capturedChunks: number;
  stopRequested: boolean;
  workerFinished: boolean;
};

type RecordingStopResult = {
  meetingId: string;
  title: string;
  startedAt: string;
  endedAt: string;
  status: string;
  capturedChunks: number;
};

type RuntimeDownloadResult = { status: SidecarStatus };
type ModelDownloadResult = { model: ModelStatus; downloaded: boolean };
type AtlasMode = "archive" | "night";
type AtlasSelectOption = {
  label: string;
  value: string;
};

const defaultSettings: AppSettings = {
  rawAudioRetentionDays: 7,
  transcriptionProvider: "local-whisper",
  summaryProvider: "codex-cli",
  localTranscriptionModel: "large-v3-turbo",
  openaiTranscriptionModel: "gpt-4o-mini-transcribe",
  languageHint: "zh",
  summaryLanguage: "auto",
  recordingConsentReminderDismissed: false
};
const RECORDING_FAILSAFE_SECONDS = 4 * 60 * 60;
const ATLAS_MODE_STORAGE_KEY = "note-taker-atlas-mode";
const DEFAULT_ATLAS_MODE: AtlasMode = "night";
const RAW_AUDIO_RETENTION_OPTIONS: AtlasSelectOption[] = [
  { value: "0", label: "Delete after review" },
  { value: "7", label: "Keep 7 days" },
  { value: "30", label: "Keep 30 days" },
  { value: "365", label: "Keep 1 year" }
];
const TRANSCRIPTION_PROVIDER_OPTIONS: AtlasSelectOption[] = [
  { value: "local-whisper", label: "Local Whisper sidecar" },
  { value: "openai-api", label: "OpenAI API speech-to-text" }
];
const LOCAL_MODEL_OPTIONS: AtlasSelectOption[] = [
  { value: "large-v3-turbo", label: "large-v3-turbo balanced" },
  { value: "large-v3", label: "large-v3 most accurate" }
];
const OPENAI_TRANSCRIPTION_MODEL_OPTIONS: AtlasSelectOption[] = [
  { value: "gpt-4o-mini-transcribe", label: "gpt-4o-mini-transcribe" },
  { value: "gpt-4o-transcribe", label: "gpt-4o-transcribe" },
  { value: "whisper-1", label: "whisper-1" }
];
const LANGUAGE_HINT_OPTIONS: AtlasSelectOption[] = [
  { value: "zh", label: "Chinese" },
  { value: "auto", label: "Auto detect" },
  { value: "ja", label: "Japanese" },
  { value: "en", label: "English" }
];
const SUMMARY_LANGUAGE_OPTIONS: AtlasSelectOption[] = [
  { value: "auto", label: "Auto" },
  { value: "zh", label: "Chinese" },
  { value: "ja", label: "Japanese" },
  { value: "en", label: "English" }
];
const CHUNK_SECONDS_OPTIONS: AtlasSelectOption[] = [
  { value: "10", label: "Auto smart chunks, recommended" },
  { value: "5", label: "5 seconds" },
  { value: "15", label: "15 seconds" },
  { value: "30", label: "30 seconds" },
  { value: "60", label: "60 seconds" }
];

function readStoredAtlasMode(): AtlasMode {
  if (typeof window === "undefined") return DEFAULT_ATLAS_MODE;
  const stored = window.localStorage.getItem(ATLAS_MODE_STORAGE_KEY);
  return stored === "archive" || stored === "night" ? stored : DEFAULT_ATLAS_MODE;
}

function App() {
  const [atlasMode, setAtlasMode] = React.useState<AtlasMode>(readStoredAtlasMode);
  const [status, setStatus] = React.useState<AppStatus | null>(null);
  const [settings, setSettings] = React.useState<AppSettings>(defaultSettings);
  const [devices, setDevices] = React.useState<AudioDevice[]>([]);
  const [meetings, setMeetings] = React.useState<MeetingListItem[]>([]);
  const [detail, setDetail] = React.useState<MeetingDetail | null>(null);
  const [selectedMeetingId, setSelectedMeetingId] = React.useState<string | null>(null);
  const [query, setQuery] = React.useState("");
  const [chunkSeconds, setChunkSeconds] = React.useState(10);
  const [busy, setBusy] = React.useState<string | null>(null);
  const [error, setError] = React.useState<string | null>(null);
  const [notice, setNotice] = React.useState<string | null>(null);
  const [exportResult, setExportResult] = React.useState<ExportResult | null>(null);
  const [consentAccepted, setConsentAccepted] = React.useState(false);
  const [activeRecording, setActiveRecording] = React.useState<ActiveRecordingStatus | null>(null);

  React.useEffect(() => {
    void refreshAll();
  }, []);

  React.useEffect(() => {
    document.documentElement.dataset.atlasMode = atlasMode;
    window.localStorage.setItem(ATLAS_MODE_STORAGE_KEY, atlasMode);
    return () => {
      delete document.documentElement.dataset.atlasMode;
    };
  }, [atlasMode]);

  React.useEffect(() => {
    if (!activeRecording) return undefined;
    const handle = window.setInterval(async () => {
      try {
        const next = await callBackend<ActiveRecordingStatus | null>("get_active_recording");
        setActiveRecording(next);
      } catch {
        window.clearInterval(handle);
      }
    }, 1500);
    return () => window.clearInterval(handle);
  }, [activeRecording?.meetingId]);

  React.useEffect(() => {
    if (selectedMeetingId) {
      void loadDetail(selectedMeetingId);
    }
  }, [selectedMeetingId]);

  async function refreshAll(activeMeetingId = selectedMeetingId) {
    setError(null);
    try {
      const [nextStatus, nextDevices, nextSettings, nextMeetings, nextRecording] = await Promise.all([
        callBackend<AppStatus>("get_app_status"),
        callBackend<AudioDevice[]>("list_audio_devices"),
        callBackend<AppSettings>("get_app_settings"),
        callBackend<MeetingListItem[]>("list_meetings", { limit: 80 }),
        callBackend<ActiveRecordingStatus | null>("get_active_recording")
      ]);
      setStatus(nextStatus);
      setDevices(nextDevices);
      setSettings(nextSettings);
      setMeetings(nextMeetings);
      setActiveRecording(nextRecording);
      if (activeMeetingId) {
        setSelectedMeetingId(activeMeetingId);
        await loadDetail(activeMeetingId);
      } else if (nextMeetings[0]) {
        setSelectedMeetingId(nextMeetings[0].id);
        await loadDetail(nextMeetings[0].id);
      }
    } catch (refreshError) {
      setError(String(refreshError));
    }
  }

  async function loadDetail(meetingId: string) {
    const nextDetail = await callBackend<MeetingDetail | null>("get_meeting_detail", { meetingId });
    setDetail(nextDetail);
  }

  async function runSearch(nextQuery = query) {
    setError(null);
    try {
      const results = await callBackend<MeetingListItem[]>("search_meetings", {
        query: nextQuery,
        limit: 80
      });
      setMeetings(results);
      if (results[0] && !results.some((meeting) => meeting.id === selectedMeetingId)) {
        setSelectedMeetingId(results[0].id);
      }
    } catch (searchError) {
      setError(String(searchError));
    }
  }

  async function startRecording(consentOverride = false) {
    if (!consentOverride && !settings.recordingConsentReminderDismissed && !consentAccepted) {
      setError("Please acknowledge the lightweight consent reminder before recording.");
      return;
    }
    setBusy("recording");
    setError(null);
    setNotice(null);
    try {
      const result = await callBackend<ActiveRecordingStatus>("start_recording", {
        requestedSeconds: RECORDING_FAILSAFE_SECONDS,
        chunkSeconds
      });
      setSelectedMeetingId(result.meetingId);
      setActiveRecording(result);
      setNotice("Recording started. Stop it when the meeting ends.");
      await refreshAll(result.meetingId);
    } catch (recordError) {
      setError(String(recordError));
    } finally {
      setBusy(null);
    }
  }

  async function stopRecording() {
    setBusy("stopping");
    setError(null);
    try {
      const result = await callBackend<RecordingStopResult>("stop_recording");
      setActiveRecording(null);
      setSelectedMeetingId(result.meetingId);
      setNotice(`Recording stopped with ${result.capturedChunks} audio chunks.`);
      await refreshAll(result.meetingId);
    } catch (stopError) {
      setError(String(stopError));
    } finally {
      setBusy(null);
    }
  }

  async function dismissConsentAndRecord() {
    setConsentAccepted(true);
    const nextSettings = await callBackend<AppSettings>("update_app_setting", {
      key: "recording_consent_reminder_dismissed",
      value: "true"
    });
    setSettings(nextSettings);
    await startRecording(true);
  }

  async function transcribeSelected() {
    if (!selectedMeetingId) return;
    setBusy("transcribing");
    setError(null);
    setNotice("Transcribing this meeting with the current provider.");
    try {
      const result = await callBackend<MeetingTranscriptionResult>("transcribe_meeting_demo", {
        meetingId: selectedMeetingId
      });
      setNotice(`Transcribed ${result.transcribedChunks} chunks, ${result.failedChunks} failed.`);
      await refreshAll(selectedMeetingId);
    } catch (transcribeError) {
      setError(String(transcribeError));
    } finally {
      setBusy(null);
    }
  }

  async function retranscribeSelected() {
    if (!selectedMeetingId) return;
    setBusy("retranscribing");
    setError(null);
    setNotice("Re-transcribing from stored audio. Existing transcript and summary will be refreshed.");
    try {
      const result = await callBackend<MeetingTranscriptionResult>("retranscribe_meeting_demo", {
        meetingId: selectedMeetingId
      });
      setExportResult(null);
      setNotice(`Re-transcribed ${result.transcribedChunks} chunks with current quality settings.`);
      await refreshAll(selectedMeetingId);
    } catch (transcribeError) {
      setError(String(transcribeError));
    } finally {
      setBusy(null);
    }
  }

  async function summarizeSelected() {
    if (!selectedMeetingId) return;
    setBusy("summarizing");
    setError(null);
    try {
      const result = await callBackend<MeetingSummaryResult>("summarize_meeting_demo", {
        meetingId: selectedMeetingId
      });
      setNotice(`Generated summary: ${result.suggestedTitle}`);
      await refreshAll(selectedMeetingId);
    } catch (summaryError) {
      setError(String(summaryError));
    } finally {
      setBusy(null);
    }
  }

  async function exportSelected(format: "markdown" | "json") {
    if (!selectedMeetingId) return;
    setBusy(`export-${format}`);
    setError(null);
    try {
      const command = format === "markdown" ? "export_meeting_as_markdown" : "export_meeting_as_json";
      const result = await callBackend<ExportResult>(command, { meetingId: selectedMeetingId });
      setExportResult(result);
      setNotice(`Exported ${format.toUpperCase()} to ${result.path}`);
    } catch (exportError) {
      setError(String(exportError));
    } finally {
      setBusy(null);
    }
  }

  async function archiveSelected() {
    if (!selectedMeetingId || !detail) return;
    if (activeRecording?.meetingId === selectedMeetingId) {
      setError("Stop the active recording before archiving this meeting.");
      return;
    }
    const archivedTitle = detail.meeting.title;
    setBusy("archiving");
    setError(null);
    try {
      await callBackend<void>("archive_meeting", { meetingId: selectedMeetingId });
      setSelectedMeetingId(null);
      setDetail(null);
      setExportResult(null);
      setNotice(`Archived "${archivedTitle}". It is hidden from the meeting index.`);
      await refreshAll(null);
    } catch (archiveError) {
      setError(String(archiveError));
    } finally {
      setBusy(null);
    }
  }

  async function downloadRuntime() {
    setBusy("runtime");
    setError(null);
    try {
      await callBackend<RuntimeDownloadResult>("download_default_sidecar_runtime");
      await refreshAll();
    } catch (runtimeError) {
      setError(String(runtimeError));
    } finally {
      setBusy(null);
    }
  }

  async function downloadModel() {
    setBusy("model");
    setError(null);
    try {
      await callBackend<ModelDownloadResult>("download_default_transcription_model");
      await refreshAll();
    } catch (modelError) {
      setError(String(modelError));
    } finally {
      setBusy(null);
    }
  }

  async function updateSetting(key: string, value: string) {
    setError(null);
    const nextSettings = await callBackend<AppSettings>("update_app_setting", { key, value });
    setSettings(nextSettings);
    await refreshAll();
  }

  const inputDevice = devices.find((device) => device.kind === "input" && device.isDefault) ?? devices.find((device) => device.kind === "input");
  const outputDevice = devices.find((device) => device.kind === "output" && device.isDefault) ?? devices.find((device) => device.kind === "output");
  const setupReady = settings.transcriptionProvider === "openai-api" || Boolean(status?.sidecar.ready);
  const selectedMeeting = detail?.meeting;
  const atlasModeLabel = atlasMode === "archive" ? "Archive Sheet" : "Night Atlas";

  return (
    <main className="atlas-shell">
      <header className="atlas-header">
        <a className="atlas-brand" href="#today" aria-label="Note Taker home">
          <span className="brand-mark"><Activity size={20} aria-hidden="true" /></span>
          <span>
            <strong>Note Taker</strong>
            <small>Local meeting memory</small>
          </span>
        </a>
        <nav className="atlas-nav" aria-label="Primary navigation">
          <a className="nav-item active" href="#today"><Clock3 size={16} aria-hidden="true" /> Capture</a>
          <a className="nav-item" href="#meetings"><Database size={16} aria-hidden="true" /> Archive</a>
          <a className="nav-item" href="#settings"><Settings size={16} aria-hidden="true" /> Instruments</a>
        </nav>
        <div className="atlas-header-status">
          <span className={setupReady ? "status-dot ready" : "status-dot"} />
          <span>{settings.transcriptionProvider === "openai-api" ? "Cloud STT selected" : setupReady ? "Local stack ready" : "Setup needed"}</span>
          <button
            className="theme-toggle"
            type="button"
            onClick={() => setAtlasMode((mode) => (mode === "archive" ? "night" : "archive"))}
            aria-label={`Current theme: ${atlasModeLabel}. Toggle theme.`}
            title={atlasModeLabel}
          >
            {atlasMode === "archive" ? <Sun size={15} aria-hidden="true" /> : <Moon size={15} aria-hidden="true" />}
            <span>{atlasModeLabel}</span>
          </button>
          <button className="ghost-action" type="button" onClick={() => void refreshAll()} aria-label="Refresh status">
            <RefreshCw size={15} aria-hidden="true" />
          </button>
        </div>
      </header>

      <div className="atlas-grid">
        <section id="meetings" className="atlas-archive">
          <div className="atlas-panel-heading">
            <div>
              <p className="section-label">Archive</p>
              <h2>Meeting index</h2>
            </div>
            <span className="count-pill">{meetings.length}</span>
          </div>
          <div className="rail-search-row">
            <label className="search-box">
              <Search size={16} aria-hidden="true" />
              <input
                value={query}
                onChange={(event) => {
                  setQuery(event.target.value);
                  void runSearch(event.target.value);
                }}
                placeholder="Search meetings"
              />
            </label>
            <button className="icon-button" type="button" onClick={() => void runSearch(query)} aria-label="Filter meetings">
              <Filter size={17} aria-hidden="true" />
            </button>
          </div>
          <div className="meeting-list">
            {meetings.length === 0 ? (
              <EmptyState title="No meetings yet" text="Record a meeting to create searchable local notes." />
            ) : (
              meetings.map((meeting) => (
                <button
                  className={meeting.id === selectedMeetingId ? "meeting-list-item selected" : "meeting-list-item"}
                  key={meeting.id}
                  type="button"
                  onClick={() => setSelectedMeetingId(meeting.id)}
                >
                  <span className="meeting-time">{formatDateTime(meeting.startedAt)}</span>
                  <strong>{meeting.title}</strong>
                  {meeting.summaryOverview ? <p>{meeting.summaryOverview}</p> : <p>{formatStatus(meeting.status)}</p>}
                  <small>{meeting.segmentCount} segments · {meeting.actionItemCount} actions</small>
                </button>
              ))
            )}
          </div>
        </section>

        <section id="today" className="atlas-observatory">
          <div className="atlas-notices">
            {error ? <div className="error-banner">{error}</div> : null}
            {notice ? <div className="notice-banner">{notice}</div> : null}
          </div>

          <RecordingPanel
            busy={busy}
            activeRecording={activeRecording}
            chunkSeconds={chunkSeconds}
            consentRequired={!settings.recordingConsentReminderDismissed && !consentAccepted}
            inputDevice={inputDevice}
            outputDevice={outputDevice}
            onRecord={() => void startRecording(false)}
            onStop={() => void stopRecording()}
            onConsentRecord={() => void dismissConsentAndRecord()}
            onChunkSecondsChange={setChunkSeconds}
          />

          {settings.transcriptionProvider === "local-whisper" && !setupReady ? (
            <section className="setup-strip">
              <div>
                <p className="section-label">First-run setup</p>
                <h3>Install Local Whisper before transcribing.</h3>
              </div>
              <div className="setup-actions">
                <button className="secondary-action" type="button" onClick={() => void downloadRuntime()} disabled={busy === "runtime" || status?.sidecar.executableExists}>
                  <Download size={16} aria-hidden="true" />
                  {status?.sidecar.executableExists ? "Runtime installed" : busy === "runtime" ? "Downloading..." : "Download runtime"}
                </button>
                <button className="secondary-action" type="button" onClick={() => void downloadModel()} disabled={busy === "model" || status?.sidecar.model.verified}>
                  <Download size={16} aria-hidden="true" />
                  {status?.sidecar.model.verified ? "Model verified" : busy === "model" ? "Downloading..." : `Download ${settings.localTranscriptionModel}`}
                </button>
              </div>
            </section>
          ) : null}

          {selectedMeeting && detail ? (
            <MeetingDetailView
              detail={detail}
              busy={busy}
              exportResult={exportResult}
              onTranscribe={() => void transcribeSelected()}
              onRetranscribe={() => void retranscribeSelected()}
              onSummarize={() => void summarizeSelected()}
              onArchive={() => void archiveSelected()}
              onExportMarkdown={() => void exportSelected("markdown")}
              onExportJson={() => void exportSelected("json")}
              onOpenExports={() => void callBackend<void>("open_exports_folder")}
            />
          ) : (
            <section className="panel empty-detail">
              <EmptyState title="Pick or record a meeting" text="The meeting page will show summary, transcript, chunks, and exports." />
            </section>
          )}
        </section>

        <aside id="settings" className="atlas-instruments">
          <section className="panel local-ai-panel">
            <div className="panel-title-row">
              <div>
                <p className="section-label">Local AI</p>
                <h3>{setupReady ? "Ready" : "Setup needed"}</h3>
              </div>
              <ChevronDown size={18} aria-hidden="true" />
            </div>
            <DataRow label="Local Whisper" value={status?.sidecar.executableExists ? "Runtime installed" : "Runtime missing"} tone={status?.sidecar.executableExists ? "ready" : "warn"} />
            <DataRow label="Model" value={status?.sidecar.model.verified ? `${status.sidecar.model.id} verified` : `${settings.localTranscriptionModel} missing`} tone={status?.sidecar.model.verified ? "ready" : "warn"} />
            <DataRow label="Codex" value={`${settings.summaryProvider} · gpt-5.4`} tone="ready" />
            <DataRow label="OpenAI STT" value={settings.transcriptionProvider === "openai-api" ? settings.openaiTranscriptionModel : "Optional"} />
            <button className="secondary-action sidecar-folder-action" type="button" onClick={() => void callBackend<void>("open_sidecar_folder")} aria-label="Open sidecar folder">
              <FolderOpen size={16} aria-hidden="true" />
              Open folder
            </button>
          </section>

          <section className="panel settings-panel">
            <div className="panel-title-row">
              <div>
                <p className="section-label">Settings</p>
                <h3>Defaults</h3>
              </div>
              <ChevronDown size={18} aria-hidden="true" />
            </div>
            <div className="field">
              <span>Raw audio retention</span>
              <AtlasSelect value={String(settings.rawAudioRetentionDays)} options={RAW_AUDIO_RETENTION_OPTIONS} onChange={(value) => void updateSetting("raw_audio_retention_days", value)} />
            </div>
            <div className="field">
              <span>Transcription provider</span>
              <AtlasSelect value={settings.transcriptionProvider} options={TRANSCRIPTION_PROVIDER_OPTIONS} onChange={(value) => void updateSetting("transcription_provider", value)} />
            </div>
            <div className="field">
              <span>Local model</span>
              <AtlasSelect
                value={settings.localTranscriptionModel}
                options={LOCAL_MODEL_OPTIONS}
                disabled={settings.transcriptionProvider !== "local-whisper"}
                onChange={(value) => void updateSetting("local_transcription_model", value)}
              />
            </div>
            <div className="field">
              <span>OpenAI model</span>
              <AtlasSelect
                value={settings.openaiTranscriptionModel}
                options={OPENAI_TRANSCRIPTION_MODEL_OPTIONS}
                disabled={settings.transcriptionProvider !== "openai-api"}
                onChange={(value) => void updateSetting("openai_transcription_model", value)}
              />
            </div>
            <div className="field">
              <span>Transcription language</span>
              <AtlasSelect value={settings.languageHint} options={LANGUAGE_HINT_OPTIONS} onChange={(value) => void updateSetting("language_hint", value)} />
            </div>
            <div className="field">
              <span>Summary language</span>
              <AtlasSelect value={settings.summaryLanguage} options={SUMMARY_LANGUAGE_OPTIONS} onChange={(value) => void updateSetting("summary_language", value)} />
            </div>
          </section>
        </aside>
      </div>

      <footer className="atlas-footer">
        <span className="privacy-dot" />
        <strong>Local only</strong>
        <span>{settings.transcriptionProvider === "openai-api" ? "Cloud speech-to-text selected" : "All meeting data stays on this device by default"}</span>
        <code>{status?.appDataDir ?? "Initializing app data"}</code>
      </footer>
    </main>
  );
}

function RecordingPanel({
  busy,
  activeRecording,
  chunkSeconds,
  consentRequired,
  inputDevice,
  outputDevice,
  onRecord,
  onStop,
  onConsentRecord,
  onChunkSecondsChange
}: {
  busy: string | null;
  activeRecording: ActiveRecordingStatus | null;
  chunkSeconds: number;
  consentRequired: boolean;
  inputDevice?: AudioDevice;
  outputDevice?: AudioDevice;
  onRecord: () => void;
  onStop: () => void;
  onConsentRecord: () => void;
  onChunkSecondsChange: (seconds: number) => void;
}) {
  return (
    <section id="record" className="capture-console">
      <div className="source-grid">
        <SourceTile icon={<Mic size={23} aria-hidden="true" />} title="Microphone" value={inputDevice?.name ?? "No input device found"} state="On" />
        <SourceTile icon={<MonitorSpeaker size={23} aria-hidden="true" />} title="Computer audio" value={outputDevice?.name ?? "Default render endpoint"} state="On" />
      </div>

      <div className="capture-actions">
        <button
          className={activeRecording ? "danger-action record-button" : "primary-action record-button"}
          type="button"
          onClick={activeRecording ? onStop : onRecord}
          disabled={busy === "recording" || busy === "stopping"}
        >
          {activeRecording ? <Square size={17} aria-hidden="true" /> : <Play size={17} aria-hidden="true" />}
          {activeRecording ? (busy === "stopping" ? "Stopping..." : "Stop recording") : busy === "recording" ? "Starting..." : "Start recording"}
          <ChevronDown size={17} aria-hidden="true" />
        </button>
        <p>Recordings are saved locally and never leave this device unless you choose a cloud provider.</p>
        <div className="field compact-field">
          <span>Capture chunks</span>
          <AtlasSelect value={String(chunkSeconds)} options={CHUNK_SECONDS_OPTIONS} onChange={(value) => onChunkSecondsChange(Number(value))} />
        </div>
      </div>

      {activeRecording ? (
        <div className="active-recording">
          <strong>{activeRecording.title}</strong>
          <span>Started {formatDateTime(activeRecording.startedAt)} · {activeRecording.capturedChunks} chunks captured · {activeRecording.chunkSeconds}s cadence</span>
        </div>
      ) : null}

      {consentRequired ? (
        <div className="consent-box">
          <ShieldCheck size={18} aria-hidden="true" />
          <p>Confirm that your meeting participants understand this session may be recorded and transcribed locally.</p>
          <button type="button" onClick={onConsentRecord}>I have consent, record</button>
        </div>
      ) : null}

    </section>
  );
}

function MeetingDetailView({
  detail,
  busy,
  exportResult,
  onTranscribe,
  onRetranscribe,
  onSummarize,
  onArchive,
  onExportMarkdown,
  onExportJson,
  onOpenExports
}: {
  detail: MeetingDetail;
  busy: string | null;
  exportResult: ExportResult | null;
  onTranscribe: () => void;
  onRetranscribe: () => void;
  onSummarize: () => void;
  onArchive: () => void;
  onExportMarkdown: () => void;
  onExportJson: () => void;
  onOpenExports: () => void;
}) {
  const summary = detail.summary;
  const topics = parseStringArray(summary?.topicsJson);
  const decisions = parseObjectArray(summary?.decisionsJson, "text");
  const actionItems = parseActionItems(summary?.actionItemsJson);
  const questions = parseObjectArray(summary?.risksOrQuestionsJson, "text");
  const duration = detail.meeting.endedAt
    ? `${Math.max(1, Math.round((new Date(detail.meeting.endedAt).getTime() - new Date(detail.meeting.startedAt).getTime()) / 60000))}m`
    : "In progress";

  return (
    <section className="detail-surface session-record">
      <div className="meeting-heading session-record-heading">
        <div>
          <p className="section-label">Session record</p>
          <h2>{detail.meeting.title}</h2>
          <p>{formatDateTime(detail.meeting.startedAt)} · {duration} · {formatStatus(detail.meeting.status)}</p>
        </div>
        <div className="action-row">
          <span className="meeting-status">{formatStatus(detail.meeting.status)}</span>
          <button className="secondary-action" type="button" onClick={onArchive} disabled={busy === "archiving"} title="Hide this meeting from the index. Local files are kept.">
            <Archive size={16} aria-hidden="true" />
            {busy === "archiving" ? "Archiving..." : "Archive"}
          </button>
          <button className="secondary-action" type="button" onClick={onSummarize} disabled={busy === "summarizing" || detail.transcriptSegments.length === 0}>
            <Sparkles size={16} aria-hidden="true" />
            {busy === "summarizing" ? "Summarizing..." : "Summarize"}
          </button>
        </div>
      </div>

      <div className="metric-grid session-index">
        <Metric label="Chunks" value={String(detail.chunks.length)} />
        <Metric label="Segments" value={String(detail.transcriptSegments.length)} />
        <Metric label="Actions" value={String(actionItems.length)} />
        <Metric label="Duration" value={duration} />
      </div>

      <div className="detail-content-grid">
        <section className="panel section-panel summary-panel">
          <div className="panel-title-row">
            <div>
              <p className="section-label">Summary</p>
              <h3>{summary?.suggestedTitle ?? "No summary yet"}</h3>
            </div>
            {summary ? <span className="provider-pill">Generated by {summary.provider}</span> : null}
          </div>
          {summary ? (
            <>
              <p className="summary-overview">{summary.overview}</p>
              <SummarySection title="Topics" items={topics} />
              <SummarySection title="Decisions" items={decisions} />
              <SummarySection title="Action items" items={actionItems} checkable />
              <SummarySection title="Open questions" items={questions} />
            </>
          ) : (
            <EmptyState title="Ready for Codex" text="Generate a summary after transcript segments exist." />
          )}
        </section>

        <section className="panel section-panel transcript-panel">
          <div className="panel-title-row">
            <div>
              <p className="section-label">Transcript</p>
              <h3>Source-attributed segments</h3>
            </div>
            <div className="compact-actions">
              <button
                className="secondary-action icon-only tooltip-action"
                type="button"
                onClick={onTranscribe}
                disabled={busy === "transcribing" || detail.chunks.length === 0}
                aria-label="Transcribe meeting"
                data-tooltip="Transcribe stored audio"
                title="Transcribe stored audio"
              >
                <FileJson size={16} aria-hidden="true" />
              </button>
              <button
                className="secondary-action icon-only tooltip-action"
                type="button"
                onClick={onRetranscribe}
                disabled={busy === "retranscribing" || detail.chunks.length === 0}
                aria-label="Re-transcribe meeting"
                data-tooltip="Re-transcribe and replace"
                title="Re-transcribe and replace"
              >
                <RefreshCw size={16} aria-hidden="true" />
              </button>
            </div>
          </div>
          <div className="transcript-list">
            {detail.transcriptSegments.length === 0 ? (
              <EmptyState title="No transcript yet" text="Run transcription after recording finishes." />
            ) : (
              detail.transcriptSegments.map((segment) => (
                <article className="transcript-row" key={segment.id}>
                  <span>{formatTimestamp(segment.startMs)}</span>
                  <p>{segment.text}</p>
                  <small>{segment.speakerLabel} · {segment.provider}</small>
                </article>
              ))
            )}
          </div>
        </section>
      </div>

      <section className="panel export-panel">
        <div className="panel-title-row">
          <div>
            <p className="section-label">Export</p>
            <h3>Markdown and JSON</h3>
          </div>
          <div className="action-row">
            <button className="secondary-action" type="button" onClick={onExportMarkdown} disabled={busy === "export-markdown"}>
              <FileText size={16} aria-hidden="true" />
              Markdown
            </button>
            <button className="secondary-action" type="button" onClick={onExportJson} disabled={busy === "export-json"}>
              <FileJson size={16} aria-hidden="true" />
              JSON
            </button>
          </div>
        </div>
        {exportResult ? (
          <div className="export-result">
            <code>{exportResult.path}</code>
            <button className="ghost-action" type="button" onClick={onOpenExports}>Open exports folder</button>
          </div>
        ) : (
          <p className="muted">Exports are written under the app data exports directory.</p>
        )}
      </section>
    </section>
  );
}

function SourceTile({ icon, title, value, state }: { icon: React.ReactNode; title: string; value: string; state?: string }) {
  return (
    <article className="source-tile">
      <span>{icon}</span>
      <div>
        <strong>{title}</strong>
        <p>{state ? <><i /> {state}</> : value}</p>
        {state ? <small>{value}</small> : null}
      </div>
    </article>
  );
}

function AtlasSelect({
  value,
  options,
  disabled = false,
  onChange
}: {
  value: string;
  options: AtlasSelectOption[];
  disabled?: boolean;
  onChange: (value: string) => void;
}) {
  const [open, setOpen] = React.useState(false);
  const [menuStyle, setMenuStyle] = React.useState<React.CSSProperties>({});
  const rootRef = React.useRef<HTMLDivElement | null>(null);
  const menuRef = React.useRef<HTMLDivElement | null>(null);
  const triggerRef = React.useRef<HTMLButtonElement | null>(null);
  const listboxId = React.useId();
  const selectedOption = options.find((option) => option.value === value) ?? options[0];

  const updateMenuPosition = React.useCallback(() => {
    const trigger = triggerRef.current;
    if (!trigger) return;
    const rect = trigger.getBoundingClientRect();
    const menuHeight = Math.min(260, options.length * 34 + 12);
    const spaceBelow = window.innerHeight - rect.bottom;
    const top = spaceBelow < menuHeight + 10 && rect.top > menuHeight
      ? rect.top - menuHeight - 6
      : rect.bottom + 6;
    setMenuStyle({
      left: rect.left,
      top,
      width: rect.width
    });
  }, [options.length]);

  React.useEffect(() => {
    if (!open) return undefined;
    updateMenuPosition();

    function handlePointerDown(event: PointerEvent) {
      const target = event.target as Node;
      if (
        rootRef.current &&
        !rootRef.current.contains(target) &&
        !menuRef.current?.contains(target)
      ) {
        setOpen(false);
      }
    }

    function handleKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") {
        setOpen(false);
      }
    }

    function handleReposition() {
      updateMenuPosition();
    }

    document.addEventListener("pointerdown", handlePointerDown);
    document.addEventListener("keydown", handleKeyDown);
    window.addEventListener("resize", handleReposition);
    window.addEventListener("scroll", handleReposition, true);
    return () => {
      document.removeEventListener("pointerdown", handlePointerDown);
      document.removeEventListener("keydown", handleKeyDown);
      window.removeEventListener("resize", handleReposition);
      window.removeEventListener("scroll", handleReposition, true);
    };
  }, [open, updateMenuPosition]);

  function chooseOption(nextValue: string) {
    onChange(nextValue);
    setOpen(false);
  }

  function handleButtonKeyDown(event: React.KeyboardEvent<HTMLButtonElement>) {
    if (event.key === "ArrowDown" || event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      if (!disabled) setOpen(true);
    }
  }

  return (
    <div className={open ? "atlas-select open" : "atlas-select"} ref={rootRef}>
      <button
        aria-controls={listboxId}
        aria-expanded={open}
        aria-haspopup="listbox"
        className="atlas-select-trigger"
        disabled={disabled}
        onClick={() => setOpen((current) => !current)}
        onKeyDown={handleButtonKeyDown}
        ref={triggerRef}
        type="button"
      >
        <span>{selectedOption?.label ?? value}</span>
        <ChevronDown size={16} aria-hidden="true" />
      </button>
      {open ? createPortal((
        <div className="atlas-select-menu" id={listboxId} role="listbox" aria-label={selectedOption?.label ?? "Select option"} ref={menuRef} style={menuStyle}>
          {options.map((option) => {
            const selected = option.value === value;
            return (
              <button
                aria-selected={selected}
                className={selected ? "atlas-select-option selected" : "atlas-select-option"}
                key={option.value}
                onClick={() => chooseOption(option.value)}
                role="option"
                type="button"
              >
                <span>{option.label}</span>
                {selected ? <CheckCircle2 size={14} aria-hidden="true" /> : null}
              </button>
            );
          })}
        </div>
      ), document.body) : null}
    </div>
  );
}

function DataRow({ label, value, tone }: { label: string; value: string; tone?: "ready" | "warn" }) {
  return (
    <div className={tone ? `data-row ${tone}` : "data-row"}>
      <span>{label}</span>
      <code>{value}</code>
      {tone ? <i aria-hidden="true" /> : null}
    </div>
  );
}

function Metric({ label, value }: { label: string; value: string }) {
  return (
    <div className="metric">
      <span>{label}</span>
      <strong>{value}</strong>
    </div>
  );
}

function SummarySection({ title, items, checkable = false }: { title: string; items: string[]; checkable?: boolean }) {
  if (items.length === 0) return null;
  return (
    <div className="summary-section">
      <h4>{title}</h4>
      <ul>
        {items.map((item) => (
          <li key={item}>{checkable ? "[ ] " : ""}{item}</li>
        ))}
      </ul>
    </div>
  );
}

function EmptyState({ title, text }: { title: string; text: string }) {
  return (
    <div className="empty-state">
      <CheckCircle2 size={18} aria-hidden="true" />
      <strong>{title}</strong>
      <p>{text}</p>
    </div>
  );
}

function parseStringArray(raw?: string | null): string[] {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string") : [];
  } catch {
    return [];
  }
}

function parseObjectArray(raw: string | undefined | null, field: string): string[] {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    return Array.isArray(value)
      ? value.map((item) => item?.[field]).filter((item): item is string => typeof item === "string")
      : [];
  } catch {
    return [];
  }
}

function parseActionItems(raw?: string | null): string[] {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    if (!Array.isArray(value)) return [];
    return value.map((item) => {
      const owner = item.owner ? ` · ${item.owner}` : "";
      const dueDate = item.dueDate ? ` · ${item.dueDate}` : "";
      return `${item.task}${owner}${dueDate}`;
    });
  } catch {
    return [];
  }
}

function formatStatus(status: string): string {
  return status.replace(/_/g, " ");
}

function formatTimestamp(ms: number): string {
  const seconds = Math.floor(ms / 1000);
  const minutes = Math.floor(seconds / 60);
  return `${minutes}:${String(seconds % 60).padStart(2, "0")}`;
}

function formatDateTime(value: string): string {
  const date = new Date(value);
  if (Number.isNaN(date.getTime())) return value;
  return new Intl.DateTimeFormat(undefined, {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit"
  }).format(date);
}

async function callBackend<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  if (window.__TAURI_INTERNALS__) {
    return invoke<T>(command, args);
  }
  return mockBackend<T>(command, args);
}

let mockRuntimeInstalled = true;
let mockModelVerified = true;
let mockSettings: AppSettings = { ...defaultSettings, recordingConsentReminderDismissed: true };
let mockActiveRecording: ActiveRecordingStatus | null = null;
let mockMeetings: MeetingListItem[] = [
  {
    id: "meeting-1",
    title: "Local transcription smoke review",
    titleSource: "ai_generated",
    startedAt: "2026-06-11T14:00:00Z",
    endedAt: "2026-06-11T14:14:00Z",
    status: "summarized",
    summaryOverview: "Validated local chunk recording, Whisper transcription, Codex summary, and export flow.",
    chunkCount: 8,
    segmentCount: 4,
    actionItemCount: 1,
    topicCount: 3
  }
];
let mockDetails: Record<string, MeetingDetail> = {
  "meeting-1": makeMockDetail("meeting-1", "Local transcription smoke review", "summarized")
};

async function mockBackend<T>(command: string, args?: Record<string, unknown>): Promise<T> {
  await new Promise((resolve) => window.setTimeout(resolve, command.includes("download") ? 400 : 140));
  if (command === "get_app_status") {
    const sidecar = mockSidecar(mockModelVerified, mockRuntimeInstalled);
    return {
      appDataDir: "C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker",
      databasePath: "C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\note-taker.sqlite3",
      recordingsDir: "C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\recordings",
      sidecarDir: "C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\sidecars",
      modelsDir: "C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\models",
      transcriptionsDir: "C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\transcriptions",
      summariesDir: "C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\summaries",
      exportsDir: "C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\exports",
      sidecarConfigured: sidecar.ready,
      defaultModel: mockSettings.localTranscriptionModel,
      rawAudioRetentionDays: mockSettings.rawAudioRetentionDays,
      sidecar,
      settings: mockSettings
    } as T;
  }
  if (command === "get_app_settings") return mockSettings as T;
  if (command === "get_active_recording") return mockActiveRecording as T;
  if (command === "update_app_setting") {
    const key = String(args?.key ?? "");
    const value = String(args?.value ?? "");
    mockSettings = {
      ...mockSettings,
      ...(key === "raw_audio_retention_days" ? { rawAudioRetentionDays: Number(value) } : {}),
      ...(key === "transcription_provider" ? { transcriptionProvider: value } : {}),
      ...(key === "local_transcription_model" ? { localTranscriptionModel: value } : {}),
      ...(key === "openai_transcription_model" ? { openaiTranscriptionModel: value } : {}),
      ...(key === "language_hint" ? { languageHint: value } : {}),
      ...(key === "summary_language" ? { summaryLanguage: value } : {}),
      ...(key === "recording_consent_reminder_dismissed" ? { recordingConsentReminderDismissed: value === "true" } : {})
    };
    return mockSettings as T;
  }
  if (command === "list_audio_devices") {
    return [
      { id: "input-0", name: "Default Microphone", kind: "input", isDefault: true },
      { id: "output-0", name: "Default Speakers", kind: "output", isDefault: true }
    ] as T;
  }
  if (command === "list_meetings") return mockMeetings as T;
  if (command === "search_meetings") {
    const query = String(args?.query ?? "").toLowerCase();
    return mockMeetings.filter((meeting) => `${meeting.title} ${meeting.summaryOverview ?? ""}`.toLowerCase().includes(query)) as T;
  }
  if (command === "get_meeting_detail") {
    return (mockDetails[String(args?.meetingId ?? "")] ?? null) as T;
  }
  if (command === "archive_meeting") {
    const id = String(args?.meetingId ?? "");
    if (mockDetails[id]) {
      mockDetails[id].meeting.archivedAt = new Date().toISOString();
    }
    mockMeetings = mockMeetings.filter((meeting) => meeting.id !== id);
    return undefined as T;
  }
  if (command === "start_recording") {
    const id = `meeting-${mockMeetings.length + 1}`;
    const title = `Meeting ${new Date().toLocaleString()}`;
    const detail = makeMockDetail(id, title, "recorded", false);
    detail.meeting.status = "recording";
    detail.chunks = [];
    detail.transcriptSegments = [];
    mockDetails[id] = detail;
    mockMeetings = [toListItem(detail), ...mockMeetings];
    mockActiveRecording = {
      meetingId: id,
      title,
      startedAt: detail.meeting.startedAt,
      chunkSeconds: Number(args?.chunkSeconds ?? 30),
      capturedChunks: 0,
      stopRequested: false,
      workerFinished: false
    };
    return mockActiveRecording as T;
  }
  if (command === "stop_recording") {
    if (!mockActiveRecording) throw new Error("No recording is running");
    const id = mockActiveRecording.meetingId;
    const detail = mockDetails[id];
    detail.chunks = makeMockDetail(id, detail.meeting.title, "recorded", false).chunks;
    detail.meeting.status = "recorded";
    detail.meeting.endedAt = new Date().toISOString();
    const result = {
      meetingId: id,
      title: detail.meeting.title,
      startedAt: detail.meeting.startedAt,
      endedAt: detail.meeting.endedAt,
      status: "recorded",
      capturedChunks: detail.chunks.length
    };
    mockMeetings = mockMeetings.map((meeting) => meeting.id === id ? toListItem(detail) : meeting);
    mockActiveRecording = null;
    return result as T;
  }
  if (command === "transcribe_meeting_demo") {
    const id = String(args?.meetingId ?? "");
    const detail = mockDetails[id];
    detail.transcriptSegments = makeSegments(id);
    detail.meeting.status = "transcribed";
    mockMeetings = mockMeetings.map((meeting) => meeting.id === id ? toListItem(detail) : meeting);
    return {
      meetingId: id,
      status: "transcribed",
      provider: mockSettings.transcriptionProvider === "openai-api" ? `openai-api:${mockSettings.openaiTranscriptionModel}` : "local-whisper",
      processedChunks: detail.chunks.length,
      transcribedChunks: detail.transcriptSegments.length,
      emptyChunks: 0,
      failedChunks: 0,
      segments: detail.transcriptSegments,
      failures: []
    } as T;
  }
  if (command === "retranscribe_meeting_demo") {
    const id = String(args?.meetingId ?? "");
    const detail = mockDetails[id];
    detail.summary = null;
    detail.transcriptSegments = makeSegments(id).map((segment, index) => ({
      ...segment,
      id: `${id}-segment-retry-${index + 1}`
    }));
    detail.meeting.status = "transcribed";
    mockMeetings = mockMeetings.map((meeting) => meeting.id === id ? toListItem(detail) : meeting);
    return {
      meetingId: id,
      status: "transcribed",
      provider: mockSettings.transcriptionProvider === "openai-api" ? `openai-api:${mockSettings.openaiTranscriptionModel}` : "local-whisper",
      processedChunks: detail.chunks.length,
      transcribedChunks: detail.transcriptSegments.length,
      emptyChunks: 0,
      failedChunks: 0,
      segments: detail.transcriptSegments,
      failures: []
    } as T;
  }
  if (command === "summarize_meeting_demo") {
    const id = String(args?.meetingId ?? "");
    const detail = mockDetails[id];
    detail.summary = makeSummary(id);
    detail.meeting.title = detail.summary.suggestedTitle;
    detail.meeting.status = "summarized";
    mockMeetings = mockMeetings.map((meeting) => meeting.id === id ? toListItem(detail) : meeting);
    return {
      meetingId: id,
      suggestedTitle: detail.summary.suggestedTitle,
      provider: "codex-cli",
      model: "gpt-5.4",
      language: "zh-CN",
      overview: detail.summary.overview,
      topics: parseStringArray(detail.summary.topicsJson),
      decisions: [{ text: "Keep the local-first sidecar transcription path.", evidence: null }],
      actionItems: [{ task: "Add a proper stop-recording worker.", owner: null, dueDate: null, evidence: null }],
      openQuestions: [],
      rawJson: "{}"
    } as T;
  }
  if (command === "export_meeting_as_markdown" || command === "export_meeting_as_json") {
    return {
      meetingId: String(args?.meetingId ?? ""),
      format: command.endsWith("json") ? "json" : "markdown",
      path: `C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\exports\\meeting.${command.endsWith("json") ? "json" : "md"}`,
      bytes: 2048
    } as T;
  }
  if (command === "download_default_sidecar_runtime") {
    mockRuntimeInstalled = true;
    return { status: mockSidecar(mockModelVerified, true) } as T;
  }
  if (command === "download_default_transcription_model") {
    mockModelVerified = true;
    return { downloaded: true, model: mockSidecar(true, mockRuntimeInstalled).model } as T;
  }
  if (command === "open_sidecar_folder" || command === "open_exports_folder") return undefined as T;
  throw new Error(`Unsupported mock command: ${command}`);
}

function makeMockDetail(id: string, title: string, status: string, withSummary = true): MeetingDetail {
  const now = new Date().toISOString();
  const detail: MeetingDetail = {
    meeting: {
      id,
      title,
      titleSource: withSummary ? "ai_generated" : "datetime_placeholder",
      startedAt: now,
      endedAt: now,
      status,
      languageHint: mockSettings.languageHint,
      summaryLanguage: "auto",
      createdAt: now,
      updatedAt: now
    },
    chunks: Array.from({ length: 6 }, (_, index) => ({
      id: `${id}-chunk-${index}`,
      meetingId: id,
      sourceKind: index % 2 === 0 ? "microphone" : "system",
      startedAtMs: Math.floor(index / 2) * 15000,
      durationMs: 15000,
      path: `C:\\recordings\\${id}\\chunk-${index}.wav`,
      status: "transcribed",
      transcriptionError: null
    })),
    transcriptSegments: makeSegments(id),
    summary: withSummary ? makeSummary(id) : null
  };
  return detail;
}

function makeSegments(id: string): TranscriptSegmentRecord[] {
  const provider = mockSettings.transcriptionProvider === "openai-api"
    ? `openai-api:${mockSettings.openaiTranscriptionModel}`
    : "local-whisper";
  return [
    {
      id: `${id}-segment-1`,
      meetingId: id,
      sourceKind: "microphone",
      speakerLabel: "Me",
      language: "auto",
      startMs: 0,
      endMs: 12000,
      text: "We verified that microphone and computer audio are captured as separate local chunks.",
      provider
    },
    {
      id: `${id}-segment-2`,
      meetingId: id,
      sourceKind: "system",
      speakerLabel: "Others",
      language: "auto",
      startMs: 15000,
      endMs: 28000,
      text: "The next step is making the meeting history, search, summary, and export flow usable every day.",
      provider
    }
  ];
}

function makeSummary(id: string): MeetingSummaryRecord {
  return {
    meetingId: id,
    suggestedTitle: "Local transcription smoke review",
    provider: "codex-cli",
    model: "gpt-5.4",
    language: "zh-CN",
    overview: "本次会议验证了本地录音、分段转录、Codex 总结和导出流程。可推断的负责人和截止日期保持为空，避免臆造。",
    topicsJson: JSON.stringify(["Local recording", "Whisper transcription", "Codex summary"]),
    decisionsJson: JSON.stringify([{ text: "Keep the local-first sidecar transcription path.", evidence: null }]),
    actionItemsJson: JSON.stringify([{ task: "Add a proper stop-recording worker.", owner: null, dueDate: null, evidence: null }]),
    risksOrQuestionsJson: JSON.stringify([]),
    rawJson: "{}",
    generatedAt: new Date().toISOString()
  };
}

function toListItem(detail: MeetingDetail): MeetingListItem {
  const actionItemCount = parseActionItems(detail.summary?.actionItemsJson).length;
  const topicCount = parseStringArray(detail.summary?.topicsJson).length;
  return {
    id: detail.meeting.id,
    title: detail.meeting.title,
    titleSource: detail.meeting.titleSource,
    startedAt: detail.meeting.startedAt,
    endedAt: detail.meeting.endedAt,
    status: detail.meeting.status,
    summaryOverview: detail.summary?.overview ?? null,
    chunkCount: detail.chunks.length,
    segmentCount: detail.transcriptSegments.length,
    actionItemCount,
    topicCount
  };
}

function mockSidecar(modelVerified: boolean, runtimeInstalled: boolean): SidecarStatus {
  return {
    executablePath: "C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\sidecars\\whisper-cli.exe",
    executableExists: runtimeInstalled,
      ready: runtimeInstalled && modelVerified,
    model: {
      id: mockSettings.localTranscriptionModel,
      fileName: mockSettings.localTranscriptionModel === "large-v3" ? "ggml-large-v3.bin" : "ggml-large-v3-turbo.bin",
      path: `C:\\Users\\you\\AppData\\Roaming\\com.yihui.notetaker\\models\\${mockSettings.localTranscriptionModel === "large-v3" ? "ggml-large-v3.bin" : "ggml-large-v3-turbo.bin"}`,
      url: `https://huggingface.co/ggerganov/whisper.cpp/resolve/main/${mockSettings.localTranscriptionModel === "large-v3" ? "ggml-large-v3.bin" : "ggml-large-v3-turbo.bin"}`,
      expectedSha256: mockSettings.localTranscriptionModel === "large-v3" ? "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2" : "5a4b65b05933d70ce9d5aa6265eb128fa5eba38f6fee40836fdedc4d2fde42ad",
      expectedBytes: mockSettings.localTranscriptionModel === "large-v3" ? 3095033483 : 1624555275,
      exists: modelVerified,
      actualBytes: modelVerified ? (mockSettings.localTranscriptionModel === "large-v3" ? 3095033483 : 1624555275) : null,
      actualSha256: modelVerified ? (mockSettings.localTranscriptionModel === "large-v3" ? "64d182b440b98d5203c4f9bd541544d84c605196c4f7b845dfa11fb23594d1e2" : "5a4b65b05933d70ce9d5aa6265eb128fa5eba38f6fee40836fdedc4d2fde42ad") : null,
      verified: modelVerified
    }
  };
}

ReactDOM.createRoot(document.getElementById("root") as HTMLElement).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>
);

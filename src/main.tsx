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
  ExternalLink,
  FileJson,
  FileText,
  Filter,
  FolderOpen,
  KeyRound,
  Mic,
  MonitorSpeaker,
  Moon,
  Play,
  Plus,
  RefreshCw,
  Search,
  Settings,
  ShieldCheck,
  Sparkles,
  Square,
  Sun,
  Trash2
} from "lucide-react";
import { invoke } from "@tauri-apps/api/core";
import "./styles.css";

declare global {
  interface Window {
    __TAURI_INTERNALS__?: unknown;
  }
}

type AppStatus = {
  appVersion: string;
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
  summaryModel: string;
  localTranscriptionModel: string;
  openaiTranscriptionModel: string;
  languageHint: string;
  summaryLanguage: string;
  customGlossary: string;
  recordingConsentReminderDismissed: boolean;
};

type GlossaryEntry = {
  id: string;
  term: string;
  description: string;
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
  summaryOutline: SummaryOutlineSection[];
  structuredNotes: StructuredSummaryNote[];
  detailedNotes: DetailedSummaryNote[];
  rawJson: string;
};

type CancelMeetingTaskResult = {
  meetingId: string;
  cancelRequested: boolean;
  status: string;
};

type MeetingTaskStatus = {
  meetingId: string;
  kind: string;
  phase: string;
  message: string;
  current: number;
  total?: number | null;
  percent?: number | null;
  cancelRequested: boolean;
};

type SummaryOutlineSection = {
  title: string;
  summary: string;
  items: SummaryOutlineItem[];
};

type SummaryOutlineItem = {
  title: string;
  summary: string;
  detail: string;
  evidence?: string | null;
  decisions: string[];
  actionItems: string[];
  openQuestions: string[];
};

type DetailedSummaryNote = {
  title: string;
  detail: string;
  evidence?: string | null;
};

type StructuredSummaryNote = {
  title: string;
  category: string;
  summary: string;
  detail: string;
  evidence?: string | null;
  decisions: string[];
  actionItems: string[];
  openQuestions: string[];
};

type ExportResult = {
  meetingId: string;
  format: string;
  path: string;
  bytes: number;
};

type AppUpdateCheck = {
  currentVersion: string;
  latestVersion?: string | null;
  updateAvailable: boolean;
  installable?: boolean;
  releaseName?: string | null;
  releaseUrl?: string | null;
  publishedAt?: string | null;
  notes?: string | null;
};

type AppUpdateProgress = {
  downloadedBytes: number;
  contentLength?: number;
  phase: "downloading" | "installing" | "restarting";
};

type OpenAiApiKeyStatus = {
  hasKey: boolean;
  source: string;
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
  summaryModel: "gpt-5.4",
  localTranscriptionModel: "large-v3-turbo",
  openaiTranscriptionModel: "gpt-4o-mini-transcribe",
  languageHint: "zh",
  summaryLanguage: "auto",
  customGlossary: "",
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
const SUMMARY_MODEL_OPTIONS: AtlasSelectOption[] = [
  { value: "gpt-5.4", label: "gpt-5.4" },
  { value: "gpt-5.4-mini", label: "gpt-5.4-mini" },
  { value: "gpt-5.5", label: "gpt-5.5" }
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
  const [taskStatuses, setTaskStatuses] = React.useState<Record<string, MeetingTaskStatus>>({});
  const [updateCheck, setUpdateCheck] = React.useState<AppUpdateCheck | null>(null);
  const [updateCheckStatus, setUpdateCheckStatus] = React.useState<"idle" | "checking" | "failed">("idle");
  const [updateProgress, setUpdateProgress] = React.useState<AppUpdateProgress | null>(null);
  const [glossaryEntries, setGlossaryEntries] = React.useState<GlossaryEntry[]>(() => parseGlossaryEntries(defaultSettings.customGlossary));
  const [expandedGlossaryEntryId, setExpandedGlossaryEntryId] = React.useState<string | null>(null);
  const [openaiApiKeyStatus, setOpenaiApiKeyStatus] = React.useState<OpenAiApiKeyStatus | null>(null);
  const [openaiApiKeyDraft, setOpenaiApiKeyDraft] = React.useState("");
  const glossaryDraft = React.useMemo(() => serializeGlossaryEntries(glossaryEntries), [glossaryEntries]);
  const savedGlossaryDraft = React.useMemo(() => serializeGlossaryEntries(parseGlossaryEntries(settings.customGlossary)), [settings.customGlossary]);
  const glossaryEntryCount = glossaryEntries.filter((entry) => entry.term.trim() || entry.description.trim()).length;
  const pendingUpdateRef = React.useRef<unknown>(null);

  React.useEffect(() => {
    void refreshAll();
    void checkForUpdates(true);
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

  React.useEffect(() => {
    void refreshTaskStatuses();
    const handle = window.setInterval(() => {
      void refreshTaskStatuses();
    }, 1200);
    return () => window.clearInterval(handle);
  }, []);

  React.useEffect(() => {
    setGlossaryEntries(parseGlossaryEntries(settings.customGlossary));
    setExpandedGlossaryEntryId(null);
  }, [settings.customGlossary]);

  async function refreshAll(activeMeetingId = selectedMeetingId) {
    setError(null);
    try {
      const [nextStatus, nextDevices, nextSettings, nextMeetings, nextRecording, nextTaskStatuses, nextOpenAiKeyStatus] = await Promise.all([
        callBackend<AppStatus>("get_app_status"),
        callBackend<AudioDevice[]>("list_audio_devices"),
        callBackend<AppSettings>("get_app_settings"),
        callBackend<MeetingListItem[]>("list_meetings", { limit: 80 }),
        callBackend<ActiveRecordingStatus | null>("get_active_recording"),
        callBackend<MeetingTaskStatus[]>("list_meeting_task_statuses"),
        callBackend<OpenAiApiKeyStatus>("get_openai_api_key_status")
      ]);
      setStatus(nextStatus);
      setDevices(nextDevices);
      setSettings(nextSettings);
      setMeetings(nextMeetings);
      setActiveRecording(nextRecording);
      setTaskStatuses(indexTaskStatuses(nextTaskStatuses));
      setOpenaiApiKeyStatus(nextOpenAiKeyStatus);
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

  async function refreshTaskStatuses() {
    try {
      const statuses = await callBackend<MeetingTaskStatus[]>("list_meeting_task_statuses");
      setTaskStatuses(indexTaskStatuses(statuses));
    } catch {
      // Task progress is best-effort and should never interrupt capture or review.
    }
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
      setNotice(result.status === "transcription_cancelled"
        ? `Transcription stopped after ${result.transcribedChunks} chunks.`
        : `Transcribed ${result.transcribedChunks} chunks, ${result.failedChunks} failed.`);
      if (result.status !== "transcription_cancelled") {
        void notifyTaskComplete("Transcription complete", `${result.transcribedChunks} chunks transcribed.`);
      }
      await refreshAll(selectedMeetingId);
    } catch (transcribeError) {
      const message = String(transcribeError);
      if (isCancellationMessage(message)) {
        setNotice("Transcription stopped.");
      } else {
        setError(message);
      }
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
      setNotice(result.status === "transcription_cancelled"
        ? `Re-transcription stopped after ${result.transcribedChunks} chunks.`
        : `Re-transcribed ${result.transcribedChunks} chunks with current quality settings.`);
      if (result.status !== "transcription_cancelled") {
        void notifyTaskComplete("Re-transcription complete", `${result.transcribedChunks} chunks transcribed.`);
      }
      await refreshAll(selectedMeetingId);
    } catch (transcribeError) {
      const message = String(transcribeError);
      if (isCancellationMessage(message)) {
        setNotice("Re-transcription stopped.");
      } else {
        setError(message);
      }
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
      void notifyTaskComplete("Summary complete", result.suggestedTitle);
      await refreshAll(selectedMeetingId);
    } catch (summaryError) {
      const message = String(summaryError);
      if (isCancellationMessage(message)) {
        setNotice("Summary generation stopped.");
      } else {
        setError(message);
      }
    } finally {
      setBusy(null);
    }
  }

  async function cancelSelectedTask(meetingId = selectedMeetingId) {
    if (!meetingId) return;
    setBusy("canceling");
    setError(null);
    try {
      const result = await callBackend<CancelMeetingTaskResult>("cancel_meeting_task", { meetingId });
      setNotice(result.cancelRequested ? "Stop requested. Current task is winding down." : "No active worker was found. The meeting was marked stopped.");
      await refreshAll(meetingId);
    } catch (cancelError) {
      setError(String(cancelError));
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

  async function saveGlossary() {
    await updateSetting("custom_glossary", glossaryDraft);
    setNotice("Glossary saved for transcription and summaries.");
  }

  async function saveOpenAiApiKey() {
    setBusy("openai-key");
    setError(null);
    try {
      const status = await callBackend<OpenAiApiKeyStatus>("save_openai_api_key", {
        apiKey: openaiApiKeyDraft
      });
      setOpenaiApiKeyStatus(status);
      setOpenaiApiKeyDraft("");
      setNotice("OpenAI API key saved to Windows Credential Manager.");
    } catch (keyError) {
      setError(String(keyError));
    } finally {
      setBusy(null);
    }
  }

  async function clearOpenAiApiKey() {
    setBusy("openai-key");
    setError(null);
    try {
      const status = await callBackend<OpenAiApiKeyStatus>("clear_openai_api_key");
      setOpenaiApiKeyStatus(status);
      setOpenaiApiKeyDraft("");
      setNotice(status.hasKey ? "Environment OpenAI API key is still active." : "Stored OpenAI API key cleared.");
    } catch (keyError) {
      setError(String(keyError));
    } finally {
      setBusy(null);
    }
  }

  function addGlossaryEntry() {
    const entry = createBlankGlossaryEntry();
    setGlossaryEntries((entries) => [...entries, entry]);
    setExpandedGlossaryEntryId(entry.id);
  }

  function updateGlossaryEntry(id: string, patch: Partial<Omit<GlossaryEntry, "id">>) {
    setGlossaryEntries((entries) => entries.map((entry) => entry.id === id ? { ...entry, ...patch } : entry));
  }

  function removeGlossaryEntry(id: string) {
    setGlossaryEntries((entries) => entries.filter((entry) => entry.id !== id));
    setExpandedGlossaryEntryId((current) => current === id ? null : current);
  }

  function toggleGlossaryEntry(id: string) {
    setExpandedGlossaryEntryId((current) => current === id ? null : id);
  }

  async function checkForUpdates(silent = false) {
    setUpdateCheckStatus("checking");
    try {
      const result = window.__TAURI_INTERNALS__
        ? await checkTauriUpdater()
        : await callBackend<AppUpdateCheck>("check_for_app_update");
      setUpdateCheck(result);
      setUpdateCheckStatus("idle");
      if (!silent && !result.updateAvailable) {
        setNotice(`Note Taker ${result.currentVersion} is up to date.`);
      }
    } catch {
      setUpdateCheckStatus("failed");
      if (!silent) {
        setNotice("Could not check GitHub releases right now.");
      }
    }
  }

  async function checkTauriUpdater(): Promise<AppUpdateCheck> {
    const { getVersion } = await import("@tauri-apps/api/app");
    const { check } = await import("@tauri-apps/plugin-updater");
    const currentVersion = await getVersion();
    const update = await check();
    pendingUpdateRef.current = update;
    if (!update) {
      return {
        currentVersion,
        latestVersion: null,
        updateAvailable: false,
        installable: false
      };
    }
    return {
      currentVersion: update.currentVersion,
      latestVersion: update.version,
      updateAvailable: true,
      installable: true,
      releaseName: `Note Taker ${update.version}`,
      releaseUrl: "https://github.com/yihuil1992/note-taker/releases/latest",
      publishedAt: update.date ?? null,
      notes: update.body ?? null
    };
  }

  async function installAvailableUpdate() {
    if (!updateCheck?.updateAvailable) return;
    setBusy("install-update");
    setError(null);
    setUpdateProgress({ downloadedBytes: 0, phase: "downloading" });
    try {
      if (!window.__TAURI_INTERNALS__) {
        await new Promise((resolve) => window.setTimeout(resolve, 650));
        setUpdateProgress({ downloadedBytes: 1, contentLength: 1, phase: "restarting" });
        setNotice("Preview mode simulated the signed updater install flow.");
        return;
      }

      const update = pendingUpdateRef.current as {
        downloadAndInstall: (handler: (event: { event: string; data?: { contentLength?: number; chunkLength?: number } }) => void) => Promise<void>;
      } | null;
      if (!update) {
        throw new Error("No signed update is ready to install. Check again first.");
      }

      let downloadedBytes = 0;
      let contentLength: number | undefined;
      await update.downloadAndInstall((event) => {
        if (event.event === "Started") {
          contentLength = event.data?.contentLength;
          downloadedBytes = 0;
          setUpdateProgress({ downloadedBytes, contentLength, phase: "downloading" });
        } else if (event.event === "Progress") {
          downloadedBytes += event.data?.chunkLength ?? 0;
          setUpdateProgress({ downloadedBytes, contentLength, phase: "downloading" });
        } else if (event.event === "Finished") {
          setUpdateProgress({ downloadedBytes, contentLength, phase: "installing" });
        }
      });

      setUpdateProgress({ downloadedBytes, contentLength, phase: "restarting" });
      const { relaunch } = await import("@tauri-apps/plugin-process");
      await relaunch();
    } catch (installError) {
      setError(String(installError));
      setUpdateProgress(null);
    } finally {
      setBusy(null);
    }
  }

  async function openUpdateRelease() {
    if (!updateCheck?.releaseUrl) return;
    setBusy("open-release");
    try {
      await callBackend<void>("open_url", { url: updateCheck.releaseUrl });
    } catch (openError) {
      setError(String(openError));
    } finally {
      setBusy(null);
    }
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
            <small>Meeting memory</small>
          </span>
        </a>
        <nav className="atlas-nav" aria-label="Primary navigation">
          <a className="nav-item active" href="#today"><Clock3 size={16} aria-hidden="true" /> Capture</a>
          <a className="nav-item" href="#meetings"><Database size={16} aria-hidden="true" /> Archive</a>
          <a className="nav-item" href="#settings"><Settings size={16} aria-hidden="true" /> Instruments</a>
        </nav>
        <div className="atlas-header-status">
          <span className={setupReady ? "status-dot ready" : "status-dot"} />
          <span>{settings.transcriptionProvider === "openai-api" ? "Cloud transcription selected" : setupReady ? "Local stack ready" : "Setup needed"}</span>
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
              <EmptyState title="No meetings yet" text="Record a meeting to create searchable notes." />
            ) : (
              meetings.map((meeting) => {
                const taskStatus = taskStatuses[meeting.id];
                const meetingTaskActive = isTaskActiveStatus(meeting.status) || Boolean(taskStatus);
                return (
                  <article
                    className={[
                      "meeting-list-item",
                      meeting.id === selectedMeetingId ? "selected" : "",
                      meetingTaskActive ? "has-task" : ""
                    ].filter(Boolean).join(" ")}
                    key={meeting.id}
                  >
                    <button
                      className="meeting-list-item-main"
                      type="button"
                      onClick={() => setSelectedMeetingId(meeting.id)}
                    >
                      <span className="meeting-time">{formatDateTime(meeting.startedAt)}</span>
                      <strong>{meeting.title}</strong>
                      {meeting.summaryOverview ? <p>{meeting.summaryOverview}</p> : <p>{formatStatus(meeting.status)}</p>}
                      {taskStatus ? <TaskProgress status={taskStatus} compact /> : null}
                      <small>{meeting.segmentCount} segments · {meeting.actionItemCount} actions</small>
                    </button>
                    {meetingTaskActive ? (
                      <button
                        className="meeting-list-stop"
                        type="button"
                        onClick={() => void cancelSelectedTask(meeting.id)}
                        disabled={busy === "canceling"}
                        aria-label={`Stop task for ${meeting.title}`}
                        title="Stop current task"
                      >
                        <Square size={12} aria-hidden="true" />
                        Stop
                      </button>
                    ) : null}
                  </article>
                );
              })
            )}
          </div>
        </section>

        <section id="today" className="atlas-observatory">
          <div className="atlas-notices">
            {error ? <div className="error-banner">{error}</div> : null}
            {notice ? <div className="notice-banner">{notice}</div> : null}
            {updateCheck?.updateAvailable ? (
              <UpdateNotice
                update={updateCheck}
                busy={busy === "install-update" || busy === "open-release"}
                progress={updateProgress}
                onInstall={() => void installAvailableUpdate()}
                onOpenRelease={() => void openUpdateRelease()}
              />
            ) : null}
          </div>

          <RecordingPanel
            busy={busy}
            activeRecording={activeRecording}
            chunkSeconds={chunkSeconds}
            consentRequired={!settings.recordingConsentReminderDismissed && !consentAccepted}
            inputDevice={inputDevice}
            outputDevice={outputDevice}
            cloudTranscriptionSelected={settings.transcriptionProvider === "openai-api"}
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
              taskStatus={taskStatuses[detail.meeting.id] ?? null}
              exportResult={exportResult}
              onTranscribe={() => void transcribeSelected()}
              onRetranscribe={() => void retranscribeSelected()}
              onSummarize={() => void summarizeSelected()}
              onCancelTask={() => void cancelSelectedTask()}
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
            <DataRow label="Codex" value={`${settings.summaryProvider} · ${settings.summaryModel}`} tone="ready" />
            <DataRow
              label="OpenAI transcription"
              value={openaiApiKeyStatus?.hasKey ? `${settings.openaiTranscriptionModel} key ready` : settings.transcriptionProvider === "openai-api" ? `${settings.openaiTranscriptionModel} needs key` : "Optional"}
              tone={openaiApiKeyStatus?.hasKey ? "ready" : settings.transcriptionProvider === "openai-api" ? "warn" : undefined}
            />
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
              <span>Whisper model</span>
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
            <div className="field openai-key-field">
              <span>OpenAI API key</span>
              <div className="secret-input-row">
                <div className="secret-input-wrap">
                  <KeyRound size={14} aria-hidden="true" />
                  <input
                    type="password"
                    value={openaiApiKeyDraft}
                    placeholder={openaiApiKeyStatus?.hasKey ? "Stored in Windows Credential Manager" : "Paste API key"}
                    autoComplete="off"
                    spellCheck={false}
                    onChange={(event) => setOpenaiApiKeyDraft(event.target.value)}
                    aria-label="OpenAI API key"
                  />
                </div>
                <button
                  className="icon-action"
                  type="button"
                  onClick={() => void saveOpenAiApiKey()}
                  disabled={busy === "openai-key" || !openaiApiKeyDraft.trim()}
                  aria-label="Save OpenAI API key"
                  title="Save OpenAI API key"
                  data-tooltip="Save key"
                >
                  <CheckCircle2 size={16} aria-hidden="true" />
                </button>
                <button
                  className="icon-action"
                  type="button"
                  onClick={() => void clearOpenAiApiKey()}
                  disabled={busy === "openai-key" || !openaiApiKeyStatus?.hasKey}
                  aria-label="Clear OpenAI API key"
                  title="Clear OpenAI API key"
                  data-tooltip="Clear key"
                >
                  <Trash2 size={16} aria-hidden="true" />
                </button>
              </div>
              <small className={openaiApiKeyStatus?.hasKey ? "secret-status ready" : "secret-status warn"}>
                {openaiApiKeyStatus?.hasKey
                  ? openaiApiKeyStatus.source === "environment"
                    ? "Using OPENAI_API_KEY from this process."
                    : "Stored securely in Windows Credential Manager."
                  : "Required only when OpenAI API speech-to-text is selected."}
              </small>
            </div>
            <div className="field">
              <span>Transcription language</span>
              <AtlasSelect value={settings.languageHint} options={LANGUAGE_HINT_OPTIONS} onChange={(value) => void updateSetting("language_hint", value)} />
            </div>
            <div className="field">
              <span>Summary language</span>
              <AtlasSelect value={settings.summaryLanguage} options={SUMMARY_LANGUAGE_OPTIONS} onChange={(value) => void updateSetting("summary_language", value)} />
            </div>
            <div className="field">
              <span>Codex model</span>
              <AtlasSelect value={settings.summaryModel} options={SUMMARY_MODEL_OPTIONS} onChange={(value) => void updateSetting("summary_model", value)} />
            </div>
          </section>

          <section className="panel glossary-panel">
            <div className="panel-title-row glossary-heading">
              <div>
                <p className="section-label">Glossary</p>
                <h3>Terms and context</h3>
                <small>{glossaryEntryCount === 0 ? "No terms yet" : `${glossaryEntryCount} terms`}</small>
              </div>
              <button className="ghost-action glossary-add" type="button" onClick={addGlossaryEntry}>
                <Plus size={14} aria-hidden="true" />
                Add term
              </button>
            </div>
            <div className="glossary-editor">
              {glossaryEntries.length === 0 ? (
                <div className="glossary-empty">
                  <strong>Add names, acronyms, and internal terms.</strong>
                  <span>These hints are passed to transcription and summaries.</span>
                </div>
              ) : (
                <div className="glossary-list">
                  {glossaryEntries.map((entry, index) => {
                    const isExpanded = expandedGlossaryEntryId === entry.id;
                    const termLabel = entry.term.trim() || "Untitled term";
                    return (
                      <div className={isExpanded ? "glossary-entry expanded" : "glossary-entry"} key={entry.id}>
                        <button
                          className="glossary-entry-summary"
                          type="button"
                          onClick={() => toggleGlossaryEntry(entry.id)}
                          aria-expanded={isExpanded}
                          aria-controls={`glossary-entry-detail-${entry.id}`}
                        >
                          <span className={entry.term.trim() ? "glossary-term-name" : "glossary-term-name empty"}>{termLabel}</span>
                          <span className="glossary-entry-tools">
                            {isExpanded ? (
                              <span
                                className="glossary-remove-inline"
                                role="button"
                                tabIndex={0}
                                aria-label={`Remove glossary entry ${entry.term || index + 1}`}
                                title="Remove term"
                                onClick={(event) => {
                                  event.stopPropagation();
                                  removeGlossaryEntry(entry.id);
                                }}
                                onKeyDown={(event) => {
                                  if (event.key !== "Enter" && event.key !== " ") return;
                                  event.preventDefault();
                                  event.stopPropagation();
                                  removeGlossaryEntry(entry.id);
                                }}
                              >
                                <Trash2 size={14} aria-hidden="true" />
                              </span>
                            ) : null}
                            <ChevronDown size={15} aria-hidden="true" />
                          </span>
                        </button>
                        {isExpanded ? (
                          <div className="glossary-entry-detail" id={`glossary-entry-detail-${entry.id}`}>
                            <label>
                              <span>Term</span>
                              <input
                                value={entry.term}
                                maxLength={160}
                                placeholder={index === 0 ? "RAG" : "Term"}
                                onChange={(event) => updateGlossaryEntry(entry.id, { term: event.target.value })}
                              />
                            </label>
                            <label>
                              <span>Explanation</span>
                              <input
                                value={entry.description}
                                maxLength={500}
                                placeholder={index === 0 ? "retrieval augmented generation" : "Meaning, pronunciation, or context"}
                                onChange={(event) => updateGlossaryEntry(entry.id, { description: event.target.value })}
                              />
                            </label>
                          </div>
                        ) : null}
                      </div>
                    );
                  })}
                </div>
              )}
              <small className={glossaryDraft.length > 12000 ? "glossary-limit over" : "glossary-limit"}>{glossaryDraft.length}/12000</small>
            </div>
            <button
              className="secondary-action full-width"
              type="button"
              disabled={glossaryDraft === savedGlossaryDraft || glossaryDraft.length > 12000}
              onClick={() => void saveGlossary()}
            >
              <CheckCircle2 size={15} aria-hidden="true" />
              Save glossary
            </button>
          </section>
        </aside>
      </div>

      <footer className="atlas-footer">
        <span className="privacy-dot" />
        <strong>{settings.transcriptionProvider === "openai-api" ? "Cloud transcription on" : "Local by default"}</strong>
        <span className="version-pill">v{status?.appVersion ?? "0.2.3"}</span>
        <span>{settings.transcriptionProvider === "openai-api" ? "Audio windows are sent to OpenAI for transcription." : "Audio and transcripts stay on this device unless you choose cloud transcription."}</span>
        <button className="ghost-action" type="button" onClick={() => void checkForUpdates(false)} disabled={updateCheckStatus === "checking"}>
          <RefreshCw size={14} aria-hidden="true" />
          {updateCheckStatus === "checking" ? "Checking" : updateCheck?.latestVersion ? `Latest ${updateCheck.latestVersion}` : "Check updates"}
        </button>
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
  cloudTranscriptionSelected,
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
  cloudTranscriptionSelected: boolean;
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
        <p>
          {cloudTranscriptionSelected
            ? "Recording files are saved locally. Transcription sends audio windows to OpenAI."
            : "Recording files and transcription stay on this device unless you choose cloud transcription."}
        </p>
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
          <p>
            {cloudTranscriptionSelected
              ? "Confirm that participants understand this session may be recorded and sent to OpenAI for transcription."
              : "Confirm that participants understand this session may be recorded and transcribed locally."}
          </p>
          <button type="button" onClick={onConsentRecord}>I have consent, record</button>
        </div>
      ) : null}

    </section>
  );
}

function UpdateNotice({
  update,
  busy,
  progress,
  onInstall,
  onOpenRelease
}: {
  update: AppUpdateCheck;
  busy: boolean;
  progress: AppUpdateProgress | null;
  onInstall: () => void;
  onOpenRelease: () => void;
}) {
  const releaseLabel = update.releaseName ?? update.latestVersion ?? "New release";
  const progressPercent = progress?.contentLength
    ? Math.min(100, Math.round((progress.downloadedBytes / progress.contentLength) * 100))
    : progress
      ? 12
      : 0;
  const actionLabel = progress?.phase === "installing"
    ? "Installing"
    : progress?.phase === "restarting"
      ? "Restarting"
      : busy
        ? "Downloading"
        : update.installable
          ? "Update and restart"
          : "Open release";
  return (
    <section className="update-notice" aria-label="Application update available">
      <div className="update-notice-mark">
        <Download size={18} aria-hidden="true" />
      </div>
      <div className="update-notice-copy">
        <p className="section-label">Update available</p>
        <h3>{releaseLabel}</h3>
        <p>
          Current {update.currentVersion}
          {update.latestVersion ? ` · Latest ${update.latestVersion}` : ""}
          {update.publishedAt ? ` · ${formatDateTime(update.publishedAt)}` : ""}
        </p>
        {update.notes ? <small>{update.notes}</small> : null}
        {progress ? (
          <div className="update-progress" aria-label={`Update ${progress.phase}`}>
            <span style={{ width: `${progressPercent}%` }} />
          </div>
        ) : null}
      </div>
      <button className="secondary-action" type="button" onClick={update.installable ? onInstall : onOpenRelease} disabled={busy}>
        {update.installable ? <Download size={16} aria-hidden="true" /> : <ExternalLink size={16} aria-hidden="true" />}
        {actionLabel}
      </button>
      {update.installable && update.releaseUrl ? (
        <button className="ghost-action update-release-link" type="button" onClick={onOpenRelease} disabled={busy}>
          <ExternalLink size={14} aria-hidden="true" />
          Release notes
        </button>
      ) : null}
    </section>
  );
}

function TaskProgress({ status, compact = false }: { status: MeetingTaskStatus; compact?: boolean }) {
  const percent = typeof status.percent === "number" ? Math.max(0, Math.min(100, status.percent)) : null;
  const label = formatTaskKind(status.kind);
  const count = status.total ? `${status.current}/${status.total}` : status.phase;
  return (
    <div className={compact ? "task-progress compact" : "task-progress"} aria-label={`${label} progress`}>
      <div className="task-progress-copy">
        <strong>{label}</strong>
        <span>{status.cancelRequested ? "Stopping..." : status.message}</span>
        <small>{percent !== null ? `${percent}% · ${count}` : count}</small>
      </div>
      <div className={percent === null ? "task-progress-bar indeterminate" : "task-progress-bar"}>
        <span style={percent === null ? undefined : { width: `${percent}%` }} />
      </div>
    </div>
  );
}

function MeetingDetailView({
  detail,
  busy,
  taskStatus,
  exportResult,
  onTranscribe,
  onRetranscribe,
  onSummarize,
  onCancelTask,
  onArchive,
  onExportMarkdown,
  onExportJson,
  onOpenExports
}: {
  detail: MeetingDetail;
  busy: string | null;
  taskStatus: MeetingTaskStatus | null;
  exportResult: ExportResult | null;
  onTranscribe: () => void;
  onRetranscribe: () => void;
  onSummarize: () => void;
  onCancelTask: () => void;
  onArchive: () => void;
  onExportMarkdown: () => void;
  onExportJson: () => void;
  onOpenExports: () => void;
}) {
  const summary = detail.summary;
  const actionItems = parseActionItems(summary?.actionItemsJson);
  const summaryOutline = parseSummaryOutline(summary?.rawJson);
  const duration = detail.meeting.endedAt
    ? `${Math.max(1, Math.round((new Date(detail.meeting.endedAt).getTime() - new Date(detail.meeting.startedAt).getTime()) / 60000))}m`
    : "In progress";
  const taskActive = isTaskActiveStatus(detail.meeting.status) || isTaskBusy(busy) || Boolean(taskStatus);

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
          {taskActive ? (
            <button className="danger-action" type="button" onClick={onCancelTask} disabled={busy === "canceling"}>
              <Square size={16} aria-hidden="true" />
              {busy === "canceling" ? "Stopping..." : "Stop task"}
            </button>
          ) : null}
          <button className="secondary-action" type="button" onClick={onArchive} disabled={busy === "archiving" || taskActive} title="Hide this meeting from the index. Local files are kept.">
            <Archive size={16} aria-hidden="true" />
            {busy === "archiving" ? "Archiving..." : "Archive"}
          </button>
          <button className="secondary-action" type="button" onClick={onSummarize} disabled={taskActive || detail.transcriptSegments.length === 0}>
            <Sparkles size={16} aria-hidden="true" />
            {busy === "summarizing" ? "Summarizing..." : "Summarize"}
          </button>
        </div>
      </div>

      {taskStatus ? <TaskProgress status={taskStatus} /> : null}

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
          <div className="summary-scroll">
            {summary ? (
              <>
                <p className="summary-overview">{summary.overview}</p>
                <SummaryOutline sections={summaryOutline} />
              </>
            ) : (
              <EmptyState title="Ready for Codex" text="Generate a summary after transcript segments exist." />
            )}
          </div>
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
                disabled={taskActive || detail.chunks.length === 0}
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
                disabled={taskActive || detail.chunks.length === 0}
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

function SummaryOutline({ sections }: { sections: SummaryOutlineSection[] }) {
  if (sections.length === 0) return null;
  return (
    <div className="summary-section summary-outline">
      <h4>Meeting record</h4>
      <div className="summary-outline-list">
        {sections.map((section) => (
          <section className="summary-outline-section" key={`${section.title}-${section.summary}`}>
            <div className="summary-outline-section-heading">
              <strong>{section.title}</strong>
              {section.summary ? <small>{section.summary}</small> : null}
            </div>
            <div className="summary-outline-items">
              {section.items.map((item) => (
                <details className="summary-outline-item" key={`${section.title}-${item.title}-${item.detail}`}>
                  <summary>
                    <span className="summary-outline-item-heading">
                      <strong>{item.title}</strong>
                      <small>{item.summary}</small>
                    </span>
                    <ChevronDown size={15} aria-hidden="true" />
                  </summary>
                  <div className="summary-outline-item-body">
                    <p>{item.detail}</p>
                    {item.decisions.length > 0 ? <InlineNoteList title="Decisions" items={item.decisions} /> : null}
                    {item.actionItems.length > 0 ? <InlineNoteList title="Actions" items={item.actionItems} checkable /> : null}
                    {item.openQuestions.length > 0 ? <InlineNoteList title="Open questions" items={item.openQuestions} /> : null}
                    {item.evidence ? <small>{item.evidence}</small> : null}
                  </div>
                </details>
              ))}
            </div>
          </section>
        ))}
      </div>
    </div>
  );
}

function InlineNoteList({ title, items, checkable = false }: { title: string; items: string[]; checkable?: boolean }) {
  return (
    <div className="inline-note-list">
      <h5>{title}</h5>
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

function parseSummaryOutline(raw?: string | null): SummaryOutlineSection[] {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    if (Array.isArray(value?.summaryOutline)) {
      const sections = value.summaryOutline
        .map((item: unknown) => normalizeSummaryOutlineSection(item))
        .filter((item: SummaryOutlineSection | null): item is SummaryOutlineSection => item !== null);
      if (sections.length > 0) return sections;
    }
  } catch {
    return [];
  }

  const legacyNotes = parseStructuredNotes(raw);
  if (legacyNotes.length === 0) return [];
  const grouped = new Map<string, SummaryOutlineItem[]>();
  for (const note of legacyNotes) {
    const sectionTitle = formatSectionTitle(note.category);
    const items = grouped.get(sectionTitle) ?? [];
    items.push({
      title: note.title,
      summary: note.summary,
      detail: note.detail,
      evidence: note.evidence,
      decisions: note.decisions,
      actionItems: note.actionItems,
      openQuestions: note.openQuestions
    });
    grouped.set(sectionTitle, items);
  }
  return Array.from(grouped.entries()).map(([title, items]) => ({
    title,
    summary: "",
    items
  }));
}

function normalizeSummaryOutlineSection(item: unknown): SummaryOutlineSection | null {
  if (!item || typeof item !== "object") return null;
  const record = item as Record<string, unknown>;
  const title = typeof record.title === "string" ? record.title.trim() : "";
  const summary = typeof record.summary === "string" ? record.summary.trim() : "";
  const items = Array.isArray(record.items)
    ? record.items
        .map((entry: unknown) => normalizeSummaryOutlineItem(entry))
        .filter((entry: SummaryOutlineItem | null): entry is SummaryOutlineItem => entry !== null)
    : [];
  if (!title || items.length === 0) return null;
  return { title, summary, items };
}

function normalizeSummaryOutlineItem(item: unknown): SummaryOutlineItem | null {
  if (!item || typeof item !== "object") return null;
  const record = item as Record<string, unknown>;
  const title = typeof record.title === "string" ? record.title.trim() : "";
  const summary = typeof record.summary === "string" ? record.summary.trim() : title;
  const detail = typeof record.detail === "string" ? record.detail.trim() : "";
  const evidence = typeof record.evidence === "string" ? record.evidence.trim() : null;
  if (!title || !detail) return null;
  return {
    title,
    summary: summary || title,
    detail,
    evidence,
    decisions: parseStringList(record.decisions),
    actionItems: parseStringList(record.actionItems),
    openQuestions: parseStringList(record.openQuestions)
  };
}

function parseDetailedNotes(raw?: string | null): DetailedSummaryNote[] {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    if (!Array.isArray(value?.detailedNotes)) return [];
    return value.detailedNotes
      .map((item: unknown) => {
        if (!item || typeof item !== "object") return null;
        const record = item as Record<string, unknown>;
        const title = typeof record.title === "string" ? record.title.trim() : "";
        const detail = typeof record.detail === "string" ? record.detail.trim() : "";
        const evidence = typeof record.evidence === "string" ? record.evidence.trim() : null;
        return title && detail ? { title, detail, evidence } : null;
      })
      .filter((item: DetailedSummaryNote | null): item is DetailedSummaryNote => item !== null);
  } catch {
    return [];
  }
}

function parseStructuredNotes(raw?: string | null): StructuredSummaryNote[] {
  if (!raw) return [];
  try {
    const value = JSON.parse(raw);
    if (Array.isArray(value?.structuredNotes)) {
      return value.structuredNotes
        .map((item: unknown) => normalizeStructuredNote(item))
        .filter((item: StructuredSummaryNote | null): item is StructuredSummaryNote => item !== null);
    }
  } catch {
    return [];
  }
  return parseDetailedNotes(raw).map((note) => ({
    title: note.title,
    category: "other",
    summary: note.title,
    detail: note.detail,
    evidence: note.evidence,
    decisions: [],
    actionItems: [],
    openQuestions: []
  }));
}

function normalizeStructuredNote(item: unknown): StructuredSummaryNote | null {
  if (!item || typeof item !== "object") return null;
  const record = item as Record<string, unknown>;
  const title = typeof record.title === "string" ? record.title.trim() : "";
  const category = typeof record.category === "string" ? record.category.trim() : "other";
  const summary = typeof record.summary === "string" ? record.summary.trim() : title;
  const detail = typeof record.detail === "string" ? record.detail.trim() : "";
  const evidence = typeof record.evidence === "string" ? record.evidence.trim() : null;
  if (!title || !detail) return null;
  return {
    title,
    category: category || "other",
    summary: summary || title,
    detail,
    evidence,
    decisions: parseStringList(record.decisions),
    actionItems: parseStringList(record.actionItems),
    openQuestions: parseStringList(record.openQuestions)
  };
}

function parseStringList(value: unknown): string[] {
  return Array.isArray(value) ? value.filter((item): item is string => typeof item === "string" && item.trim().length > 0) : [];
}

function formatSectionTitle(value: string): string {
  const normalized = value.replace(/[-_]/g, " ").trim();
  return normalized || "Meeting notes";
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

let glossaryEntrySequence = 0;

function createBlankGlossaryEntry(): GlossaryEntry {
  glossaryEntrySequence += 1;
  return {
    id: `glossary-${Date.now()}-${glossaryEntrySequence}`,
    term: "",
    description: ""
  };
}

function parseGlossaryEntries(raw: string): GlossaryEntry[] {
  return raw
    .split(/\r?\n/)
    .map((line) => line.trim())
    .filter(Boolean)
    .map((line) => {
      const separatorIndex = findGlossarySeparator(line);
      if (separatorIndex === -1) {
        return {
          ...createBlankGlossaryEntry(),
          term: line,
          description: ""
        };
      }
      return {
        ...createBlankGlossaryEntry(),
        term: line.slice(0, separatorIndex).trim(),
        description: line.slice(separatorIndex + 1).trim()
      };
    });
}

function findGlossarySeparator(line: string): number {
  const colon = line.indexOf(":");
  const fullWidthColon = line.indexOf("：");
  if (colon === -1) return fullWidthColon;
  if (fullWidthColon === -1) return colon;
  return Math.min(colon, fullWidthColon);
}

function serializeGlossaryEntries(entries: GlossaryEntry[]): string {
  return entries
    .map((entry) => {
      const term = entry.term.trim();
      const description = entry.description.trim();
      if (term && description) return `${term}: ${description}`;
      return term || description;
    })
    .filter(Boolean)
    .join("\n");
}

function formatStatus(status: string): string {
  return status.replace(/_/g, " ");
}

function isTaskActiveStatus(status: string): boolean {
  return ["transcribing", "summarizing", "canceling"].includes(status);
}

function isTaskBusy(busy: string | null): boolean {
  return busy === "transcribing" || busy === "retranscribing" || busy === "summarizing" || busy === "canceling";
}

function isCancellationMessage(message: string): boolean {
  return message.toLowerCase().includes("cancelled") || message.toLowerCase().includes("canceled");
}

function indexTaskStatuses(statuses: MeetingTaskStatus[]): Record<string, MeetingTaskStatus> {
  return Object.fromEntries(statuses.map((status) => [status.meetingId, status]));
}

function formatTaskKind(kind: string): string {
  if (kind === "summary") return "Summary";
  if (kind === "transcription") return "Transcription";
  return formatStatus(kind);
}

async function notifyTaskComplete(title: string, body: string) {
  if (!window.__TAURI_INTERNALS__) return;
  try {
    const { isPermissionGranted, requestPermission, sendNotification } = await import("@tauri-apps/plugin-notification");
    let granted = await isPermissionGranted();
    if (!granted) {
      granted = (await requestPermission()) === "granted";
    }
    if (granted) {
      sendNotification({ title, body });
    }
  } catch {
    // Notifications are helpful, but they should never block the meeting workflow.
  }
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
let mockOpenAiApiKeyStatus: OpenAiApiKeyStatus = { hasKey: false, source: "credential-manager" };
let mockActiveRecording: ActiveRecordingStatus | null = null;
let mockTaskStatuses: Record<string, MeetingTaskStatus> = {};
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
  seedMockTaskProgress(command, args);
  const delay = ["transcribe_meeting_demo", "retranscribe_meeting_demo", "summarize_meeting_demo"].includes(command)
    ? 1600
    : command.includes("download")
      ? 400
      : 140;
  await new Promise((resolve) => window.setTimeout(resolve, delay));
  if (command === "get_app_status") {
    const sidecar = mockSidecar(mockModelVerified, mockRuntimeInstalled);
    return {
      appVersion: "0.2.3",
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
  if (command === "get_openai_api_key_status") return mockOpenAiApiKeyStatus as T;
  if (command === "save_openai_api_key") {
    const apiKey = String(args?.apiKey ?? "").trim();
    if (!apiKey) throw new Error("OpenAI API key is empty.");
    mockOpenAiApiKeyStatus = { hasKey: true, source: "credential-manager" };
    return mockOpenAiApiKeyStatus as T;
  }
  if (command === "clear_openai_api_key") {
    mockOpenAiApiKeyStatus = { hasKey: false, source: "credential-manager" };
    return mockOpenAiApiKeyStatus as T;
  }
  if (command === "get_active_recording") return mockActiveRecording as T;
  if (command === "list_meeting_task_statuses") return Object.values(mockTaskStatuses) as T;
  if (command === "get_meeting_task_status") {
    return (mockTaskStatuses[String(args?.meetingId ?? "")] ?? null) as T;
  }
  if (command === "update_app_setting") {
    const key = String(args?.key ?? "");
    const value = String(args?.value ?? "");
    mockSettings = {
      ...mockSettings,
      ...(key === "raw_audio_retention_days" ? { rawAudioRetentionDays: Number(value) } : {}),
      ...(key === "transcription_provider" ? { transcriptionProvider: value } : {}),
      ...(key === "summary_model" ? { summaryModel: value } : {}),
      ...(key === "local_transcription_model" ? { localTranscriptionModel: value } : {}),
      ...(key === "openai_transcription_model" ? { openaiTranscriptionModel: value } : {}),
      ...(key === "language_hint" ? { languageHint: value } : {}),
      ...(key === "summary_language" ? { summaryLanguage: value } : {}),
      ...(key === "custom_glossary" ? { customGlossary: value } : {}),
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
    mockTaskStatuses[id] = {
      meetingId: id,
      kind: "transcription",
      phase: "transcribing",
      message: "Transcribing window 1 of 6",
      current: 1,
      total: 6,
      percent: 16,
      cancelRequested: false
    };
    const detail = mockDetails[id];
    detail.transcriptSegments = makeSegments(id);
    detail.meeting.status = "transcribed";
    mockMeetings = mockMeetings.map((meeting) => meeting.id === id ? toListItem(detail) : meeting);
    delete mockTaskStatuses[id];
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
    mockTaskStatuses[id] = {
      meetingId: id,
      kind: "transcription",
      phase: "transcribing",
      message: "Re-transcribing window 1 of 6",
      current: 1,
      total: 6,
      percent: 16,
      cancelRequested: false
    };
    const detail = mockDetails[id];
    detail.summary = null;
    detail.transcriptSegments = makeSegments(id).map((segment, index) => ({
      ...segment,
      id: `${id}-segment-retry-${index + 1}`
    }));
    detail.meeting.status = "transcribed";
    mockMeetings = mockMeetings.map((meeting) => meeting.id === id ? toListItem(detail) : meeting);
    delete mockTaskStatuses[id];
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
    mockTaskStatuses[id] = {
      meetingId: id,
      kind: "summary",
      phase: "summarizing",
      message: "Generating summary with Codex",
      current: 2,
      total: 4,
      percent: 50,
      cancelRequested: false
    };
    const detail = mockDetails[id];
    detail.summary = makeSummary(id);
    detail.meeting.title = detail.summary.suggestedTitle;
    detail.meeting.status = "summarized";
    mockMeetings = mockMeetings.map((meeting) => meeting.id === id ? toListItem(detail) : meeting);
    delete mockTaskStatuses[id];
    return {
      meetingId: id,
      suggestedTitle: detail.summary.suggestedTitle,
      provider: "codex-cli",
      model: mockSettings.summaryModel,
      language: "zh-CN",
      overview: detail.summary.overview,
      topics: parseStringArray(detail.summary.topicsJson),
      decisions: [{ text: "Keep local Whisper as the fallback transcription path.", evidence: null }],
      actionItems: [{ task: "Add a proper stop-recording worker.", owner: null, dueDate: null, evidence: null }],
      openQuestions: [],
      summaryOutline: parseSummaryOutline(detail.summary.rawJson),
      structuredNotes: parseStructuredNotes(detail.summary.rawJson),
      detailedNotes: parseDetailedNotes(detail.summary.rawJson),
      rawJson: detail.summary.rawJson
    } as T;
  }
  if (command === "cancel_meeting_task") {
    const id = String(args?.meetingId ?? "");
    const detail = mockDetails[id];
    const previousStatus = detail?.meeting.status ?? "unknown";
    const status = previousStatus === "summarizing"
      ? "summary_cancelled"
      : previousStatus === "transcribing" || previousStatus === "canceling"
        ? "transcription_cancelled"
        : previousStatus;
    if (detail) {
      detail.meeting.status = status;
      mockMeetings = mockMeetings.map((meeting) => meeting.id === id ? toListItem(detail) : meeting);
    }
    delete mockTaskStatuses[id];
    return {
      meetingId: id,
      cancelRequested: ["transcribing", "summarizing", "canceling"].includes(previousStatus),
      status
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
  if (command === "check_for_app_update") {
    return {
      currentVersion: "0.2.3",
      latestVersion: "v0.2.3",
      updateAvailable: false,
      installable: false,
      releaseName: "Note Taker v0.2.3",
      releaseUrl: "https://github.com/yihuil1992/note-taker/releases/tag/v0.2.3",
      publishedAt: "2026-06-12T22:47:08Z",
      notes: "You are running the latest release."
    } as T;
  }
  if (command === "open_sidecar_folder" || command === "open_exports_folder" || command === "open_url") return undefined as T;
  throw new Error(`Unsupported mock command: ${command}`);
}

function seedMockTaskProgress(command: string, args?: Record<string, unknown>) {
  const id = String(args?.meetingId ?? "");
  if (!id) return;
  if (command === "transcribe_meeting_demo" || command === "retranscribe_meeting_demo") {
    mockTaskStatuses[id] = {
      meetingId: id,
      kind: "transcription",
      phase: "transcribing",
      message: command === "retranscribe_meeting_demo" ? "Re-transcribing window 1 of 6" : "Transcribing window 1 of 6",
      current: 1,
      total: 6,
      percent: 16,
      cancelRequested: false
    };
  } else if (command === "summarize_meeting_demo") {
    mockTaskStatuses[id] = {
      meetingId: id,
      kind: "summary",
      phase: "summarizing",
      message: "Generating summary with Codex",
      current: 2,
      total: 4,
      percent: 50,
      cancelRequested: false
    };
  }
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
    },
    {
      id: `${id}-segment-3`,
      meetingId: id,
      sourceKind: "microphone",
      speakerLabel: "Me",
      language: "auto",
      startMs: 31000,
      endMs: 47000,
      text: "We also need the summary to keep enough context for reviewing a long meeting, not just a handful of bullets.",
      provider
    },
    {
      id: `${id}-segment-4`,
      meetingId: id,
      sourceKind: "system",
      speakerLabel: "Others",
      language: "auto",
      startMs: 52000,
      endMs: 69000,
      text: "A concise version is useful for scanning, but the reviewer should be able to expand details and see why the summary made those calls.",
      provider
    },
    {
      id: `${id}-segment-5`,
      meetingId: id,
      sourceKind: "microphone",
      speakerLabel: "Me",
      language: "auto",
      startMs: 74000,
      endMs: 91000,
      text: "The transcript panel should feel paired with the summary panel so the two panes behave like one review instrument.",
      provider
    },
    {
      id: `${id}-segment-6`,
      meetingId: id,
      sourceKind: "system",
      speakerLabel: "Others",
      language: "auto",
      startMs: 96000,
      endMs: 116000,
      text: "On tablet widths, keeping every column visible makes the central workspace too narrow for the recording controls and detail review.",
      provider
    }
  ];
}

function makeSummary(id: string): MeetingSummaryRecord {
  const rawSummary = {
    suggestedTitle: "Local transcription smoke review",
    language: "zh-CN",
    overview: "本次会议验证了本地录音、分段转录、Codex 总结和导出流程。可推断的负责人和截止日期保持为空，避免臆造。",
    topics: ["Local recording", "Whisper transcription", "Codex summary"],
    decisions: [{ text: "Keep local Whisper as the fallback transcription path.", evidence: null }],
    actionItems: [{ task: "Add a proper stop-recording worker.", owner: null, dueDate: null, evidence: null }],
    openQuestions: [],
    summaryOutline: [
      {
        title: "Local capture path",
        summary: "The meeting validates the local recording and transcription path as one review flow.",
        items: [
          {
            title: "Microphone and computer audio stay separated",
            summary: "The app stores separate local chunks and preserves source labels.",
            detail: "The mock review confirms microphone and computer audio remain separate local chunks. The transcript preserves source labels, which is enough for the current source-attributed meeting record without claiming full diarization.",
            evidence: "0:00, 0:15",
            decisions: ["Keep source attribution instead of claiming full speaker diarization."],
            actionItems: [],
            openQuestions: []
          },
          {
            title: "Stop-recording worker still needs hardening",
            summary: "The local capture path needs a proper worker boundary for stopping.",
            detail: "Stopping should wait for the active chunk to finish and then persist the meeting cleanly, rather than leaving the UI to infer whether the background worker has finished.",
            evidence: "0:15",
            decisions: [],
            actionItems: ["Add a proper stop-recording worker."],
            openQuestions: []
          }
        ]
      },
      {
        title: "Summary review model",
        summary: "Summary should be one structured meeting record with expandable points.",
        items: [
          {
            title: "Use an integrated outline instead of a detail dump",
            summary: "The summary should not split into a top summary and a flat detailed notes list.",
            detail: "The summary should keep a fast-scannable overview while also retaining enough detail for someone to reconstruct the discussion later. The review surface should read like one structured meeting record: section, point, and expandable detail.",
            evidence: "0:31, 0:52",
            decisions: ["Render the meeting record as grouped expandable points."],
            actionItems: [],
            openQuestions: []
          }
        ]
      },
      {
        title: "Review layout",
        summary: "Summary and transcript should act as paired review panes.",
        items: [
          {
            title: "Transcript height follows the summary panel",
            summary: "The right transcript pane should feel paired with the left summary record.",
            detail: "The transcript and summary are treated as paired review panes. On wider detail layouts, the transcript scroll region stretches to the summary panel height instead of using a hard viewport cap.",
            evidence: "1:14",
            decisions: [],
            actionItems: [],
            openQuestions: ["Whether transcript height should follow the collapsed or expanded summary state."]
          }
        ]
      }
    ]
  };
  return {
    meetingId: id,
    suggestedTitle: "Local transcription smoke review",
    provider: "codex-cli",
    model: mockSettings.summaryModel,
    language: "zh-CN",
    overview: "本次会议验证了本地录音、分段转录、Codex 总结和导出流程。可推断的负责人和截止日期保持为空，避免臆造。",
    topicsJson: JSON.stringify(["Local recording", "Whisper transcription", "Codex summary"]),
    decisionsJson: JSON.stringify([{ text: "Keep local Whisper as the fallback transcription path.", evidence: null }]),
    actionItemsJson: JSON.stringify([{ task: "Add a proper stop-recording worker.", owner: null, dueDate: null, evidence: null }]),
    risksOrQuestionsJson: JSON.stringify([]),
    rawJson: JSON.stringify(rawSummary),
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

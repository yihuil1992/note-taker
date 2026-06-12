# Note Taker

Windows-first meeting capture and transcription app with local transcription by default.

Note Taker records microphone and computer audio on Windows, stores meeting records in a local SQLite database, transcribes with a local Whisper-compatible sidecar by default, summarizes with Codex CLI, and exports Markdown or JSON notes.

## Demo

Archive Sheet theme:

![Archive Sheet UI](docs/demo/archive-sheet.png)

Night Atlas theme:

![Night Atlas UI](docs/demo/night-atlas.png)

Mobile layout:

![Mobile Archive UI](docs/demo/mobile-archive.png)

## Current Status

Phase 1 is a working local-by-default MVP.

Implemented:

- Tauri 2 desktop app with React and TypeScript UI.
- Hoshikuzu-inspired Night Atlas and Archive Sheet themes.
- Microphone and system audio capture on Windows.
- Chunked recording with concurrent microphone and computer-audio streams.
- Smart transcription windows built from short persisted capture chunks.
- SQLite storage for meetings, audio chunks, transcript segments, summaries, settings, and search.
- Local Whisper-compatible sidecar transcription.
- Optional OpenAI Speech-to-Text provider with a key from Windows Credential Manager or `OPENAI_API_KEY`.
- Codex CLI structured summaries from transcript segments.
- Local search over title, summary, action items, topics, and transcript text.
- Markdown and JSON export to the app-data `exports/` directory.
- Guided local sidecar setup for whisper.cpp runtime and model downloads.
- Meeting soft archive: archived meetings are hidden from the main list and search without deleting local files.
- Code-native themed dropdowns, icon button tooltips, and browser-preview mock data for UI smoke testing outside Tauri.

Not implemented yet:

- Tray/background behavior if the window is closed during recording.
- Editable transcript, summary, and action-item fields.
- Keyring-backed OpenAI API key entry.
- A UI view for restoring archived meetings.

## Requirements

- Windows.
- Node.js and pnpm.
- Rust stable toolchain.
- WebView2 runtime for Tauri.
- Codex CLI for local structured summaries.

## Quick Start

Install dependencies:

```powershell
pnpm install
```

Run the browser preview:

```powershell
pnpm dev
```

Run the desktop app:

```powershell
pnpm tauri:dev
```

Build the desktop app and installers:

```powershell
pnpm tauri build
```

Release artifacts are written under:

```text
src-tauri/target/release/
src-tauri/target/release/bundle/
```

## Using The App

1. Open the app.
2. If local AI is not ready, download the whisper.cpp runtime and default model from the setup panel.
3. Choose a transcription provider in Settings.
4. Start recording after confirming meeting consent.
5. Stop recording when the meeting ends.
6. Select the meeting, run Transcribe or Re-transcribe, then Summarize.
7. Search prior meetings, archive unwanted records, or export the selected meeting as Markdown/JSON.

Local Whisper stays on device. The OpenAI provider uploads audio chunks and must be explicitly selected. If OpenAI transcription fails for a window, the app falls back to local `large-v3-turbo` for that window.

## Local AI Sidecar

The app stores sidecar assets under the platform app-data directory:

```text
sidecars/
  whisper-cli.exe
models/
  ggml-large-v3-turbo.bin
```

The default runtime is pinned to the official `ggml-org/whisper.cpp` `v1.8.6` Windows x64 release asset `whisper-bin-x64.zip`. The app verifies SHA-256 before extracting `whisper-cli.exe` and sibling runtime files into the sidecar directory.

The default model is the multilingual `ggml-large-v3-turbo.bin` artifact. Users can switch to `large-v3` for maximum accuracy.

## Development Commands

Run the real audio spike:

```powershell
pnpm audio:spike 3
```

Run a chunked meeting capture demo:

```powershell
pnpm meeting:demo 6 3 target\meeting-demo
```

Transcribe a recorded meeting:

```powershell
pnpm meeting:transcribe <meeting-id> target\meeting-demo
```

Rechunk a meeting into fixed windows for debugging:

```powershell
pnpm meeting:rechunk <meeting-id-or-title> target\meeting-demo 15
```

Generate a structured summary with Codex CLI:

```powershell
pnpm meeting:summarize <meeting-id> target\meeting-demo
```

Download and verify the pinned whisper.cpp Windows x64 runtime:

```powershell
pnpm sidecar:runtime target\sidecar-runtime
```

Download and verify the default local transcription model:

```powershell
pnpm sidecar:model target\sidecar-runtime
```

Run a sidecar transcription smoke test:

```powershell
pnpm transcribe:smoke target\audio-spike\microphone.wav target\sidecar-smoke
```

## Optional OpenAI Transcription

To use OpenAI Speech-to-Text in the desktop app, choose `OpenAI API speech-to-text` in Settings, paste your OpenAI API key into the OpenAI API key field, and save it. The key is stored in Windows Credential Manager and is not written to SQLite. If an OpenAI window fails because of API, quota, rate, or key errors, that window falls back to the local whisper.cpp `large-v3-turbo` model.

For development commands, you can still set `OPENAI_API_KEY` before launching the app or running the CLI. Environment variables take precedence over the stored key.

```powershell
$env:OPENAI_API_KEY="your-openai-api-key"
pnpm tauri:dev
```

Supported model choices:

- `gpt-4o-mini-transcribe`
- `gpt-4o-transcribe`
- `whisper-1`

Codex CLI remains the structured text summary provider. Audio transcription through Codex CLI is not used because the current CLI path does not expose audio-file attachments.

## Verification

```powershell
pnpm typecheck
pnpm build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
pnpm tauri build
```

## CI/CD

GitHub Actions is configured for a Windows-first pipeline.

- `CI` runs on pushes and pull requests to `main`.
- `CI` installs pnpm/Node/Rust, then runs frontend typecheck, frontend build, Rust check, and Rust tests.
- `Release` runs manually from the Actions tab or when pushing a `v*` tag.
- `Release` runs `pnpm tauri build` on `windows-2022` and uploads the standalone executable plus MSI/NSIS installers as workflow artifacts.
- Tag builds such as `v0.2.0` also create or update a GitHub Release with those Windows artifacts.

The release workflow does not sign binaries yet. Treat uploaded installers as unsigned development artifacts until a signing certificate is configured.

## Automatic Updates

On startup, the app checks the signed Tauri updater manifest at the latest public GitHub Release for `yihuil1992/note-taker`.

- If a newer signed updater bundle is available, the app shows an update notice with an `Update and restart` action.
- Clicking `Update and restart` downloads the signed updater bundle, installs it through Tauri's updater, and relaunches the app.
- If GitHub is unavailable, the updater manifest is missing, or no signed update is available, the app stays quiet and continues normally.
- Release notes links are restricted to this repository's GitHub release URLs before they are opened.
- This is an in-app signed bundle update, not a binary-diff incremental patch. Windows installers are still not code-signed, so SmartScreen may still warn on direct installer downloads.

The updater signing private key is stored outside the repository. GitHub Actions expects the `TAURI_SIGNING_PRIVATE_KEY` secret when publishing tagged releases.

## Privacy Defaults

- Meeting audio and transcript data stay local by default.
- Raw audio retention defaults to 7 days.
- Local Whisper transcription stays on device.
- OpenAI transcription is opt-in and sends selected audio windows to OpenAI.
- Provider secrets use the OS credential store.
- Codex auth tokens must not be stored by this app.

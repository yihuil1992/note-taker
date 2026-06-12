use crate::storage::AudioChunkRecord;
use hound::{SampleFormat, WavReader, WavSpec, WavWriter};
use serde::Serialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

const FRAME_MS: i64 = 100;
const QUIET_RMS_THRESHOLD: f64 = 0.004;
const MIN_UTTERANCE_MS: i64 = 1_200;
const END_OF_TURN_SILENCE_MS: i64 = 900;
const MICROPHONE_TARGET_WINDOW_MS: i64 = 24_000;
const MICROPHONE_MAX_WINDOW_MS: i64 = 30_000;
const SYSTEM_TARGET_WINDOW_MS: i64 = 16_000;
const SYSTEM_MAX_WINDOW_MS: i64 = 20_000;
const PRE_ROLL_MS: i64 = 1_000;
const POST_ROLL_MS: i64 = 1_000;
const DOMINANCE_RATIO: f64 = 1.25;
const TARGET_WINDOW_RMS: f64 = 0.045;
const MAX_NORMALIZATION_GAIN: f64 = 3.0;
const MIN_NORMALIZATION_RMS: f64 = 0.008;

#[derive(Debug, Clone, Copy)]
struct WindowPolicy {
    target_ms: i64,
    max_output_ms: i64,
    same_source_gap_ms: i64,
    pre_roll_ms: i64,
    post_roll_ms: i64,
}

impl WindowPolicy {
    fn max_core_ms(self) -> i64 {
        self.max_output_ms - self.pre_roll_ms - self.post_roll_ms
    }
}

#[derive(Debug, Error)]
pub enum SmartChunkError {
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("WAV error: {0}")]
    Wav(#[from] hound::Error),
    #[error("Unsupported WAV format in {path}: {message}")]
    UnsupportedWav { path: String, message: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranscriptionWindow {
    pub id: String,
    pub meeting_id: String,
    pub source_kind: String,
    pub chunk_ids: Vec<String>,
    pub started_at_ms: i64,
    pub duration_ms: i64,
    pub path: String,
}

#[derive(Debug, Clone)]
struct ChunkAudio {
    chunk: AudioChunkRecord,
    spec: WavSpec,
    samples: Vec<i16>,
}

#[derive(Debug)]
struct ChunkSpan {
    id: String,
    start_ms: i64,
    end_ms: i64,
}

#[derive(Debug)]
struct SourceTimeline {
    meeting_id: String,
    source_kind: String,
    started_at_ms: i64,
    spec: WavSpec,
    samples: Vec<i16>,
    chunks: Vec<ChunkSpan>,
}

#[derive(Debug, Clone)]
struct SourceRun {
    source_kind: String,
    start_ms: i64,
    end_ms: i64,
}

pub fn build_transcription_windows(
    chunks: &[AudioChunkRecord],
    output_dir: &Path,
) -> Result<Vec<TranscriptionWindow>, SmartChunkError> {
    fs::create_dir_all(output_dir)?;
    let timelines = build_source_timelines(chunks)?;
    if timelines.is_empty() {
        return Ok(Vec::new());
    }

    let runs = build_turn_runs(&timelines);
    let mut windows = Vec::new();
    for run in runs {
        if let Some(timeline) = timelines.get(&run.source_kind) {
            windows.push(write_window(
                timeline,
                output_dir,
                run.start_ms,
                run.end_ms,
            )?);
        }
    }
    windows.sort_by_key(|window| (window.started_at_ms, window.source_kind.clone()));
    Ok(windows)
}

fn build_source_timelines(
    chunks: &[AudioChunkRecord],
) -> Result<BTreeMap<String, SourceTimeline>, SmartChunkError> {
    let mut by_source: BTreeMap<String, Vec<ChunkAudio>> = BTreeMap::new();
    for chunk in chunks {
        if chunk.status == "capture_failed" {
            continue;
        }
        let audio = read_chunk_audio(chunk)?;
        by_source
            .entry(chunk.source_kind.clone())
            .or_default()
            .push(audio);
    }

    let mut timelines = BTreeMap::new();
    for (source_kind, mut source_chunks) in by_source {
        source_chunks.sort_by_key(|entry| entry.chunk.started_at_ms);
        if let Some(timeline) = concatenate_source_chunks(&source_kind, &source_chunks) {
            timelines.insert(source_kind, timeline);
        }
    }
    Ok(timelines)
}

fn concatenate_source_chunks(source_kind: &str, chunks: &[ChunkAudio]) -> Option<SourceTimeline> {
    let first = chunks.first()?;
    let mut timeline = SourceTimeline {
        meeting_id: first.chunk.meeting_id.clone(),
        source_kind: source_kind.to_string(),
        started_at_ms: first.chunk.started_at_ms,
        spec: first.spec,
        samples: Vec::new(),
        chunks: Vec::new(),
    };

    for chunk in chunks {
        if chunk.spec != timeline.spec {
            continue;
        }
        let expected_start =
            timeline.started_at_ms + frames_to_ms(total_frames(&timeline), timeline.spec);
        if chunk.chunk.started_at_ms > expected_start + FRAME_MS {
            append_silence(
                &mut timeline.samples,
                timeline.spec,
                chunk.chunk.started_at_ms - expected_start,
            );
        }

        let chunk_frames = frame_count(&chunk.samples, chunk.spec.channels);
        let actual_start =
            timeline.started_at_ms + frames_to_ms(total_frames(&timeline), timeline.spec);
        timeline.samples.extend_from_slice(&chunk.samples);
        timeline.chunks.push(ChunkSpan {
            id: chunk.chunk.id.clone(),
            start_ms: actual_start,
            end_ms: actual_start + frames_to_ms(chunk_frames, chunk.spec),
        });
    }

    Some(timeline)
}

fn build_turn_runs(timelines: &BTreeMap<String, SourceTimeline>) -> Vec<SourceRun> {
    let start_ms = timelines
        .values()
        .map(|timeline| timeline.started_at_ms)
        .min()
        .unwrap_or(0);
    let end_ms = timelines
        .values()
        .map(SourceTimeline::end_ms)
        .max()
        .unwrap_or(start_ms);

    let mut raw_runs = Vec::new();
    let mut active_source: Option<String> = None;
    let mut active_start = start_ms;
    let mut silence_start: Option<i64> = None;

    let mut cursor = start_ms;
    while cursor < end_ms {
        let next = (cursor + FRAME_MS).min(end_ms);
        let decided_source = dominant_source(timelines, cursor, next);

        match (&active_source, decided_source) {
            (None, Some(source)) => {
                active_source = Some(source);
                active_start = cursor;
                silence_start = None;
            }
            (Some(current), Some(next_source)) if *current == next_source => {
                silence_start = None;
            }
            (Some(current), Some(next_source)) => {
                push_run(&mut raw_runs, current.clone(), active_start, cursor);
                active_source = Some(next_source);
                active_start = cursor;
                silence_start = None;
            }
            (Some(current), None) => {
                let silence = silence_start.get_or_insert(cursor);
                if next - *silence >= END_OF_TURN_SILENCE_MS {
                    push_run(&mut raw_runs, current.clone(), active_start, *silence);
                    active_source = None;
                    silence_start = None;
                }
            }
            (None, None) => {}
        }

        cursor = next;
    }

    if let Some(source) = active_source {
        let end = silence_start.unwrap_or(end_ms);
        push_run(&mut raw_runs, source, active_start, end);
    }

    split_long_runs(merge_context_runs(merge_short_gaps(raw_runs)))
}

fn dominant_source(
    timelines: &BTreeMap<String, SourceTimeline>,
    start_ms: i64,
    end_ms: i64,
) -> Option<String> {
    let mut scored = timelines
        .values()
        .map(|timeline| {
            (
                timeline.source_kind.clone(),
                timeline.rms_between(start_ms, end_ms),
            )
        })
        .filter(|(_, rms)| *rms >= QUIET_RMS_THRESHOLD)
        .collect::<Vec<_>>();
    scored.sort_by(|left, right| right.1.total_cmp(&left.1));

    let (top_source, top_rms) = scored.first()?.clone();
    let second_rms = scored.get(1).map(|(_, rms)| *rms).unwrap_or(0.0);
    if second_rms == 0.0 || top_rms >= second_rms * DOMINANCE_RATIO {
        return Some(top_source);
    }

    // When both channels contain the same remote speaker, the loopback/system
    // source is usually the cleaner attribution. Only let the mic win if it is
    // clearly louder than the system channel.
    let system = scored.iter().find(|(source, _)| source == "system");
    let microphone = scored.iter().find(|(source, _)| source == "microphone");
    match (microphone, system) {
        (Some((_, mic)), Some((_, sys))) if *mic >= *sys * DOMINANCE_RATIO => {
            Some("microphone".to_string())
        }
        (_, Some(_)) => Some("system".to_string()),
        (Some(_), _) => Some("microphone".to_string()),
        _ => Some(top_source),
    }
}

fn push_run(runs: &mut Vec<SourceRun>, source_kind: String, start_ms: i64, end_ms: i64) {
    if end_ms - start_ms >= MIN_UTTERANCE_MS {
        runs.push(SourceRun {
            source_kind,
            start_ms,
            end_ms,
        });
    }
}

fn merge_short_gaps(runs: Vec<SourceRun>) -> Vec<SourceRun> {
    let mut merged: Vec<SourceRun> = Vec::new();
    for run in runs {
        if let Some(previous) = merged.last_mut() {
            let policy = window_policy(&run.source_kind);
            if previous.source_kind == run.source_kind
                && run.start_ms - previous.end_ms <= policy.same_source_gap_ms
            {
                previous.end_ms = run.end_ms;
                continue;
            }
        }
        merged.push(run);
    }
    merged
}

fn merge_context_runs(runs: Vec<SourceRun>) -> Vec<SourceRun> {
    let mut merged: Vec<SourceRun> = Vec::new();
    for run in runs {
        if let Some(previous) = merged.last_mut() {
            let policy = window_policy(&run.source_kind);
            let same_source = previous.source_kind == run.source_kind;
            let short_gap = run.start_ms - previous.end_ms <= policy.same_source_gap_ms;
            let combined = run.end_ms - previous.start_ms;
            if same_source && short_gap && combined <= policy.max_core_ms() {
                previous.end_ms = run.end_ms;
                continue;
            }
        }
        merged.push(run);
    }
    merged
}

fn split_long_runs(runs: Vec<SourceRun>) -> Vec<SourceRun> {
    let mut split = Vec::new();
    for run in runs {
        let policy = window_policy(&run.source_kind);
        let mut start = run.start_ms;
        while run.end_ms - start > policy.max_core_ms() {
            split.push(SourceRun {
                source_kind: run.source_kind.clone(),
                start_ms: start,
                end_ms: (start + policy.target_ms).min(run.end_ms),
            });
            start += policy.target_ms;
        }
        if run.end_ms - start >= MIN_UTTERANCE_MS {
            split.push(SourceRun {
                source_kind: run.source_kind,
                start_ms: start,
                end_ms: run.end_ms,
            });
        }
    }
    split
}

fn window_policy(source_kind: &str) -> WindowPolicy {
    match source_kind {
        "system" => WindowPolicy {
            target_ms: SYSTEM_TARGET_WINDOW_MS,
            max_output_ms: SYSTEM_MAX_WINDOW_MS,
            same_source_gap_ms: 1_200,
            pre_roll_ms: PRE_ROLL_MS,
            post_roll_ms: POST_ROLL_MS,
        },
        _ => WindowPolicy {
            target_ms: MICROPHONE_TARGET_WINDOW_MS,
            max_output_ms: MICROPHONE_MAX_WINDOW_MS,
            same_source_gap_ms: 2_500,
            pre_roll_ms: PRE_ROLL_MS,
            post_roll_ms: POST_ROLL_MS,
        },
    }
}

fn write_window(
    timeline: &SourceTimeline,
    output_dir: &Path,
    run_start_ms: i64,
    run_end_ms: i64,
) -> Result<TranscriptionWindow, SmartChunkError> {
    let policy = window_policy(&timeline.source_kind);
    let absolute_start_ms = (run_start_ms - policy.pre_roll_ms).max(timeline.started_at_ms);
    let absolute_end_ms = (run_end_ms + policy.post_roll_ms).min(timeline.end_ms());
    let duration_ms = absolute_end_ms.saturating_sub(absolute_start_ms);
    let safe_source = timeline.source_kind.replace(['\\', '/', ':'], "-");
    let path = output_dir.join(format!(
        "{}-{}-{}-{}.wav",
        timeline.meeting_id, safe_source, absolute_start_ms, absolute_end_ms
    ));
    write_sample_slice(timeline, &path, absolute_start_ms, absolute_end_ms)?;

    Ok(TranscriptionWindow {
        id: format!(
            "{}:{}:{}-{}",
            timeline.meeting_id, timeline.source_kind, absolute_start_ms, absolute_end_ms
        ),
        meeting_id: timeline.meeting_id.clone(),
        source_kind: timeline.source_kind.clone(),
        chunk_ids: overlapping_chunk_ids(timeline, absolute_start_ms, absolute_end_ms),
        started_at_ms: absolute_start_ms,
        duration_ms,
        path: path.display().to_string(),
    })
}

fn write_sample_slice(
    timeline: &SourceTimeline,
    output_path: &Path,
    start_ms: i64,
    end_ms: i64,
) -> Result<(), SmartChunkError> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    let start_frame = timeline.ms_to_local_frame(start_ms);
    let end_frame = timeline
        .ms_to_local_frame(end_ms)
        .min(total_frames(timeline));
    let channels = usize::from(timeline.spec.channels);
    let start_sample = start_frame * channels;
    let end_sample = end_frame * channels;
    let samples = normalize_window_samples(&timeline.samples[start_sample..end_sample]);
    let mut writer = WavWriter::create(output_path, timeline.spec)?;
    for sample in samples {
        writer.write_sample(sample)?;
    }
    writer.finalize()?;
    Ok(())
}

fn normalize_window_samples(samples: &[i16]) -> Vec<i16> {
    if samples.is_empty() {
        return Vec::new();
    }
    let rms = rms_for_samples(samples);
    if rms < MIN_NORMALIZATION_RMS || rms >= TARGET_WINDOW_RMS {
        return samples.to_vec();
    }
    let gain = (TARGET_WINDOW_RMS / rms).min(MAX_NORMALIZATION_GAIN);
    samples
        .iter()
        .map(|sample| {
            let scaled = f64::from(*sample) * gain;
            scaled.clamp(f64::from(i16::MIN), f64::from(i16::MAX)) as i16
        })
        .collect()
}

fn rms_for_samples(samples: &[i16]) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum = samples
        .iter()
        .map(|sample| {
            let normalized = f64::from(*sample) / f64::from(i16::MAX);
            normalized * normalized
        })
        .sum::<f64>();
    (sum / samples.len() as f64).sqrt()
}

fn overlapping_chunk_ids(timeline: &SourceTimeline, start_ms: i64, end_ms: i64) -> Vec<String> {
    timeline
        .chunks
        .iter()
        .filter(|chunk| chunk.end_ms > start_ms && chunk.start_ms < end_ms)
        .map(|chunk| chunk.id.clone())
        .collect()
}

fn read_chunk_audio(chunk: &AudioChunkRecord) -> Result<ChunkAudio, SmartChunkError> {
    let path = PathBuf::from(&chunk.path);
    let mut reader = WavReader::open(&path)?;
    let spec = reader.spec();
    validate_spec(&path, spec)?;
    let mut samples = Vec::new();
    for sample in reader.samples::<i16>() {
        samples.push(sample?);
    }

    Ok(ChunkAudio {
        chunk: chunk.clone(),
        spec,
        samples,
    })
}

fn validate_spec(path: &Path, spec: WavSpec) -> Result<(), SmartChunkError> {
    if spec.channels == 0 || spec.sample_rate == 0 {
        return Err(SmartChunkError::UnsupportedWav {
            path: path.display().to_string(),
            message: "missing channels or sample rate".to_string(),
        });
    }
    if spec.sample_format != SampleFormat::Int || spec.bits_per_sample != 16 {
        return Err(SmartChunkError::UnsupportedWav {
            path: path.display().to_string(),
            message: format!(
                "expected 16-bit PCM, got {:?} {} bits",
                spec.sample_format, spec.bits_per_sample
            ),
        });
    }
    Ok(())
}

impl SourceTimeline {
    fn end_ms(&self) -> i64 {
        self.started_at_ms + frames_to_ms(total_frames(self), self.spec)
    }

    fn ms_to_local_frame(&self, absolute_ms: i64) -> usize {
        ms_to_frames(absolute_ms.saturating_sub(self.started_at_ms), self.spec)
    }

    fn rms_between(&self, start_ms: i64, end_ms: i64) -> f64 {
        let start_frame = self.ms_to_local_frame(start_ms).min(total_frames(self));
        let end_frame = self.ms_to_local_frame(end_ms).min(total_frames(self));
        if end_frame <= start_frame {
            return 0.0;
        }
        rms_for_frames(self, start_frame, end_frame)
    }
}

fn rms_for_frames(timeline: &SourceTimeline, start_frame: usize, end_frame: usize) -> f64 {
    let channels = usize::from(timeline.spec.channels);
    let start_sample = start_frame * channels;
    let end_sample = (end_frame * channels).min(timeline.samples.len());
    if end_sample <= start_sample {
        return 0.0;
    }
    let mut sum = 0.0;
    let mut count = 0_u64;
    for sample in &timeline.samples[start_sample..end_sample] {
        let normalized = f64::from(*sample) / f64::from(i16::MAX);
        sum += normalized * normalized;
        count += 1;
    }
    (sum / count as f64).sqrt()
}

fn append_silence(samples: &mut Vec<i16>, spec: WavSpec, silence_ms: i64) {
    let frames = ms_to_frames(silence_ms, spec);
    samples.resize(samples.len() + frames * usize::from(spec.channels), 0);
}

fn total_frames(timeline: &SourceTimeline) -> usize {
    frame_count(&timeline.samples, timeline.spec.channels)
}

fn frame_count(samples: &[i16], channels: u16) -> usize {
    samples.len() / usize::from(channels)
}

fn ms_to_frames(ms: i64, spec: WavSpec) -> usize {
    ((ms.max(0) as u128 * u128::from(spec.sample_rate)) / 1000) as usize
}

fn frames_to_ms(frames: usize, spec: WavSpec) -> i64 {
    ((frames as u128 * 1000) / u128::from(spec.sample_rate)) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_spec() -> WavSpec {
        WavSpec {
            channels: 1,
            sample_rate: 1_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        }
    }

    fn write_test_wav(path: &Path, duration_ms: i64, sample: i16) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create wav parent");
        }
        let spec = test_spec();
        let mut writer = WavWriter::create(path, spec).expect("create wav");
        let frames = ms_to_frames(duration_ms, spec);
        for _ in 0..frames {
            writer.write_sample(sample).expect("write sample");
        }
        writer.finalize().expect("finalize wav");
    }

    fn chunk(
        root: &Path,
        source_kind: &str,
        start_ms: i64,
        duration_ms: i64,
        sample: i16,
    ) -> AudioChunkRecord {
        let path = root.join(format!("{source_kind}-{start_ms}-{duration_ms}.wav"));
        write_test_wav(&path, duration_ms, sample);
        AudioChunkRecord {
            id: format!("{source_kind}-{start_ms}"),
            meeting_id: "meeting-a".to_string(),
            source_kind: source_kind.to_string(),
            started_at_ms: start_ms,
            duration_ms,
            path: path.display().to_string(),
            status: "captured".to_string(),
            transcription_error: None,
        }
    }

    fn temp_root() -> PathBuf {
        std::env::temp_dir().join(format!(
            "note-taker-smart-chunks-test-{}",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn merges_same_source_context_across_short_silence() {
        let root = temp_root();
        let input_dir = root.join("input");
        let output_dir = root.join("output");
        let chunks = vec![
            chunk(&input_dir, "microphone", 0, 10_000, 4_000),
            chunk(&input_dir, "microphone", 12_000, 10_000, 4_000),
        ];

        let windows = build_transcription_windows(&chunks, &output_dir).expect("build windows");

        assert_eq!(windows.len(), 1);
        assert_eq!(windows[0].source_kind, "microphone");
        assert_eq!(windows[0].started_at_ms, 0);
        assert!(windows[0].duration_ms >= 21_000);

        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn splits_long_context_windows_without_returning_to_tiny_chunks() {
        let root = temp_root();
        let input_dir = root.join("input");
        let output_dir = root.join("output");
        let chunks = vec![chunk(&input_dir, "system", 0, 70_000, 4_000)];

        let windows = build_transcription_windows(&chunks, &output_dir).expect("build windows");

        assert_eq!(windows.len(), 5);
        assert!(windows.iter().all(|window| window.source_kind == "system"));
        assert!(windows.iter().all(
            |window| window.duration_ms <= SYSTEM_TARGET_WINDOW_MS + PRE_ROLL_MS + POST_ROLL_MS
        ));
        assert!(windows.iter().all(|window| window.duration_ms >= 6_000));

        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn ignores_silent_audio() {
        let root = temp_root();
        let input_dir = root.join("input");
        let output_dir = root.join("output");
        let chunks = vec![chunk(&input_dir, "microphone", 0, 10_000, 0)];

        let windows = build_transcription_windows(&chunks, &output_dir).expect("build windows");

        assert!(windows.is_empty());

        fs::remove_dir_all(root).expect("remove temp root");
    }

    #[test]
    fn normalization_applies_limited_gain_to_quiet_windows() {
        let samples = vec![400_i16; 1_000];

        let normalized = normalize_window_samples(&samples);

        assert!(rms_for_samples(&normalized) > rms_for_samples(&samples));
        assert!(normalized.iter().all(|sample| *sample <= 1_200));
    }
}

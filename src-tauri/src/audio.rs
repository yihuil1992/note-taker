use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use hound::{SampleFormat as WavSampleFormat, WavSpec, WavWriter};
use serde::Serialize;
use std::fs;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AudioError {
    #[error("No default audio device found for {0}")]
    MissingDevice(&'static str),
    #[error("Audio backend error: {0}")]
    Backend(String),
    #[error("Failed to write WAV file: {0}")]
    Wav(#[from] hound::Error),
    #[error("File system error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Windows WASAPI error: {0}")]
    Windows(String),
    #[error("Unsupported audio format: {0}")]
    UnsupportedFormat(String),
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AudioDevice {
    id: String,
    name: String,
    kind: AudioDeviceKind,
    is_default: bool,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum AudioDeviceKind {
    Input,
    Output,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SpikeResult {
    pub mic: CaptureArtifact,
    pub system: CaptureArtifact,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CaptureArtifact {
    pub path: String,
    pub duration_seconds: f32,
    pub sample_rate: u32,
    pub channels: u16,
    pub rms: f32,
    pub non_zero_samples: usize,
    pub error: Option<String>,
}

pub fn list_devices() -> Result<Vec<AudioDevice>, AudioError> {
    let host = cpal::default_host();
    let default_input = host
        .default_input_device()
        .and_then(|device| device.name().ok());
    let default_output = host
        .default_output_device()
        .and_then(|device| device.name().ok());
    let mut devices = Vec::new();

    if let Ok(inputs) = host.input_devices() {
        for (index, device) in inputs.enumerate() {
            let name = device
                .name()
                .unwrap_or_else(|_| format!("Input device {index}"));
            devices.push(AudioDevice {
                id: format!("input-{index}"),
                is_default: default_input.as_ref() == Some(&name),
                name,
                kind: AudioDeviceKind::Input,
            });
        }
    }

    if let Ok(outputs) = host.output_devices() {
        for (index, device) in outputs.enumerate() {
            let name = device
                .name()
                .unwrap_or_else(|_| format!("Output device {index}"));
            devices.push(AudioDevice {
                id: format!("output-{index}"),
                is_default: default_output.as_ref() == Some(&name),
                name,
                kind: AudioDeviceKind::Output,
            });
        }
    }

    Ok(devices)
}

pub fn capture_spike(output_dir: &Path, seconds: u32) -> Result<SpikeResult, AudioError> {
    fs::create_dir_all(output_dir)?;
    let mic_path = output_dir.join("microphone.wav");
    let system_path = output_dir.join("system.wav");

    let (mic, system) = thread::scope(|scope| {
        let mic_handle = scope.spawn(|| capture_microphone(&mic_path, seconds));
        let system_handle = scope.spawn(|| capture_system_loopback(&system_path, seconds));
        let mic = mic_handle
            .join()
            .unwrap_or_else(|_| {
                Err(AudioError::Backend(
                    "microphone capture thread panicked".to_string(),
                ))
            })
            .unwrap_or_else(|error| CaptureArtifact::failed(&mic_path, error));
        let system = system_handle
            .join()
            .unwrap_or_else(|_| {
                Err(AudioError::Backend(
                    "system capture thread panicked".to_string(),
                ))
            })
            .unwrap_or_else(|error| CaptureArtifact::failed(&system_path, error));
        (mic, system)
    });

    Ok(SpikeResult { mic, system })
}

fn capture_microphone(path: &Path, seconds: u32) -> Result<CaptureArtifact, AudioError> {
    let host = cpal::default_host();
    let device = host
        .default_input_device()
        .ok_or(AudioError::MissingDevice("microphone"))?;
    let config = device
        .default_input_config()
        .map_err(|error| AudioError::Backend(error.to_string()))?;
    let sample_rate = config.sample_rate().0;
    let channels = config.channels();
    let stream_config = config.config();
    let samples = Arc::new(Mutex::new(Vec::<f32>::new()));
    let sink = Arc::clone(&samples);
    let err_fn = |error| eprintln!("microphone stream error: {error}");

    let stream = match config.sample_format() {
        cpal::SampleFormat::F32 => device
            .build_input_stream(
                &stream_config,
                move |data: &[f32], _| append_f32(data, &sink),
                err_fn,
                None,
            )
            .map_err(|error| AudioError::Backend(error.to_string()))?,
        cpal::SampleFormat::I16 => device
            .build_input_stream(
                &stream_config,
                move |data: &[i16], _| append_i16(data, &sink),
                err_fn,
                None,
            )
            .map_err(|error| AudioError::Backend(error.to_string()))?,
        cpal::SampleFormat::U16 => device
            .build_input_stream(
                &stream_config,
                move |data: &[u16], _| append_u16(data, &sink),
                err_fn,
                None,
            )
            .map_err(|error| AudioError::Backend(error.to_string()))?,
        other => {
            return Err(AudioError::UnsupportedFormat(format!(
                "microphone sample format {other:?}"
            )))
        }
    };

    stream
        .play()
        .map_err(|error| AudioError::Backend(error.to_string()))?;
    std::thread::sleep(Duration::from_secs(seconds.into()));
    drop(stream);

    let samples = samples
        .lock()
        .map_err(|error| AudioError::Backend(error.to_string()))?
        .clone();
    write_i16_wav(path, &samples, sample_rate, channels)?;
    Ok(CaptureArtifact::from_samples(
        path,
        &samples,
        sample_rate,
        channels,
    ))
}

fn append_f32(data: &[f32], sink: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = sink.lock() {
        samples.extend(data.iter().map(|sample| sample.clamp(-1.0, 1.0)));
    }
}

fn append_i16(data: &[i16], sink: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = sink.lock() {
        samples.extend(
            data.iter()
                .map(|sample| f32::from(*sample) / f32::from(i16::MAX)),
        );
    }
}

fn append_u16(data: &[u16], sink: &Arc<Mutex<Vec<f32>>>) {
    if let Ok(mut samples) = sink.lock() {
        samples.extend(
            data.iter()
                .map(|sample| ((*sample as f32 / u16::MAX as f32) * 2.0) - 1.0),
        );
    }
}

#[cfg(target_os = "windows")]
fn capture_system_loopback(path: &Path, seconds: u32) -> Result<CaptureArtifact, AudioError> {
    wasapi_loopback::capture(path, seconds)
}

#[cfg(not(target_os = "windows"))]
fn capture_system_loopback(path: &Path, _seconds: u32) -> Result<CaptureArtifact, AudioError> {
    Err(AudioError::UnsupportedFormat(format!(
        "system loopback capture is only available on Windows: {}",
        path.display()
    )))
}

fn write_i16_wav(
    path: &Path,
    samples: &[f32],
    sample_rate: u32,
    channels: u16,
) -> Result<(), AudioError> {
    let spec = WavSpec {
        channels,
        sample_rate,
        bits_per_sample: 16,
        sample_format: WavSampleFormat::Int,
    };
    let mut writer = WavWriter::create(path, spec)?;
    for sample in samples {
        let scaled = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
        writer.write_sample(scaled)?;
    }
    writer.finalize()?;
    Ok(())
}

impl CaptureArtifact {
    fn from_samples(path: &Path, samples: &[f32], sample_rate: u32, channels: u16) -> Self {
        let non_zero_samples = samples
            .iter()
            .filter(|sample| sample.abs() > 0.0001)
            .count();
        let rms = if samples.is_empty() {
            0.0
        } else {
            (samples.iter().map(|sample| sample * sample).sum::<f32>() / samples.len() as f32)
                .sqrt()
        };
        let duration_seconds = if sample_rate == 0 || channels == 0 {
            0.0
        } else {
            samples.len() as f32 / sample_rate as f32 / channels as f32
        };
        Self {
            path: path.display().to_string(),
            duration_seconds,
            sample_rate,
            channels,
            rms,
            non_zero_samples,
            error: None,
        }
    }

    fn failed(path: &Path, error: AudioError) -> Self {
        Self {
            path: path.display().to_string(),
            duration_seconds: 0.0,
            sample_rate: 0,
            channels: 0,
            rms: 0.0,
            non_zero_samples: 0,
            error: Some(error.to_string()),
        }
    }
}

#[cfg(target_os = "windows")]
mod wasapi_loopback {
    use super::{write_i16_wav, AudioError, CaptureArtifact};
    use std::path::Path;
    use std::ptr::null_mut;
    use std::thread::sleep;
    use std::time::{Duration, Instant};
    use windows::Win32::Foundation::RPC_E_CHANGED_MODE;
    use windows::Win32::Media::Audio::{
        eConsole, eRender, IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator,
        MMDeviceEnumerator, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
        AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
    };
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_ALL,
        COINIT_MULTITHREADED,
    };

    const WAVE_FORMAT_PCM: u16 = 0x0001;
    const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
    const WAVE_FORMAT_EXTENSIBLE: u16 = 0xfffe;

    pub fn capture(path: &Path, seconds: u32) -> Result<CaptureArtifact, AudioError> {
        let should_uninitialize = unsafe {
            let hr = CoInitializeEx(None, COINIT_MULTITHREADED);
            if hr.is_ok() {
                true
            } else if hr == RPC_E_CHANGED_MODE {
                false
            } else {
                return Err(AudioError::Windows(format!("{hr:?}")));
            }
        };
        let _com = ComGuard {
            should_uninitialize,
        };
        unsafe { capture_inner(path, seconds) }
    }

    struct ComGuard {
        should_uninitialize: bool,
    }

    impl Drop for ComGuard {
        fn drop(&mut self) {
            if self.should_uninitialize {
                unsafe {
                    CoUninitialize();
                }
            }
        }
    }

    unsafe fn capture_inner(path: &Path, seconds: u32) -> Result<CaptureArtifact, AudioError> {
        let enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                .map_err(|error| AudioError::Windows(error.to_string()))?;
        let device = enumerator
            .GetDefaultAudioEndpoint(eRender, eConsole)
            .map_err(|error| AudioError::Windows(error.to_string()))?;
        let audio_client: IAudioClient = device
            .Activate(CLSCTX_ALL, None)
            .map_err(|error| AudioError::Windows(error.to_string()))?;

        let format_ptr: *mut WAVEFORMATEX = audio_client
            .GetMixFormat()
            .map_err(|error| AudioError::Windows(error.to_string()))?;
        if format_ptr.is_null() {
            return Err(AudioError::UnsupportedFormat(
                "WASAPI returned no mix format".to_string(),
            ));
        }

        let format = *format_ptr;
        let sample_rate = format.nSamplesPerSec;
        let channels = format.nChannels;
        let bits_per_sample = format.wBitsPerSample;
        let block_align = usize::from(format.nBlockAlign);
        let format_tag = format.wFormatTag;

        audio_client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                10_000_000,
                0,
                format_ptr,
                None,
            )
            .map_err(|error| {
                CoTaskMemFree(Some(format_ptr.cast()));
                AudioError::Windows(error.to_string())
            })?;

        let capture_client: IAudioCaptureClient = audio_client.GetService().map_err(|error| {
            CoTaskMemFree(Some(format_ptr.cast()));
            AudioError::Windows(error.to_string())
        })?;

        audio_client.Start().map_err(|error| {
            CoTaskMemFree(Some(format_ptr.cast()));
            AudioError::Windows(error.to_string())
        })?;

        let deadline = Instant::now() + Duration::from_secs(seconds.into());
        let mut samples = Vec::<f32>::new();

        while Instant::now() < deadline {
            let packet_size = capture_client
                .GetNextPacketSize()
                .map_err(|error| AudioError::Windows(error.to_string()))?;

            if packet_size == 0 {
                sleep(Duration::from_millis(10));
                continue;
            }

            let mut data_ptr = null_mut();
            let mut frame_count = 0;
            let mut flags = 0;
            let mut device_position = 0;
            let mut qpc_position = 0;
            capture_client
                .GetBuffer(
                    &mut data_ptr,
                    &mut frame_count,
                    &mut flags,
                    Some(&mut device_position),
                    Some(&mut qpc_position),
                )
                .map_err(|error| AudioError::Windows(error.to_string()))?;

            let frame_samples = frame_count as usize * channels as usize;
            if flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32 != 0 {
                samples.extend(std::iter::repeat(0.0).take(frame_samples));
            } else {
                let byte_len = frame_count as usize * block_align;
                let bytes = std::slice::from_raw_parts(data_ptr.cast::<u8>(), byte_len);
                append_wasapi_samples(bytes, format_tag, bits_per_sample, &mut samples)?;
            }

            capture_client
                .ReleaseBuffer(frame_count)
                .map_err(|error| AudioError::Windows(error.to_string()))?;
        }

        let _ = audio_client.Stop();
        CoTaskMemFree(Some(format_ptr.cast()));

        write_i16_wav(path, &samples, sample_rate, channels)?;
        Ok(CaptureArtifact::from_samples(
            path,
            &samples,
            sample_rate,
            channels,
        ))
    }

    fn append_wasapi_samples(
        bytes: &[u8],
        format_tag: u16,
        bits_per_sample: u16,
        samples: &mut Vec<f32>,
    ) -> Result<(), AudioError> {
        let treat_as_float = format_tag == WAVE_FORMAT_IEEE_FLOAT
            || (format_tag == WAVE_FORMAT_EXTENSIBLE && bits_per_sample == 32);
        let treat_as_pcm = format_tag == WAVE_FORMAT_PCM || format_tag == WAVE_FORMAT_EXTENSIBLE;

        if treat_as_float && bits_per_sample == 32 {
            for chunk in bytes.chunks_exact(4) {
                samples.push(
                    f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]).clamp(-1.0, 1.0),
                );
            }
            return Ok(());
        }

        if treat_as_pcm && bits_per_sample == 16 {
            for chunk in bytes.chunks_exact(2) {
                let sample = i16::from_le_bytes([chunk[0], chunk[1]]);
                samples.push(f32::from(sample) / f32::from(i16::MAX));
            }
            return Ok(());
        }

        if treat_as_pcm && bits_per_sample == 24 {
            for chunk in bytes.chunks_exact(3) {
                let value = i32::from_le_bytes([
                    chunk[0],
                    chunk[1],
                    chunk[2],
                    if chunk[2] & 0x80 == 0 { 0 } else { 0xff },
                ]);
                samples.push((value as f32 / 8_388_607.0).clamp(-1.0, 1.0));
            }
            return Ok(());
        }

        Err(AudioError::UnsupportedFormat(format!(
            "WASAPI format tag {format_tag}, bits {bits_per_sample}"
        )))
    }
}

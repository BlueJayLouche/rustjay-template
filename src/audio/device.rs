//! Audio device enumeration and stream construction.

use crate::audio::fft::{AudioConfig, AudioOutput, process_audio_frame};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use realfft::RealFftPlanner;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// List available audio input devices
pub fn list_audio_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices.filter_map(|d| d.name().ok()).collect(),
        Err(_) => Vec::new(),
    }
}

/// Get default audio input device name
pub fn default_audio_device() -> Option<String> {
    let host = cpal::default_host();
    host.default_input_device().and_then(|d| d.name().ok())
}

pub fn build_stream_f32(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_rate: f32,
    channels: usize,
    running: Arc<AtomicBool>,
    output: Arc<AudioOutput>,
    audio_config: Arc<AudioConfig>,
    stream_error: Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream> {
    let fft_size = 1024;
    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(fft_size);
    let mut input_buffer: Vec<f32> = Vec::with_capacity(fft_size);
    let mut scratch = r2c.make_scratch_vec();

    let mut beat_energy = 0.0f32;
    let mut beat_history: Vec<f32> = Vec::with_capacity(43);
    let mut beat_counter = 0u32;

    let stream = device.build_input_stream(
        config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            if !running.load(Ordering::SeqCst) {
                return;
            }
            let mono_samples: Vec<f32> = data
                .chunks(channels)
                .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                .collect();
            input_buffer.extend_from_slice(&mono_samples);
            while input_buffer.len() >= fft_size {
                let frame: Vec<f32> = input_buffer.drain(..fft_size).collect();
                process_audio_frame(
                    &frame, sample_rate, fft_size, &r2c, &mut scratch,
                    &mut beat_energy, &mut beat_history, &mut beat_counter,
                    &output, &audio_config,
                );
            }
        },
        move |err| {
            log::error!("Audio stream error: {}", err);
            stream_error.store(true, Ordering::Relaxed);
        },
        None,
    )?;

    Ok(stream)
}

pub fn build_stream_i16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_rate: f32,
    channels: usize,
    running: Arc<AtomicBool>,
    output: Arc<AudioOutput>,
    audio_config: Arc<AudioConfig>,
    stream_error: Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream> {
    let fft_size = 1024;
    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(fft_size);
    let mut input_buffer: Vec<f32> = Vec::with_capacity(fft_size);
    let mut scratch = r2c.make_scratch_vec();

    let mut beat_energy = 0.0f32;
    let mut beat_history: Vec<f32> = Vec::with_capacity(43);
    let mut beat_counter = 0u32;

    let stream = device.build_input_stream(
        config,
        move |data: &[i16], _: &cpal::InputCallbackInfo| {
            if !running.load(Ordering::SeqCst) {
                return;
            }
            let mono_samples: Vec<f32> = data
                .chunks(channels)
                .map(|chunk| {
                    let sum: i32 = chunk.iter().map(|&s| s as i32).sum();
                    (sum as f32 / channels as f32) / 32768.0
                })
                .collect();
            input_buffer.extend_from_slice(&mono_samples);
            while input_buffer.len() >= fft_size {
                let frame: Vec<f32> = input_buffer.drain(..fft_size).collect();
                process_audio_frame(
                    &frame, sample_rate, fft_size, &r2c, &mut scratch,
                    &mut beat_energy, &mut beat_history, &mut beat_counter,
                    &output, &audio_config,
                );
            }
        },
        move |err| {
            log::error!("Audio stream error: {}", err);
            stream_error.store(true, Ordering::Relaxed);
        },
        None,
    )?;

    Ok(stream)
}

pub fn build_stream_u16(
    device: &cpal::Device,
    config: &cpal::StreamConfig,
    sample_rate: f32,
    channels: usize,
    running: Arc<AtomicBool>,
    output: Arc<AudioOutput>,
    audio_config: Arc<AudioConfig>,
    stream_error: Arc<AtomicBool>,
) -> anyhow::Result<cpal::Stream> {
    let fft_size = 1024;
    let mut planner = RealFftPlanner::<f32>::new();
    let r2c = planner.plan_fft_forward(fft_size);
    let mut input_buffer: Vec<f32> = Vec::with_capacity(fft_size);
    let mut scratch = r2c.make_scratch_vec();

    let mut beat_energy = 0.0f32;
    let mut beat_history: Vec<f32> = Vec::with_capacity(43);
    let mut beat_counter = 0u32;

    let stream = device.build_input_stream(
        config,
        move |data: &[u16], _: &cpal::InputCallbackInfo| {
            if !running.load(Ordering::SeqCst) {
                return;
            }
            let mono_samples: Vec<f32> = data
                .chunks(channels)
                .map(|chunk| {
                    let sum: u32 = chunk.iter().map(|&s| s as u32).sum();
                    ((sum as f32 / channels as f32) / 32768.0) - 1.0
                })
                .collect();
            input_buffer.extend_from_slice(&mono_samples);
            while input_buffer.len() >= fft_size {
                let frame: Vec<f32> = input_buffer.drain(..fft_size).collect();
                process_audio_frame(
                    &frame, sample_rate, fft_size, &r2c, &mut scratch,
                    &mut beat_energy, &mut beat_history, &mut beat_counter,
                    &output, &audio_config,
                );
            }
        },
        move |err| {
            log::error!("Audio stream error: {}", err);
            stream_error.store(true, Ordering::Relaxed);
        },
        None,
    )?;

    Ok(stream)
}

//! # Audio Module
//!
//! Real-time audio analysis with FFT and beat detection.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// Audio analyzer running in real-time
pub struct AudioAnalyzer {
    stream: Option<cpal::Stream>,
    running: Arc<AtomicBool>,
    shared_data: Arc<Mutex<AudioData>>,
}

/// Shared audio analysis data
#[derive(Debug, Clone)]
struct AudioData {
    fft: [f32; 8],
    volume: f32,
    beat: bool,
    beat_phase: f32,
    amplitude: f32,
    smoothing: f32,
}

impl Default for AudioData {
    fn default() -> Self {
        Self {
            fft: [0.0; 8],
            volume: 0.0,
            beat: false,
            beat_phase: 0.0,
            amplitude: 1.0,
            smoothing: 0.5,
        }
    }
}

impl AudioAnalyzer {
    /// Create a new audio analyzer
    pub fn new() -> Self {
        Self {
            stream: None,
            running: Arc::new(AtomicBool::new(false)),
            shared_data: Arc::new(Mutex::new(AudioData::default())),
        }
    }

    /// Start audio analysis
    pub fn start(&mut self) -> anyhow::Result<()> {
        if self.stream.is_some() {
            return Ok(());
        }

        let host = cpal::default_host();
        let device = host
            .default_input_device()
            .ok_or_else(|| anyhow::anyhow!("No default input device"))?;

        let config = device.default_input_config()?;
        log::info!("Audio config: {:?}", config);

        let running = Arc::clone(&self.running);
        let shared_data = Arc::clone(&self.shared_data);

        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                Self::build_stream_f32(&device, &config.into(), sample_rate, channels, running, shared_data)?
            }
            cpal::SampleFormat::I16 => {
                Self::build_stream_i16(&device, &config.into(), sample_rate, channels, running, shared_data)?
            }
            cpal::SampleFormat::U16 => {
                Self::build_stream_u16(&device, &config.into(), sample_rate, channels, running, shared_data)?
            }
            _ => return Err(anyhow::anyhow!("Unsupported sample format")),
        };

        stream.play()?;
        self.stream = Some(stream);
        self.running.store(true, Ordering::SeqCst);

        log::info!("Audio analyzer started");
        Ok(())
    }

    /// Stop audio analysis
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        self.stream = None;
        log::info!("Audio analyzer stopped");
    }

    /// Get current FFT values
    pub fn get_fft(&self) -> [f32; 8] {
        self.shared_data.lock().unwrap().fft
    }

    /// Get current volume
    pub fn get_volume(&self) -> f32 {
        self.shared_data.lock().unwrap().volume
    }

    /// Check if beat detected
    pub fn is_beat(&self) -> bool {
        let mut data = self.shared_data.lock().unwrap();
        let beat = data.beat;
        data.beat = false;
        beat
    }

    /// Get beat phase
    pub fn get_beat_phase(&self) -> f32 {
        self.shared_data.lock().unwrap().beat_phase
    }

    /// Set amplitude
    pub fn set_amplitude(&self, amplitude: f32) {
        self.shared_data.lock().unwrap().amplitude = amplitude;
    }

    /// Set smoothing
    pub fn set_smoothing(&self, smoothing: f32) {
        self.shared_data.lock().unwrap().smoothing = smoothing;
    }

    /// Build audio stream for f32 samples
    fn build_stream_f32(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        sample_rate: f32,
        channels: usize,
        running: Arc<AtomicBool>,
        shared_data: Arc<Mutex<AudioData>>,
    ) -> anyhow::Result<cpal::Stream> {
        let fft_size = 1024;
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c: std::sync::Arc<dyn RealToComplex<f32>> = planner.plan_fft_forward(fft_size);
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

                // Convert to mono f32 (already f32, just average channels)
                let mono_samples: Vec<f32> = data
                    .chunks(channels)
                    .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                    .collect();

                input_buffer.extend_from_slice(&mono_samples);

                while input_buffer.len() >= fft_size {
                    let frame: Vec<f32> = input_buffer.drain(..fft_size).collect();

                    // Process frame
                    process_audio_frame(
                        &frame,
                        sample_rate,
                        fft_size,
                        &r2c,
                        &mut scratch,
                        &mut beat_energy,
                        &mut beat_history,
                        &mut beat_counter,
                        &shared_data,
                    );
                }
            },
            move |err| {
                log::error!("Audio stream error: {}", err);
            },
            None,
        )?;

        Ok(stream)
    }

    /// Build audio stream for i16 samples
    fn build_stream_i16(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        sample_rate: f32,
        channels: usize,
        running: Arc<AtomicBool>,
        shared_data: Arc<Mutex<AudioData>>,
    ) -> anyhow::Result<cpal::Stream> {
        let fft_size = 1024;
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c: std::sync::Arc<dyn RealToComplex<f32>> = planner.plan_fft_forward(fft_size);
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

                // Convert i16 to f32 mono
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
                        &frame,
                        sample_rate,
                        fft_size,
                        &r2c,
                        &mut scratch,
                        &mut beat_energy,
                        &mut beat_history,
                        &mut beat_counter,
                        &shared_data,
                    );
                }
            },
            move |err| {
                log::error!("Audio stream error: {}", err);
            },
            None,
        )?;

        Ok(stream)
    }

    /// Build audio stream for u16 samples
    fn build_stream_u16(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        sample_rate: f32,
        channels: usize,
        running: Arc<AtomicBool>,
        shared_data: Arc<Mutex<AudioData>>,
    ) -> anyhow::Result<cpal::Stream> {
        let fft_size = 1024;
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c: std::sync::Arc<dyn RealToComplex<f32>> = planner.plan_fft_forward(fft_size);
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

                // Convert u16 to f32 mono
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
                        &frame,
                        sample_rate,
                        fft_size,
                        &r2c,
                        &mut scratch,
                        &mut beat_energy,
                        &mut beat_history,
                        &mut beat_counter,
                        &shared_data,
                    );
                }
            },
            move |err| {
                log::error!("Audio stream error: {}", err);
            },
            None,
        )?;

        Ok(stream)
    }
}

impl Default for AudioAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Process a single audio frame
fn process_audio_frame(
    frame: &[f32],
    sample_rate: f32,
    fft_size: usize,
    r2c: &std::sync::Arc<dyn RealToComplex<f32>>,
    scratch: &mut [f32],
    beat_energy: &mut f32,
    beat_history: &mut Vec<f32>,
    beat_counter: &mut u32,
    shared_data: &Arc<Mutex<AudioData>>,
) {
    // Apply Hann window
    let windowed: Vec<f32> = frame
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos());
            s * w
        })
        .collect();

    // Perform FFT
    let mut spectrum = r2c.make_output_vec();
    if r2c.process_with_scratch(&windowed, &mut spectrum, scratch).is_err() {
        return;
    }

    // Calculate magnitude spectrum
    let magnitudes: Vec<f32> = spectrum
        .iter()
        .map(|c| c.norm())
        .collect();

    // Calculate 8 frequency bands
    let bands = calculate_bands(&magnitudes, sample_rate, fft_size);

    // Calculate volume
    let volume: f32 = frame.iter().map(|&s| s.abs()).sum::<f32>() / fft_size as f32;

    // Beat detection
    let instant_energy: f32 = bands.iter().sum();
    beat_history.push(instant_energy);
    if beat_history.len() > 43 {
        beat_history.remove(0);
    }

    let local_average = if beat_history.len() >= 43 {
        beat_history.iter().sum::<f32>() / beat_history.len() as f32
    } else {
        instant_energy
    };

    let variance: f32 = beat_history
        .iter()
        .map(|&e| (e - local_average).powi(2))
        .sum::<f32>()
        / beat_history.len().max(1) as f32;
    let sensitivity = (-0.0025714 * variance + 1.5142857).clamp(1.2, 2.0);

    let is_beat = instant_energy > sensitivity * local_average && instant_energy > 0.1;

    if is_beat {
        *beat_counter += 1;
        *beat_energy = instant_energy;
    }

    let phase = ((*beat_counter as f32 + (instant_energy / beat_energy.max(0.001)).min(1.0)) * 0.1) % 1.0;

    // Update shared data
    if let Ok(mut data) = shared_data.lock() {
        let smoothing = data.smoothing;
        let amplitude = data.amplitude;

        for (i, band) in bands.iter().enumerate() {
            data.fft[i] = data.fft[i] * smoothing + band * amplitude * (1.0 - smoothing);
        }

        data.volume = data.volume * smoothing + volume * amplitude * (1.0 - smoothing);

        if is_beat {
            data.beat = true;
        }

        data.beat_phase = phase;
    }
}

/// Calculate 8 logarithmic frequency bands from FFT magnitudes
fn calculate_bands(magnitudes: &[f32], sample_rate: f32, fft_size: usize) -> [f32; 8] {
    let mut bands = [0.0f32; 8];
    let freq_resolution = sample_rate / fft_size as f32;

    let ranges = [
        (20.0, 60.0),
        (60.0, 120.0),
        (120.0, 250.0),
        (250.0, 500.0),
        (500.0, 1000.0),
        (1000.0, 2000.0),
        (2000.0, 4000.0),
        (4000.0, 8000.0),
    ];

    for (i, (low, high)) in ranges.iter().enumerate() {
        let low_bin = (low / freq_resolution) as usize;
        let high_bin = (high / freq_resolution).min(magnitudes.len() - 1) as usize;

        if low_bin < magnitudes.len() && high_bin > low_bin {
            let sum: f32 = magnitudes[low_bin..=high_bin].iter().sum();
            bands[i] = sum / (high_bin - low_bin + 1) as f32;
        }
    }

    let max_band = bands.iter().cloned().fold(0.0f32, f32::max).max(0.001);
    for band in bands.iter_mut() {
        *band = (*band / max_band).min(1.0);
    }

    bands
}

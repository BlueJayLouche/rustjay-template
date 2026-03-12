//! # Audio Module
//!
//! Real-time audio analysis with FFT and beat detection.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use realfft::RealFftPlanner;
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
    /// 8-band FFT values (normalized 0-1)
    fft: [f32; 8],
    /// Overall volume
    volume: f32,
    /// Beat detected
    beat: bool,
    /// Beat phase (0-1)
    beat_phase: f32,
    /// Amplitude multiplier
    amplitude: f32,
    /// Smoothing factor
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

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => Self::build_stream::<f32>(
                &device,
                &config.into(),
                running,
                shared_data,
            )?,
            cpal::SampleFormat::I16 => Self::build_stream::<i16>(
                &device,
                &config.into(),
                running,
                shared_data,
            )?,
            cpal::SampleFormat::U16 => Self::build_stream::<u16>(
                &device,
                &config.into(),
                running,
                shared_data,
            )?,
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
        data.beat = false; // Clear beat flag
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

    /// Build audio stream for given sample type
    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        running: Arc<AtomicBool>,
        shared_data: Arc<Mutex<AudioData>>,
    ) -> anyhow::Result<cpal::Stream>
    where
        T: cpal::SizedSample + cpal::FromSample<f32> + cpal::ToSample<f32>,
    {
        let sample_rate = config.sample_rate.0 as f32;
        let channels = config.channels as usize;

        // FFT setup
        let fft_size = 1024;
        let mut planner = RealFftPlanner::<f32>::new();
        let r2c = planner.plan_fft_forward(fft_size);
        let mut input_buffer: Vec<f32> = Vec::with_capacity(fft_size);
        let mut scratch = r2c.make_scratch_vec();

        // Beat detection state
        let mut beat_energy = 0.0f32;
        let mut beat_history: Vec<f32> = Vec::with_capacity(43); // ~1 second at 44.1kHz
        let mut beat_counter = 0u32;

        let stream = device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                if !running.load(Ordering::SeqCst) {
                    return;
                }

                // Convert to mono f32
                let mono_samples: Vec<f32> = data
                    .chunks(channels)
                    .map(|chunk| {
                        chunk.iter().map(|&s| s.to_sample::<f32>()).sum::<f32>() / channels as f32
                    })
                    .collect();

                // Add to input buffer
                input_buffer.extend_from_slice(&mono_samples);

                // Process when we have enough samples
                while input_buffer.len() >= fft_size {
                    let frame: Vec<f32> = input_buffer.drain(..fft_size).collect();

                    // Apply window function (Hann)
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
                    if let Err(e) = r2c.process_with_scratch(&windowed, &mut spectrum, &mut scratch) {
                        log::warn!("FFT error: {}", e);
                        continue;
                    }

                    // Calculate magnitude spectrum
                    let magnitudes: Vec<f32> = spectrum
                        .iter()
                        .map(|c| c.norm())
                        .collect();

                    // Calculate 8 frequency bands (logarithmic scale)
                    let bands = Self::calculate_bands(&magnitudes, sample_rate, fft_size);

                    // Calculate volume
                    let volume: f32 = frame.iter().map(|&s| s.abs()).sum::<f32>() / fft_size as f32;

                    // Beat detection using energy-based algorithm
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
                        beat_counter += 1;
                        beat_energy = instant_energy;
                    }

                    // Calculate beat phase (assuming 120 BPM as base)
                    let bpm = 120.0f32;
                    let beat_duration = 60.0 / bpm;
                    let phase = ((beat_counter as f32 + (instant_energy / beat_energy.max(0.001)).min(1.0)) * 0.1) % 1.0;

                    // Update shared data
                    if let Ok(mut data) = shared_data.lock() {
                        let smoothing = data.smoothing;
                        let amplitude = data.amplitude;

                        // Apply smoothing to FFT
                        for (i, band) in bands.iter().enumerate() {
                            data.fft[i] = data.fft[i] * smoothing + band * amplitude * (1.0 - smoothing);
                        }

                        // Smooth volume
                        data.volume = data.volume * smoothing + volume * amplitude * (1.0 - smoothing);

                        // Set beat flag
                        if is_beat {
                            data.beat = true;
                        }

                        data.beat_phase = phase;
                    }
                }
            },
            move |err| {
                log::error!("Audio stream error: {}", err);
            },
            None,
        )?;

        Ok(stream)
    }

    /// Calculate 8 logarithmic frequency bands from FFT magnitudes
    fn calculate_bands(magnitudes: &[f32], sample_rate: f32, fft_size: usize) -> [f32; 8] {
        let mut bands = [0.0f32; 8];
        let freq_resolution = sample_rate / fft_size as f32;

        // Define frequency ranges for each band (logarithmic)
        let ranges = [
            (20.0, 60.0),      // Sub-bass
            (60.0, 120.0),     // Bass
            (120.0, 250.0),    // Low-mids
            (250.0, 500.0),    // Mids
            (500.0, 1000.0),   // High-mids
            (1000.0, 2000.0),  // Presence
            (2000.0, 4000.0),  // Brilliance
            (4000.0, 8000.0),  // Air
        ];

        for (i, (low, high)) in ranges.iter().enumerate() {
            let low_bin = (low / freq_resolution) as usize;
            let high_bin = (high / freq_resolution).min(magnitudes.len() - 1) as usize;

            if low_bin < magnitudes.len() && high_bin > low_bin {
                let sum: f32 = magnitudes[low_bin..=high_bin].iter().sum();
                bands[i] = sum / (high_bin - low_bin + 1) as f32;
            }
        }

        // Normalize
        let max_band = bands.iter().cloned().fold(0.0f32, f32::max).max(0.001);
        for band in bands.iter_mut() {
            *band = (*band / max_band).min(1.0);
        }

        bands
    }
}

impl Default for AudioAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

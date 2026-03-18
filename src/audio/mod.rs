//! # Audio Module
//!
//! Real-time audio analysis with FFT and beat detection.

pub mod routing;

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use realfft::{RealFftPlanner, RealToComplex};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

/// List available audio input devices
pub fn list_audio_devices() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(devices) => devices
            .filter_map(|d| d.name().ok())
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Get default audio input device name
pub fn default_audio_device() -> Option<String> {
    let host = cpal::default_host();
    host.default_input_device()
        .and_then(|d| d.name().ok())
}

// ---------------------------------------------------------------------------
// Lock-free audio output (written by real-time callback, read by main thread)
// ---------------------------------------------------------------------------

struct AudioOutput {
    fft: [AtomicU32; 8],
    volume: AtomicU32,
    /// Set true by callback; atomically swapped false when read by main thread
    beat: AtomicBool,
    beat_phase: AtomicU32,
}

impl AudioOutput {
    fn new() -> Self {
        Self {
            fft: std::array::from_fn(|_| AtomicU32::new(0)),
            volume: AtomicU32::new(0),
            beat: AtomicBool::new(false),
            beat_phase: AtomicU32::new(0),
        }
    }

    fn reset(&self) {
        for f in &self.fft {
            f.store(0f32.to_bits(), Ordering::Relaxed);
        }
        self.volume.store(0f32.to_bits(), Ordering::Relaxed);
        self.beat.store(false, Ordering::Relaxed);
        self.beat_phase.store(0f32.to_bits(), Ordering::Relaxed);
    }
}

// ---------------------------------------------------------------------------
// Lock-free audio config (written by main thread, read by real-time callback)
// ---------------------------------------------------------------------------

struct AudioConfig {
    amplitude: AtomicU32,
    smoothing: AtomicU32,
    normalize: AtomicBool,
    pink_noise_shaping: AtomicBool,
}

impl AudioConfig {
    fn new() -> Self {
        Self {
            amplitude: AtomicU32::new(1.0f32.to_bits()),
            smoothing: AtomicU32::new(0.5f32.to_bits()),
            normalize: AtomicBool::new(true),
            pink_noise_shaping: AtomicBool::new(false),
        }
    }

    fn amplitude(&self) -> f32 {
        f32::from_bits(self.amplitude.load(Ordering::Relaxed))
    }
    fn smoothing(&self) -> f32 {
        f32::from_bits(self.smoothing.load(Ordering::Relaxed))
    }
}

// ---------------------------------------------------------------------------
// Public AudioAnalyzer
// ---------------------------------------------------------------------------

/// Audio analyzer running in real-time
pub struct AudioAnalyzer {
    stream: Option<cpal::Stream>,
    running: Arc<AtomicBool>,
    /// Set true by the cpal error callback; checked by main thread for reconnect
    stream_error: Arc<AtomicBool>,
    /// Lock-free: written by audio callback, read by main thread
    output: Arc<AudioOutput>,
    /// Lock-free: written by main thread, read by audio callback
    config: Arc<AudioConfig>,
}

impl AudioAnalyzer {
    /// Create a new audio analyzer
    pub fn new() -> Self {
        Self {
            stream: None,
            running: Arc::new(AtomicBool::new(false)),
            stream_error: Arc::new(AtomicBool::new(false)),
            output: Arc::new(AudioOutput::new()),
            config: Arc::new(AudioConfig::new()),
        }
    }

    /// Returns true if the stream encountered an error since the last call
    /// (atomically clears the flag)
    pub fn take_stream_error(&self) -> bool {
        self.stream_error.swap(false, Ordering::Relaxed)
    }

    /// Start audio analysis with default device
    pub fn start(&mut self) -> anyhow::Result<()> {
        self.start_with_device(None)
    }

    /// Start audio analysis with specific device (None for default)
    pub fn start_with_device(&mut self, device_name: Option<&str>) -> anyhow::Result<()> {
        log::info!("[Audio] start_with_device called with: {:?}", device_name);

        if self.stream.is_some() {
            log::info!("[Audio] Stopping existing stream first");
            self.stop();
        }

        let host = cpal::default_host();

        // List available devices for debugging
        match host.input_devices() {
            Ok(devices) => {
                log::info!("[Audio] Available input devices:");
                for d in devices {
                    if let Ok(name) = d.name() {
                        log::info!("  - {}", name);
                    }
                }
            }
            Err(e) => log::warn!("[Audio] Failed to list input devices: {}", e),
        }

        // Select device
        let device = match device_name {
            Some(name) => {
                log::info!("[Audio] Looking for device: '{}'", name);
                host.input_devices()?
                    .find(|d| {
                        let dev_name = d.name().ok();
                        let matches = dev_name.as_deref() == Some(name);
                        log::debug!("[Audio] Checking device: {:?}, matches: {}", dev_name, matches);
                        matches
                    })
                    .ok_or_else(|| anyhow::anyhow!("Audio device '{}' not found", name))?
            }
            None => {
                log::info!("[Audio] Using default input device");
                host.default_input_device()
                    .ok_or_else(|| anyhow::anyhow!("No default input device"))?
            }
        };

        log::info!("[Audio] Selected device: {:?}", device.name()?);

        // Reset output data when starting a new device
        self.output.reset();

        let config = device.default_input_config()?;
        log::info!("Audio config: {:?}", config);

        let running = Arc::clone(&self.running);
        let output = Arc::clone(&self.output);
        let audio_config = Arc::clone(&self.config);
        let stream_error = Arc::clone(&self.stream_error);

        let sample_rate = config.sample_rate().0 as f32;
        let channels = config.channels() as usize;

        let stream = match config.sample_format() {
            cpal::SampleFormat::F32 => {
                Self::build_stream_f32(&device, &config.into(), sample_rate, channels, running, output, audio_config, stream_error)?
            }
            cpal::SampleFormat::I16 => {
                Self::build_stream_i16(&device, &config.into(), sample_rate, channels, running, output, audio_config, stream_error)?
            }
            cpal::SampleFormat::U16 => {
                Self::build_stream_u16(&device, &config.into(), sample_rate, channels, running, output, audio_config, stream_error)?
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
        self.output.reset();
        log::info!("Audio analyzer stopped");
    }
}

impl Drop for AudioAnalyzer {
    fn drop(&mut self) {
        self.stop();
    }
}

impl AudioAnalyzer {
    /// Get current FFT values
    pub fn get_fft(&self) -> [f32; 8] {
        std::array::from_fn(|i| f32::from_bits(self.output.fft[i].load(Ordering::Relaxed)))
    }

    /// Get current volume
    pub fn get_volume(&self) -> f32 {
        f32::from_bits(self.output.volume.load(Ordering::Relaxed))
    }

    /// Check if beat was detected since last call (atomically clears the flag)
    pub fn is_beat(&self) -> bool {
        self.output.beat.swap(false, Ordering::Relaxed)
    }

    /// Get beat phase
    pub fn get_beat_phase(&self) -> f32 {
        f32::from_bits(self.output.beat_phase.load(Ordering::Relaxed))
    }

    /// Set amplitude multiplier
    pub fn set_amplitude(&self, amplitude: f32) {
        self.config.amplitude.store(amplitude.to_bits(), Ordering::Relaxed);
    }

    /// Set smoothing factor
    pub fn set_smoothing(&self, smoothing: f32) {
        self.config.smoothing.store(smoothing.clamp(0.0, 0.99).to_bits(), Ordering::Relaxed);
    }

    /// Get normalization enabled
    pub fn get_normalize(&self) -> bool {
        self.config.normalize.load(Ordering::Relaxed)
    }

    /// Set normalization enabled
    pub fn set_normalize(&self, normalize: bool) {
        self.config.normalize.store(normalize, Ordering::Relaxed);
    }

    /// Get pink noise shaping (+3dB/octave) enabled
    pub fn get_pink_noise_shaping(&self) -> bool {
        self.config.pink_noise_shaping.load(Ordering::Relaxed)
    }

    /// Set pink noise shaping (+3dB/octave) enabled
    pub fn set_pink_noise_shaping(&self, enabled: bool) {
        self.config.pink_noise_shaping.store(enabled, Ordering::Relaxed);
    }

    /// Build audio stream for f32 samples
    fn build_stream_f32(
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

    /// Build audio stream for i16 samples
    fn build_stream_i16(
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

    /// Build audio stream for u16 samples
    fn build_stream_u16(
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
}

impl Default for AudioAnalyzer {
    fn default() -> Self {
        Self::new()
    }
}

/// Process a single audio frame — runs on the real-time audio callback thread.
/// Reads config atomically, writes results atomically. No mutex involved.
fn process_audio_frame(
    frame: &[f32],
    sample_rate: f32,
    fft_size: usize,
    r2c: &std::sync::Arc<dyn RealToComplex<f32>>,
    scratch: &mut [rustfft::num_complex::Complex<f32>],
    beat_energy: &mut f32,
    beat_history: &mut Vec<f32>,
    beat_counter: &mut u32,
    output: &Arc<AudioOutput>,
    config: &Arc<AudioConfig>,
) {
    use rustfft::num_complex::Complex;

    // Apply Hann window
    let mut windowed: Vec<f32> = frame
        .iter()
        .enumerate()
        .map(|(i, &s)| {
            let w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos());
            s * w
        })
        .collect();

    // Perform FFT
    let mut spectrum: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); fft_size / 2 + 1];
    if r2c.process_with_scratch(&mut windowed, &mut spectrum, scratch).is_err() {
        return;
    }

    let magnitudes: Vec<f32> = spectrum.iter().map(|c| c.norm()).collect();
    let bands = calculate_bands(&magnitudes, sample_rate, fft_size);
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

    // Read config atomically (no mutex, no blocking)
    let smoothing = config.smoothing();
    let amplitude = config.amplitude();
    let normalize = config.normalize.load(Ordering::Relaxed);
    let pink_noise_shaping = config.pink_noise_shaping.load(Ordering::Relaxed);

    let max_band = bands.iter().cloned().fold(0.0f32, f32::max).max(0.001);

    // Write results atomically (no mutex, no blocking)
    for (i, band) in bands.iter().enumerate() {
        let pink_factor = if pink_noise_shaping {
            1.0 + (i as f32 * 0.26)
        } else {
            1.0
        };

        let normalized_band = if normalize {
            (band / max_band) * pink_factor
        } else {
            band * pink_factor
        };

        let prev = f32::from_bits(output.fft[i].load(Ordering::Relaxed));
        let smoothed = prev * smoothing + normalized_band * (1.0 - smoothing);
        output.fft[i].store((smoothed * amplitude).to_bits(), Ordering::Relaxed);
    }

    let prev_volume = f32::from_bits(output.volume.load(Ordering::Relaxed));
    let smoothed_volume = prev_volume * smoothing + volume * (1.0 - smoothing);
    output.volume.store((smoothed_volume * amplitude).to_bits(), Ordering::Relaxed);

    if is_beat {
        output.beat.store(true, Ordering::Relaxed);
    }

    output.beat_phase.store(phase.to_bits(), Ordering::Relaxed);
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
        let high_bin = ((high / freq_resolution) as usize).min(magnitudes.len().saturating_sub(1));

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

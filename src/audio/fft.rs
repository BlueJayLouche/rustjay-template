//! Lock-free audio I/O types and real-time FFT processing.
//!
//! All types in this module are safe to use from the real-time audio callback:
//! no allocations, no mutexes — only atomics.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Lock-free audio output (written by real-time callback, read by main thread)
// ---------------------------------------------------------------------------

pub(crate) struct AudioOutput {
    pub fft: [AtomicU32; 8],
    pub volume: AtomicU32,
    /// Set true by callback; atomically swapped false when read by main thread
    pub beat: AtomicBool,
    pub beat_phase: AtomicU32,
}

impl AudioOutput {
    pub fn new() -> Self {
        Self {
            fft: std::array::from_fn(|_| AtomicU32::new(0)),
            volume: AtomicU32::new(0),
            beat: AtomicBool::new(false),
            beat_phase: AtomicU32::new(0),
        }
    }

    pub fn reset(&self) {
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

pub(crate) struct AudioConfig {
    pub amplitude: AtomicU32,
    pub smoothing: AtomicU32,
    pub normalize: AtomicBool,
    pub pink_noise_shaping: AtomicBool,
}

impl AudioConfig {
    pub fn new() -> Self {
        Self {
            amplitude: AtomicU32::new(1.0f32.to_bits()),
            smoothing: AtomicU32::new(0.5f32.to_bits()),
            normalize: AtomicBool::new(true),
            pink_noise_shaping: AtomicBool::new(false),
        }
    }

    pub fn amplitude(&self) -> f32 {
        f32::from_bits(self.amplitude.load(Ordering::Relaxed))
    }

    pub fn smoothing(&self) -> f32 {
        f32::from_bits(self.smoothing.load(Ordering::Relaxed))
    }
}

// ---------------------------------------------------------------------------
// Real-time audio frame processing
// ---------------------------------------------------------------------------

/// Process a single audio frame — runs on the real-time audio callback thread.
/// Reads config atomically, writes results atomically. No mutex involved.
pub fn process_audio_frame(
    frame: &[f32],
    sample_rate: f32,
    fft_size: usize,
    r2c: &std::sync::Arc<dyn realfft::RealToComplex<f32>>,
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
            let w =
                0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / fft_size as f32).cos());
            s * w
        })
        .collect();

    // Perform FFT
    let mut spectrum: Vec<Complex<f32>> = vec![Complex::new(0.0, 0.0); fft_size / 2 + 1];
    if r2c
        .process_with_scratch(&mut windowed, &mut spectrum, scratch)
        .is_err()
    {
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

    let phase = ((*beat_counter as f32
        + (instant_energy / beat_energy.max(0.001)).min(1.0))
        * 0.1)
        % 1.0;

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
    output
        .volume
        .store((smoothed_volume * amplitude).to_bits(), Ordering::Relaxed);

    if is_beat {
        output.beat.store(true, Ordering::Relaxed);
    }

    output.beat_phase.store(phase.to_bits(), Ordering::Relaxed);
}

/// Calculate 8 logarithmic frequency bands from FFT magnitudes
pub fn calculate_bands(magnitudes: &[f32], sample_rate: f32, fft_size: usize) -> [f32; 8] {
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
        let high_bin =
            ((high / freq_resolution) as usize).min(magnitudes.len().saturating_sub(1));

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

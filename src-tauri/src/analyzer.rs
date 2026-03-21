use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Target sample rate for analysis (low to speed up processing)
const TARGET_SR: usize = 11025;

/// Hop size in samples for energy computation (~23ms at 11025Hz, ~4x finer than before)
const HOP_SIZE: usize = 256;

/// Window size in samples for RMS energy computation (~46ms at 11025Hz)
const ENERGY_WINDOW: usize = 512;

/// Minimum cooldown between detected peaks in windows (~70ms)
/// Prevents double-triggering on the same transient.
const MIN_PEAK_DISTANCE: usize = 3;

pub fn detect_bpm(path: &Path) -> Option<f64> {
    let samples = decode_to_mono(path)?;

    if samples.len() < ENERGY_WINDOW * 4 {
        return None;
    }

    // Only analyze the middle 30 seconds to avoid intros/outros
    let sr = TARGET_SR;
    let thirty_sec = 30 * sr;
    let analysis_samples = if samples.len() > thirty_sec * 2 {
        let start = (samples.len() - thirty_sec) / 2;
        &samples[start..start + thirty_sec]
    } else {
        &samples
    };

    // Step 1: Compute RMS energy envelope with overlapping windows
    let num_windows = if analysis_samples.len() >= ENERGY_WINDOW {
        (analysis_samples.len() - ENERGY_WINDOW) / HOP_SIZE + 1
    } else {
        0
    };
    if num_windows < 8 {
        return None;
    }

    let mut energy: Vec<f32> = Vec::with_capacity(num_windows);
    for i in 0..num_windows {
        let start = i * HOP_SIZE;
        let end = start + ENERGY_WINDOW;
        let mut sum_sq: f64 = 0.0;
        for j in start..end {
            let s = analysis_samples[j] as f64;
            sum_sq += s * s;
        }
        energy.push((sum_sq / ENERGY_WINDOW as f64).sqrt() as f32);
    }

    // Step 2: Compute onset detection function (spectral flux approximation)
    // Use first-order difference of energy, half-wave rectified (only rises)
    let mut onset_fn: Vec<f32> = vec![0.0; energy.len()];
    for i in 1..energy.len() {
        let diff = energy[i] - energy[i - 1];
        onset_fn[i] = diff.max(0.0);
    }

    // Adaptive threshold: local median + offset over a window of ~0.5 seconds
    let median_window = (500.0 / (HOP_SIZE as f64 / sr as f64 * 1000.0)) as usize;
    let median_window = median_window.max(5);

    let mut peaks: Vec<usize> = Vec::new();
    let mut last_peak: Option<usize> = None;

    for i in 1..onset_fn.len() - 1 {
        // Local peak check
        if onset_fn[i] <= onset_fn[i - 1] || onset_fn[i] <= onset_fn[i + 1] {
            continue;
        }

        // Minimum distance enforcement
        if let Some(lp) = last_peak {
            if i - lp < MIN_PEAK_DISTANCE {
                continue;
            }
        }

        // Adaptive threshold: median of local window * 1.5
        let w_start = i.saturating_sub(median_window / 2);
        let w_end = (i + median_window / 2 + 1).min(onset_fn.len());
        let mut local: Vec<f32> = onset_fn[w_start..w_end].to_vec();
        local.sort_unstable_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let local_median = local[local.len() / 2];
        let threshold = local_median * 2.0 + 1e-6;

        if onset_fn[i] > threshold {
            peaks.push(i);
            last_peak = Some(i);
        }
    }

    if peaks.len() < 4 {
        return None;
    }

    // Step 3: Compute inter-onset intervals (IOI)
    // Consider intervals between peaks at distances 1, 2, and 3 apart
    // This makes the histogram robust to missed beats
    let hop_duration_ms = (HOP_SIZE as f64 / sr as f64) * 1000.0;
    let mut ioi_ms: Vec<f64> = Vec::new();

    for gap in 1..=3usize {
        for i in gap..peaks.len() {
            let interval = (peaks[i] - peaks[i - gap]) as f64 * hop_duration_ms;
            let bpm_for_interval = 60000.0 / interval;
            // Only consider intervals that correspond to 60-220 BPM
            if bpm_for_interval >= 60.0 && bpm_for_interval <= 220.0 {
                ioi_ms.push(interval);
            }
        }
    }

    if ioi_ms.is_empty() {
        return None;
    }

    // Step 4: Build histogram of IOIs with fine bins (2ms for ~0.5 BPM resolution at 128 BPM)
    let bin_size = 2.0;
    let min_ioi = 60000.0 / 220.0; // ~273ms
    let max_ioi = 60000.0 / 60.0;  // 1000ms
    let num_bins = ((max_ioi - min_ioi) / bin_size) as usize + 1;
    let mut histogram = vec![0u32; num_bins];

    for &ioi in &ioi_ms {
        let bin = ((ioi - min_ioi) / bin_size) as usize;
        if bin < num_bins {
            histogram[bin] += 1;
        }
    }

    // Smooth the histogram with a 5-bin Gaussian-like kernel to reduce noise
    let kernel = [1u32, 2, 4, 2, 1];
    let mut smoothed = vec![0u32; num_bins];
    for i in 0..num_bins {
        let mut sum = 0u32;
        for (k, &w) in kernel.iter().enumerate() {
            let idx = i as isize + k as isize - 2;
            if idx >= 0 && (idx as usize) < num_bins {
                sum += histogram[idx as usize] * w;
            }
        }
        smoothed[i] = sum;
    }

    // Find the bin with the most votes
    let best_bin = smoothed
        .iter()
        .enumerate()
        .max_by_key(|(_, &count)| count)
        .map(|(bin, _)| bin)?;

    // Refine with parabolic interpolation around the peak
    let best_ioi_ms = if best_bin > 0 && best_bin < num_bins - 1 {
        let y_prev = smoothed[best_bin - 1] as f64;
        let y_curr = smoothed[best_bin] as f64;
        let y_next = smoothed[best_bin + 1] as f64;
        let offset = 0.5 * (y_prev - y_next) / (y_prev - 2.0 * y_curr + y_next + 1e-10);
        let offset = offset.clamp(-0.5, 0.5);
        min_ioi + (best_bin as f64 + 0.5 + offset) * bin_size
    } else {
        min_ioi + (best_bin as f64 + 0.5) * bin_size
    };

    let mut bpm = 60000.0 / best_ioi_ms;

    // Step 5: Octave correction — prefer range [70, 180]
    // This handles DnB (170-178) and downtempo (70-85) correctly.
    // For the typical electronic library (120-140), this is a no-op.
    while bpm < 70.0 {
        bpm *= 2.0;
    }
    while bpm > 180.0 {
        bpm /= 2.0;
    }

    // Round to 1 decimal
    let bpm = (bpm * 10.0).round() / 10.0;

    Some(bpm)
}

fn decode_to_mono(path: &Path) -> Option<Vec<f32>> {
    let file = File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .ok()?;

    let mut format = probed.format;
    let track = format.default_track()?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate? as usize;

    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    let mut mono_samples: Vec<f32> = Vec::new();
    // Decode full track so we can actually take the middle 30 seconds
    // For very long tracks (>10 min), cap at 8 minutes to avoid excess memory
    let max_samples = 480 * sample_rate;

    loop {
        if mono_samples.len() >= max_samples {
            break;
        }

        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof =>
            {
                break;
            }
            Err(_) => break,
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(symphonia::core::errors::Error::DecodeError(_)) => continue,
            Err(_) => break,
        };

        let spec = *decoded.spec();
        let duration = decoded.capacity() as u64;

        let mut sample_buf = SampleBuffer::<f32>::new(duration, spec);
        sample_buf.copy_interleaved_ref(decoded);

        let samples = sample_buf.samples();
        let ch = spec.channels.count().max(1);
        let frames = sample_buf.len() / ch;

        for f in 0..frames {
            let mut sum: f32 = 0.0;
            for c in 0..ch {
                sum += samples[f * ch + c];
            }
            mono_samples.push(sum / ch as f32);
        }
    }

    if mono_samples.is_empty() {
        return None;
    }

    // Downsample to TARGET_SR with simple low-pass (averaging) anti-alias filter
    if sample_rate > TARGET_SR {
        let ratio = sample_rate as f64 / TARGET_SR as f64;
        let new_len = (mono_samples.len() as f64 / ratio) as usize;
        let mut downsampled = Vec::with_capacity(new_len);

        // Average over a window of `ratio` samples centered on the target index
        // This acts as a simple box-filter low-pass (sinc approximation)
        let half_win = (ratio / 2.0).ceil() as usize;
        for i in 0..new_len {
            let center = (i as f64 * ratio) as usize;
            let start = center.saturating_sub(half_win);
            let end = (center + half_win + 1).min(mono_samples.len());
            let mut sum: f64 = 0.0;
            for j in start..end {
                sum += mono_samples[j] as f64;
            }
            downsampled.push((sum / (end - start) as f64) as f32);
        }
        Some(downsampled)
    } else {
        Some(mono_samples)
    }
}

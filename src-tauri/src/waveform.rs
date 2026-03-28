use std::path::Path;

const NUM_BARS: usize = 128;
const MIN_BAR: u8 = 10;
const DB_FLOOR: f32 = -60.0;

/// Extract waveform data (128 u8 bars) from an audio file.
/// Uses the shared decode pipeline — no duplicate file/probe/decode logic.
pub fn extract_waveform(path: &Path) -> Option<Vec<u8>> {
    let (mono_samples, _sample_rate) = crate::audio::decode::decode_to_mono(path, None)?;
    waveform_from_samples(&mono_samples)
}

/// Compute 128-bar waveform from pre-decoded mono samples.
/// Also used by the combined decode_and_analyze path.
pub fn waveform_from_samples(mono_samples: &[f32]) -> Option<Vec<u8>> {
    if mono_samples.is_empty() {
        return Some(vec![MIN_BAR; NUM_BARS]);
    }

    let total = mono_samples.len();
    let bucket_size = total / NUM_BARS;
    if bucket_size == 0 {
        return Some(vec![MIN_BAR; NUM_BARS]);
    }

    let inv_db_range = 1.0 / (-DB_FLOOR);
    let mut waveform: Vec<u8> = Vec::with_capacity(NUM_BARS);
    for i in 0..NUM_BARS {
        let start = i * bucket_size;
        let end = if i == NUM_BARS - 1 { total } else { start + bucket_size };

        let mut sum_sq: f64 = 0.0;
        let inv_count = 1.0 / (end - start) as f64;
        for j in start..end {
            let s = mono_samples[j] as f64;
            sum_sq += s * s;
        }
        let rms = (sum_sq * inv_count).sqrt() as f32;
        let db = if rms <= 0.0 { DB_FLOOR } else { (20.0 * rms.log10()).max(DB_FLOOR).min(0.0) };
        let normalized = (db - DB_FLOOR) * inv_db_range;
        waveform.push(((normalized * 255.0).round() as u8).max(MIN_BAR));
    }

    Some(waveform)
}

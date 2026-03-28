use std::path::Path;

/// Current analysis algorithm version — bump to force re-analysis on existing tracks.
pub const ANALYSIS_VERSION: i32 = 3;

/// Full analysis result for a track: BPM, beat grid, key, loudness.
#[derive(Debug, Clone)]
pub struct TrackAnalysis {
    pub bpm: f64,
    pub beat_positions: Vec<f64>,
    pub downbeats: Vec<f64>,
    pub key: Option<String>,
    /// EBU R128 integrated loudness in LUFS
    pub lufs: Option<f64>,
}

/// Convert f32 beat times to f64 (shared by analyze_track and analyze_and_extract_waveform).
fn to_f64_vec(v: &[f32]) -> Vec<f64> {
    v.iter().map(|&t| t as f64).collect()
}

/// Extract musical key from analysis result, returning None for empty.
fn extract_key(result: &stratum_dsp::AnalysisResult) -> Option<String> {
    let name = result.key.numerical();
    if name.is_empty() { None } else { Some(name) }
}

/// Build TrackAnalysis from a stratum-dsp result (lufs added separately).
fn build_analysis(result: &stratum_dsp::AnalysisResult, lufs: Option<f64>) -> TrackAnalysis {
    TrackAnalysis {
        bpm: result.bpm as f64,
        beat_positions: to_f64_vec(&result.beat_grid.beats),
        downbeats: to_f64_vec(&result.beat_grid.downbeats),
        key: extract_key(result),
        lufs,
    }
}

/// Measure EBU R128 integrated loudness (LUFS) from mono f32 samples.
fn measure_lufs(samples: &[f32], sample_rate: u32) -> Option<f64> {
    use ebur128::{EbuR128, Mode};

    let mut meter = EbuR128::new(1, sample_rate, Mode::I).ok()?;

    // Process in chunks to avoid excessive single-call overhead
    const CHUNK: usize = 48000; // ~1 second at 48 kHz
    for chunk in samples.chunks(CHUNK) {
        meter.add_frames_f32(chunk).ok()?;
    }

    let loudness = meter.loudness_global().ok()?;
    // ebur128 returns -f64::INFINITY for silence — treat as None
    if loudness.is_finite() {
        Some(loudness)
    } else {
        None
    }
}

/// Combined decode: waveform + analysis in a single file decode pass.
/// Used by scanner to avoid decoding the same file twice.
pub fn analyze_and_extract_waveform(path: &Path) -> (Option<Vec<u8>>, Option<TrackAnalysis>) {
    // Decode once with 5-minute cap (sufficient for analysis; reduces peak RAM by 50%)
    let decoded = crate::audio::decode::decode_to_mono(path, Some(300 * 44100));

    let (samples, sample_rate) = match decoded {
        Some((s, sr)) => (s, sr),
        None => return (None, None),
    };

    // Waveform from the decoded samples
    let waveform = crate::waveform::waveform_from_samples(&samples);

    // EBU R128 loudness measurement (runs on the full decoded buffer)
    let lufs = measure_lufs(&samples, sample_rate);

    // Analysis from the same samples (reuses build_analysis helper)
    let analysis = if samples.len() >= (sample_rate as usize) * 2 {
        let config = stratum_dsp::AnalysisConfig::default();
        stratum_dsp::analyze_audio(&samples, sample_rate, config)
            .ok()
            .map(|result| build_analysis(&result, lufs))
    } else {
        // Even if BPM/key analysis can't run (track too short), still report LUFS
        if lufs.is_some() {
            Some(TrackAnalysis {
                bpm: 0.0,
                beat_positions: vec![],
                downbeats: vec![],
                key: None,
                lufs,
            })
        } else {
            None
        }
    };

    (waveform, analysis)
}

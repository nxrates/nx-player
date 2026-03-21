use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

const NUM_BARS: usize = 128;

/// Minimum bar height (out of 255) so no bar is invisible.
const MIN_BAR: u8 = 10;

/// Floor in dB below which we clamp. -60 dB is ~0.1% of full scale.
const DB_FLOOR: f32 = -60.0;

pub fn extract_waveform(path: &Path) -> Option<Vec<u8>> {
    // --- Step 1-4: Open, probe, get track, create decoder ---
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
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    // --- Step 5-6: Decode ALL packets, collect mono samples ---
    let mut mono_samples: Vec<f32> = Vec::new();

    loop {
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
        return Some(vec![MIN_BAR; NUM_BARS]);
    }

    // --- Step 7: Divide into NUM_BARS equal-length buckets ---
    let total = mono_samples.len();
    let bucket_size = total / NUM_BARS;
    if bucket_size == 0 {
        return Some(vec![MIN_BAR; NUM_BARS]);
    }

    // --- Step 8: Compute RMS per bucket ---
    let mut rms_values: Vec<f32> = Vec::with_capacity(NUM_BARS);
    for i in 0..NUM_BARS {
        let start = i * bucket_size;
        let end = if i == NUM_BARS - 1 {
            total
        } else {
            start + bucket_size
        };

        let mut sum_sq: f64 = 0.0;
        let count = (end - start) as f64;
        for j in start..end {
            let s = mono_samples[j] as f64;
            sum_sq += s * s;
        }
        let rms = (sum_sq / count).sqrt() as f32;
        rms_values.push(rms);
    }

    // --- Step 9: Convert to dB, then map to 0-255 ---
    // dB = 20 * log10(rms), clamped to [DB_FLOOR, 0]
    let db_values: Vec<f32> = rms_values
        .iter()
        .map(|&rms| {
            if rms <= 0.0 {
                DB_FLOOR
            } else {
                (20.0 * rms.log10()).max(DB_FLOOR).min(0.0)
            }
        })
        .collect();

    // Map dB range [DB_FLOOR, 0] -> [0, 255]
    let db_range = -DB_FLOOR; // positive value, e.g. 60.0
    let waveform: Vec<u8> = db_values
        .iter()
        .map(|&db| {
            // db is in [DB_FLOOR, 0], shift to [0, db_range]
            let normalized = (db - DB_FLOOR) / db_range; // 0.0 to 1.0
            let value = (normalized * 255.0).round() as u8;
            // --- Step 10: Clamp minimum ---
            value.max(MIN_BAR)
        })
        .collect();

    Some(waveform)
}

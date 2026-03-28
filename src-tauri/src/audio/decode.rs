//! Shared audio decode pipeline. Used by waveform extraction, analysis, and playback.
//! Consolidates the previously-triplicated probe/decode logic into a single module.

use std::fs::File;
use std::path::Path;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{Decoder, DecoderOptions};
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Result of probing an audio file: ready-to-use format reader + decoder.
pub struct AudioProbe {
    pub format: Box<dyn FormatReader>,
    pub track_id: u32,
    pub sample_rate: u32,
    pub decoder: Box<dyn Decoder>,
}

/// Probe an audio file and create a decoder. Single source of truth for all decode paths.
pub fn probe_audio(path: &Path) -> Option<AudioProbe> {
    let file = File::open(path).ok()?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .ok()?;

    let format = probed.format;
    let track = format.default_track()?;
    let track_id = track.id;
    let sample_rate = track.codec_params.sample_rate?;

    let decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .ok()?;

    Some(AudioProbe { format, track_id, sample_rate, decoder })
}

/// Decode an audio file to mono f32 samples. Single shared decode loop.
///
/// - `max_samples`: Optional limit (e.g. 600 * sample_rate for 10 min cap)
/// - Returns: (mono_samples, sample_rate)
pub fn decode_to_mono(path: &Path, max_samples: Option<usize>) -> Option<(Vec<f32>, u32)> {
    let AudioProbe { mut format, track_id, sample_rate, mut decoder } = probe_audio(path)?;

    let limit = max_samples.unwrap_or(usize::MAX);
    // Pre-allocate based on limit to avoid incremental Vec growth (reduces allocator fragmentation).
    // A single large mmap'd allocation gets properly munmap'd on drop.
    let mut mono_samples: Vec<f32> = Vec::with_capacity(limit.min(16 * 1024 * 1024));

    loop {
        if mono_samples.len() >= limit { break; }

        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(symphonia::core::errors::Error::IoError(ref e))
                if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(_) => break,
        };

        if packet.track_id() != track_id { continue; }

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

        // Optimized mono downmix: hoist stereo branch outside loop
        if ch >= 2 {
            for f in 0..frames {
                if mono_samples.len() >= limit { break; }
                mono_samples.push((samples[f * ch] + samples[f * ch + 1]) * 0.5);
            }
        } else {
            for f in 0..frames {
                if mono_samples.len() >= limit { break; }
                mono_samples.push(samples[f]);
            }
        }
    }

    if mono_samples.is_empty() { return None; }
    Some((mono_samples, sample_rate))
}

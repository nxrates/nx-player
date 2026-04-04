use rubato::Resampler;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::formats::SeekMode;
use symphonia::core::probe::Hint;
use symphonia::core::units::Time;

/// Ring buffer capacity in samples (stereo interleaved).
const RING_BUFFER_CAPACITY: usize = 192_000; // ~1 second at 96kHz stereo

pub struct Deck {
    /// Shared flag to signal the decode thread to stop.
    stop_flag: Arc<AtomicBool>,
    /// Current playback position in samples (at output sample rate).
    pub position: Arc<AtomicU64>,
    /// Duration of the loaded track in samples (at output sample rate).
    pub duration_samples: u64,
    /// Path to the loaded file (retained for seek-by-reload).
    pub path: PathBuf,
    /// Decode thread handle.
    _thread: Option<std::thread::JoinHandle<()>>,
}

impl Deck {
    /// Load a file and begin decoding in the background.
    /// Returns a Consumer<f32> of interleaved stereo samples resampled to `output_sr`.
    /// If `seek_to` is Some, seek to that fractional position [0..1] before decoding.
    pub fn load(path: PathBuf, output_sr: u32) -> Result<(Self, rtrb::Consumer<f32>), String> {
        Self::load_at(path, output_sr, None)
    }

    /// Load a file, optionally seeking to a position (0.0 = start, 1.0 = end).
    pub fn load_at(path: PathBuf, output_sr: u32, seek_to: Option<f64>) -> Result<(Self, rtrb::Consumer<f32>), String> {
        let (mut producer, consumer) = rtrb::RingBuffer::new(RING_BUFFER_CAPACITY);

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop = stop_flag.clone();
        let position = Arc::new(AtomicU64::new(0));
        let pos = position.clone();

        // Probe file once — reuse the format reader in the decode thread
        let file = std::fs::File::open(&path)
            .map_err(|e| format!("Failed to open file: {}", e))?;
        let mss = MediaSourceStream::new(Box::new(file), Default::default());
        let mut hint = Hint::new();
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            hint.with_extension(ext);
        }
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
            .map_err(|e| format!("Failed to probe file: {}", e))?;

        let track = probed.format.default_track()
            .ok_or_else(|| "No default track found".to_string())?;
        let track_sr = track.codec_params.sample_rate
            .ok_or_else(|| "Unknown sample rate".to_string())?;
        let track_id = track.id;
        let codec_params = track.codec_params.clone();

        let n_frames = codec_params.n_frames.unwrap_or(0);
        let duration_samples = if track_sr == output_sr {
            n_frames
        } else {
            (n_frames as f64 * output_sr as f64 / track_sr as f64) as u64
        };

        // Pass the already-probed format reader to the decode thread (no re-probe)
        let format = probed.format;
        let seek_pos = seek_to;
        let dur_secs = duration_samples as f64 / output_sr as f64;
        let thread = std::thread::spawn(move || {
            if let Err(e) = decode_loop(format, track_id, &codec_params, track_sr, output_sr, seek_pos, dur_secs, &mut producer, &stop, &pos) {
                eprintln!("Deck decode error: {}", e);
            }
        });

        Ok((
            Deck {
                stop_flag,
                position,
                duration_samples,
                path,
                _thread: Some(thread),
            },
            consumer,
        ))
    }

    pub fn stop(&self) {
        self.stop_flag.store(true, Ordering::Relaxed);
    }

    pub fn position_seconds(&self, output_sr: u32) -> f64 {
        self.position.load(Ordering::Relaxed) as f64 / output_sr as f64
    }

    pub fn duration_seconds(&self, output_sr: u32) -> f64 {
        self.duration_samples as f64 / output_sr as f64
    }
}

impl Drop for Deck {
    fn drop(&mut self) {
        self.stop_flag.store(true, Ordering::Relaxed);
        if let Some(thread) = self._thread.take() {
            // Give the decode thread a moment to notice the stop flag and exit cleanly.
            let _ = thread.join();
        }
    }
}

fn decode_loop(
    mut format: Box<dyn FormatReader>,
    track_id: u32,
    codec_params: &symphonia::core::codecs::CodecParameters,
    track_sr: u32,
    output_sr: u32,
    seek_to: Option<f64>,
    duration_secs: f64,
    producer: &mut rtrb::Producer<f32>,
    stop: &AtomicBool,
    position: &AtomicU64,
) -> Result<(), String> {
    let mut decoder = symphonia::default::get_codecs()
        .make(codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Failed to create decoder: {}", e))?;

    // Seek to target position if requested
    if let Some(frac) = seek_to {
        let target_secs = (frac.clamp(0.0, 1.0) * duration_secs).max(0.0);
        let seek_time = Time::from(target_secs);
        if let Err(e) = format.seek(SeekMode::Coarse, symphonia::core::formats::SeekTo::Time { time: seek_time, track_id: Some(track_id) }) {
            eprintln!("Seek failed (will play from start): {}", e);
        } else {
            // Update position to reflect the seek
            let seeked_samples = (target_secs * output_sr as f64) as u64;
            position.store(seeked_samples, Ordering::Relaxed);
        }
        decoder.reset();
    }

    // Set up resampler if needed
    let needs_resample = track_sr != output_sr;
    let chunk_size = 1024;
    let mut resampler = if needs_resample {
        Some(
            rubato::FftFixedIn::<f32>::new(
                track_sr as usize,
                output_sr as usize,
                chunk_size,
                2, // sub_chunks
                2, // always resample to stereo
            )
            .map_err(|e| format!("Failed to create resampler: {}", e))?,
        )
    } else {
        None
    };

    // Buffers for resampling (per-channel, non-interleaved) — pre-reserve capacity
    let mut resample_in: Vec<Vec<f32>> = vec![Vec::with_capacity(chunk_size * 2); 2];
    let mut out_pos: u64 = 0;
    let backoff_sleep = std::time::Duration::from_millis(5); // 5ms backoff reduces idle wakeups 5x vs 1ms

    loop {
        if stop.load(Ordering::Relaxed) {
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

        if needs_resample {
            // Deinterleave into per-channel buffers (branch hoisted out of loop)
            if ch >= 2 {
                for f in 0..frames {
                    resample_in[0].push(samples[f * ch]);
                    resample_in[1].push(samples[f * ch + 1]);
                }
            } else {
                for f in 0..frames {
                    let s = samples[f];
                    resample_in[0].push(s);
                    resample_in[1].push(s);
                }
            }

            // Process full chunks
            while resample_in[0].len() >= chunk_size {
                let chunk: Vec<Vec<f32>> = resample_in
                    .iter_mut()
                    .map(|buf| buf.drain(..chunk_size).collect())
                    .collect();

                if let Some(ref mut rs) = resampler {
                    match rs.process(&chunk, None) {
                        Ok(output) => {
                            let out_frames = output[0].len();
                            for f in 0..out_frames {
                                // Wait for ring buffer space with adaptive backoff
                                loop {
                                    if stop.load(Ordering::Relaxed) {
                                        return Ok(());
                                    }
                                    if producer.slots() >= 2 {
                                        break;
                                    }
                                    std::thread::sleep(backoff_sleep);
                                }
                                let _ = producer.push(output[0][f]);
                                let _ = producer.push(output[1][f]);
                                out_pos += 1;
                            }
                            position.store(out_pos, Ordering::Relaxed);
                        }
                        Err(e) => {
                            eprintln!("Resample error: {}", e);
                        }
                    }
                }
            }
        } else {
            // No resampling needed — write interleaved stereo directly
            let is_stereo = ch >= 2;
            for f in 0..frames {
                if stop.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let left = samples[f * ch];
                let right = if is_stereo { samples[f * ch + 1] } else { left };

                loop {
                    if stop.load(Ordering::Relaxed) {
                        return Ok(());
                    }
                    if producer.slots() >= 2 {
                        break;
                    }
                    std::thread::sleep(backoff_sleep);
                }
                let _ = producer.push(left);
                let _ = producer.push(right);
                out_pos += 1;
            }
            position.store(out_pos, Ordering::Relaxed);
        }
    }

    Ok(())
}

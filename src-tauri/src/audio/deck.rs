use rubato::Resampler;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;

use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::DecoderOptions;
use symphonia::core::formats::FormatOptions;
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

/// Ring buffer capacity in samples (stereo interleaved).
const RING_BUFFER_CAPACITY: usize = 192_000; // ~1 second at 96kHz stereo

pub struct Deck {
    /// Shared flag to signal the decode thread to stop.
    stop_flag: Arc<AtomicBool>,
    /// Current playback position in samples (at output sample rate).
    pub position: Arc<AtomicU64>,
    /// Duration of the loaded track in samples (at output sample rate).
    pub duration_samples: u64,
    /// Decode thread handle.
    _thread: Option<std::thread::JoinHandle<()>>,
}

impl Deck {
    /// Load a file and begin decoding in the background.
    /// Returns a Consumer<f32> of interleaved stereo samples resampled to `output_sr`.
    pub fn load(path: PathBuf, output_sr: u32) -> Result<(Self, rtrb::Consumer<f32>), String> {
        let (mut producer, consumer) = rtrb::RingBuffer::new(RING_BUFFER_CAPACITY);

        let stop_flag = Arc::new(AtomicBool::new(false));
        let stop = stop_flag.clone();
        let position = Arc::new(AtomicU64::new(0));
        let pos = position.clone();

        // Probe file to get duration and sample rate before spawning thread
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

        let n_frames = track.codec_params.n_frames.unwrap_or(0);
        let duration_samples = if track_sr == output_sr {
            n_frames
        } else {
            (n_frames as f64 * output_sr as f64 / track_sr as f64) as u64
        };

        let thread = std::thread::spawn(move || {
            if let Err(e) = decode_loop(&path, output_sr, &mut producer, &stop, &pos) {
                eprintln!("Deck decode error: {}", e);
            }
        });

        Ok((
            Deck {
                stop_flag,
                position,
                duration_samples,
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
    }
}

fn decode_loop(
    path: &PathBuf,
    output_sr: u32,
    producer: &mut rtrb::Producer<f32>,
    stop: &AtomicBool,
    position: &AtomicU64,
) -> Result<(), String> {
    let file = std::fs::File::open(path)
        .map_err(|e| format!("Failed to open file: {}", e))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }

    let probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .map_err(|e| format!("Failed to probe: {}", e))?;

    let mut format = probed.format;
    let track = format.default_track()
        .ok_or_else(|| "No default track".to_string())?;
    let track_id = track.id;
    let track_sr = track.codec_params.sample_rate
        .ok_or_else(|| "Unknown sample rate".to_string())?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .map_err(|e| format!("Failed to create decoder: {}", e))?;

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

use std::path::PathBuf;
use std::sync::mpsc;

use super::deck::Deck;
use super::eq::EqCrossfader;
use super::mixer::{mix_stereo, MixerState};
use super::output::AudioOutput;
use super::stretch::TimeStretcher;
use super::sync::{BeatGrid, BeatSync};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Commands sent from the UI thread to the engine thread.
pub enum EngineCommand {
    LoadDeckA(PathBuf),
    LoadDeckB(PathBuf),
    Play,
    Pause,
    Stop,
    Seek(f64),
    SetVolume(f32),
    SetPlaybackRate(f64),
    StartCrossfade {
        duration_secs: f64,
        deck_a_beats: Option<BeatGrid>,
        deck_b_beats: Option<BeatGrid>,
    },
    CancelCrossfade,
    SetVisualization(bool),
    Shutdown,
}

/// Playback state sent from engine thread to UI at ~30Hz.
#[derive(Debug, Clone, serde::Serialize)]
pub struct PlaybackState {
    pub playing: bool,
    pub position: f64,
    pub duration: f64,
    pub volume: f32,
    pub bpm: Option<f64>,
    pub spectrum: Vec<f32>,
    pub crossfade_active: bool,
    pub crossfade_position: f32,
}

/// Shared flag so frontend can enable/disable visualization processing.
static VISUALIZATION_ACTIVE: AtomicBool = AtomicBool::new(false);

pub fn set_visualization_active(active: bool) {
    VISUALIZATION_ACTIVE.store(active, Ordering::Relaxed);
}

pub struct AudioEngine {
    cmd_tx: mpsc::Sender<EngineCommand>,
    _thread: std::thread::JoinHandle<()>,
}

impl AudioEngine {
    pub fn new() -> Result<(Self, mpsc::Receiver<PlaybackState>), String> {
        let (cmd_tx, cmd_rx) = mpsc::channel::<EngineCommand>();
        let (state_tx, state_rx) = mpsc::channel::<PlaybackState>();

        let thread = std::thread::Builder::new()
            .name("audio-engine".into())
            .spawn(move || {
                engine_loop(cmd_rx, state_tx);
            })
            .map_err(|e| format!("Failed to spawn engine thread: {}", e))?;

        Ok((
            Self {
                cmd_tx,
                _thread: thread,
            },
            state_rx,
        ))
    }

    pub fn send(&self, cmd: EngineCommand) -> Result<(), String> {
        self.cmd_tx
            .send(cmd)
            .map_err(|e| format!("Engine channel closed: {}", e))
    }
}

impl Drop for AudioEngine {
    fn drop(&mut self) {
        let _ = self.cmd_tx.send(EngineCommand::Shutdown);
    }
}

struct EngineState {
    mixer: Arc<MixerState>,
    deck_a: Option<Deck>,
    deck_b: Option<Deck>,
    consumer_a: Option<rtrb::Consumer<f32>>,
    consumer_b: Option<rtrb::Consumer<f32>>,
    output: Option<AudioOutput>,
    output_producer: Option<rtrb::Producer<f32>>,
    output_sr: u32,
    playing: bool,
    playback_rate: f64,
    // Crossfade state
    crossfade_active: bool,
    crossfade_position: f32,
    crossfade_step: f32, // per-frame increment
    eq_crossfader: Option<EqCrossfader>,
    stretcher: Option<TimeStretcher>,
    beat_sync: Option<BeatSync>,
    // Visualization — only computed when visualization_active is true
    visualization_active: bool,
    fft_buffer: Vec<f32>,
    spectrum: [f32; 128],
}

fn engine_loop(cmd_rx: mpsc::Receiver<EngineCommand>, state_tx: mpsc::Sender<PlaybackState>) {
    // Create output ring buffer
    let (output_producer, output_consumer) = rtrb::RingBuffer::new(192_000);

    let (output, output_sr) = match AudioOutput::new(output_consumer) {
        Ok((o, sr)) => (Some(o), sr),
        Err(e) => {
            eprintln!("Failed to create audio output: {}", e);
            (None, 44100)
        }
    };

    let mut state = EngineState {
        mixer: Arc::new(MixerState::new()),
        deck_a: None,
        deck_b: None,
        consumer_a: None,
        consumer_b: None,
        output,
        output_producer: Some(output_producer),
        output_sr,
        playing: false,
        playback_rate: 1.0,
        crossfade_active: false,
        crossfade_position: 0.0,
        crossfade_step: 0.0,
        eq_crossfader: None,
        stretcher: None,
        beat_sync: None,
        visualization_active: false,
        fft_buffer: Vec::with_capacity(256),
        spectrum: [0.0; 128],
    };

    let mut last_state_time = std::time::Instant::now();
    let state_interval = std::time::Duration::from_millis(33); // ~30Hz

    // Pre-computed constants for adaptive sleep (avoid re-creating Duration objects)
    let sleep_idle = std::time::Duration::from_millis(50);
    let sleep_healthy = std::time::Duration::from_millis(5);
    let sleep_hungry = std::time::Duration::from_millis(1);
    let sleep_no_output = std::time::Duration::from_millis(10);
    let buffer_threshold = 192_000 / 2;

    loop {
        // When idle: block on command channel instead of spinning (saves a full CPU core).
        // When playing: non-blocking poll so audio processing isn't delayed.
        let first_cmd = if !state.playing {
            cmd_rx.recv_timeout(sleep_idle).ok()
        } else {
            None
        };
        for cmd in first_cmd.into_iter().chain(std::iter::from_fn(|| cmd_rx.try_recv().ok())) {
            match cmd {
                EngineCommand::LoadDeckA(path) => {
                    if let Some(ref deck) = state.deck_a {
                        deck.stop();
                    }
                    match Deck::load(path, state.output_sr) {
                        Ok((deck, consumer)) => {
                            state.deck_a = Some(deck);
                            state.consumer_a = Some(consumer);
                        }
                        Err(e) => eprintln!("Failed to load deck A: {}", e),
                    }
                }
                EngineCommand::LoadDeckB(path) => {
                    if let Some(ref deck) = state.deck_b {
                        deck.stop();
                    }
                    match Deck::load(path, state.output_sr) {
                        Ok((deck, consumer)) => {
                            state.deck_b = Some(deck);
                            state.consumer_b = Some(consumer);
                        }
                        Err(e) => eprintln!("Failed to load deck B: {}", e),
                    }
                }
                EngineCommand::Play => {
                    state.playing = true;
                    if let Some(ref output) = state.output {
                        output.resume();
                    }
                }
                EngineCommand::Pause => {
                    state.playing = false;
                    if let Some(ref output) = state.output {
                        output.pause();
                    }
                }
                EngineCommand::Stop => {
                    state.playing = false;
                    if let Some(ref output) = state.output {
                        output.pause();
                    }
                    if let Some(ref deck) = state.deck_a {
                        deck.stop();
                    }
                    if let Some(ref deck) = state.deck_b {
                        deck.stop();
                    }
                    state.deck_a = None;
                    state.deck_b = None;
                    state.consumer_a = None;
                    state.consumer_b = None;
                    state.crossfade_active = false;
                }
                EngineCommand::Seek(_pos) => {
                    // Seek is complex with streaming decode; for now, reload would be needed.
                    // TODO: implement seek by restarting decode at offset
                }
                EngineCommand::SetVolume(vol) => {
                    state
                        .mixer
                        .master_volume
                        .store(vol.clamp(0.0, 1.0), Ordering::Relaxed);
                }
                EngineCommand::SetPlaybackRate(rate) => {
                    state.playback_rate = rate.clamp(0.5, 2.0);
                }
                EngineCommand::StartCrossfade {
                    duration_secs,
                    deck_a_beats,
                    deck_b_beats,
                } => {
                    state.crossfade_active = true;
                    state.crossfade_position = 0.0;
                    let total_frames = (duration_secs * state.output_sr as f64) as f32;
                    state.crossfade_step = if total_frames > 0.0 {
                        1.0 / total_frames
                    } else {
                        1.0
                    };
                    state.eq_crossfader = Some(EqCrossfader::new(state.output_sr as f32));

                    // Beat sync: store grids and create tempo stretcher if BPMs differ
                    let mut sync = BeatSync::new();
                    sync.deck_a = deck_a_beats;
                    sync.deck_b = deck_b_beats;
                    let ratio = sync.tempo_ratio();
                    state.beat_sync = Some(sync);

                    // Create time stretcher for deck B if BPM ratio warrants it
                    if (ratio - 1.0).abs() > 0.01 && ratio > 0.8 && ratio < 1.25 {
                        state.stretcher = Some(TimeStretcher::new(2, state.output_sr));
                    } else {
                        state.stretcher = None;
                    }
                }
                EngineCommand::CancelCrossfade => {
                    state.crossfade_active = false;
                    state.crossfade_position = 0.0;
                    state.eq_crossfader = None;
                    state.stretcher = None;
                    state.beat_sync = None;
                }
                EngineCommand::SetVisualization(active) => {
                    state.visualization_active = active;
                    if !active {
                        state.fft_buffer.clear();
                        state.spectrum = [0.0; 128];
                    }
                }
                EngineCommand::Shutdown => {
                    if let Some(ref deck) = state.deck_a {
                        deck.stop();
                    }
                    if let Some(ref deck) = state.deck_b {
                        deck.stop();
                    }
                    return;
                }
            }
        }

        // Process audio: read from deck consumers, mix, write to output
        if state.playing {
            let frames_to_process = 512;
            if let Some(ref mut producer) = state.output_producer {
                let available = producer.slots() / 2; // stereo frames
                let frames = available.min(frames_to_process);

                // Batch-load all flags/gains once per frame batch (not per sample)
                let cached_master = state.mixer.master_volume.load(Ordering::Relaxed);
                let cached_a_gain = state.mixer.deck_a_gain.load(Ordering::Relaxed);
                let cached_b_gain = state.mixer.deck_b_gain.load(Ordering::Relaxed);

                // Sync visualization flag once per batch (single atomic load)
                let viz_flag = VISUALIZATION_ACTIVE.load(Ordering::Relaxed);
                if viz_flag != state.visualization_active {
                    state.visualization_active = viz_flag;
                    if !viz_flag { state.fft_buffer.clear(); }
                }
                let viz_active = state.visualization_active;

                for _ in 0..frames {
                    let (a_l, a_r) = read_stereo_frame(&mut state.consumer_a);
                    let (b_l, b_r) = read_stereo_frame(&mut state.consumer_b);

                    let (left, right) = if state.crossfade_active {
                        if let Some(ref mut eq_cf) = state.eq_crossfader {
                            let (l, r) =
                                eq_cf.process(a_l, a_r, b_l, b_r, state.crossfade_position);
                            (l * cached_master, r * cached_master)
                        } else {
                            mix_stereo(
                                a_l, a_r, b_l, b_r,
                                state.crossfade_position,
                                cached_a_gain, cached_b_gain, cached_master,
                            )
                        }
                    } else {
                        mix_stereo(a_l, a_r, b_l, b_r, 0.0, cached_a_gain, cached_b_gain, cached_master)
                    };

                    let _ = producer.push(left);
                    let _ = producer.push(right);

                    // Feed FFT buffer for visualization — only when active (cached per batch)
                    if viz_active {
                        let mono = (left + right) * 0.5;
                        state.fft_buffer.push(mono);
                        if state.fft_buffer.len() >= 256 {
                            compute_spectrum_256_into(&state.fft_buffer[..256], &mut state.spectrum);
                            state.fft_buffer.clear();
                        }
                    }

                    // Advance crossfade
                    if state.crossfade_active {
                        state.crossfade_position += state.crossfade_step;
                        if state.crossfade_position >= 1.0 {
                            state.crossfade_position = 1.0;
                            state.crossfade_active = false;
                            // Crossfade complete: deck B becomes the new primary
                            // Swap A <- B, clear B
                            state.deck_a = state.deck_b.take();
                            state.consumer_a = state.consumer_b.take();
                            state.eq_crossfader = None;
                            state.stretcher = None;
                            state.beat_sync = None;
                        }
                    }
                }
            }
        }

        // Send playback state at ~30Hz (skip when idle — nothing changes)
        let now = std::time::Instant::now();
        if (state.playing || state.crossfade_active)
            && now.duration_since(last_state_time) >= state_interval
        {
            last_state_time = now;

            let (position, duration) = match &state.deck_a {
                Some(deck) => (
                    deck.position_seconds(state.output_sr),
                    deck.duration_seconds(state.output_sr),
                ),
                None => (0.0, 0.0),
            };

            // Only include spectrum data when visualization is active (no alloc when off)
            let spectrum = if state.visualization_active {
                state.spectrum.to_vec()
            } else {
                Default::default()
            };

            let _ = state_tx.send(PlaybackState {
                playing: state.playing,
                position,
                duration,
                volume: state.mixer.master_volume.load(Ordering::Relaxed),
                bpm: None,
                spectrum,
                crossfade_active: state.crossfade_active,
                crossfade_position: state.crossfade_position,
            });
        }

        // Adaptive sleep using pre-computed constants.
        // When !playing, recv_timeout(sleep_idle) above already blocks — no extra sleep needed.
        if state.playing {
            if let Some(ref producer) = state.output_producer {
                let slots_free = producer.slots();
                std::thread::sleep(if slots_free < buffer_threshold { sleep_healthy } else { sleep_hungry });
            } else {
                std::thread::sleep(sleep_no_output);
            }
        }
    }
}

/// Read one stereo frame from a consumer, returning silence if empty/absent.
#[inline(always)]
fn read_stereo_frame(consumer: &mut Option<rtrb::Consumer<f32>>) -> (f32, f32) {
    match consumer {
        Some(ref mut c) => {
            let left = c.pop().unwrap_or(0.0);
            let right = c.pop().unwrap_or(0.0);
            (left, right)
        }
        None => (0.0, 0.0),
    }
}

// --- Hand-rolled 256-point radix-2 FFT for visualization ---
// Uses stack-allocated arrays. No external FFT dependency.

// Pre-computed twiddle factors for 256-point FFT (computed once at startup).
use std::sync::LazyLock;
static TWIDDLE_RE: LazyLock<[f32; 128]> = LazyLock::new(|| {
    let mut tw = [0.0_f32; 128];
    for i in 0..128 {
        tw[i] = (-2.0 * std::f32::consts::PI * i as f32 / 256.0).cos();
    }
    tw
});
static TWIDDLE_IM: LazyLock<[f32; 128]> = LazyLock::new(|| {
    let mut tw = [0.0_f32; 128];
    for i in 0..128 {
        tw[i] = (-2.0 * std::f32::consts::PI * i as f32 / 256.0).sin();
    }
    tw
});
static HANN_WINDOW: LazyLock<[f32; 256]> = LazyLock::new(|| {
    let mut w = [0.0_f32; 256];
    for i in 0..256 {
        w[i] = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / 255.0).cos());
    }
    w
});

/// Compute a 128-bin magnitude spectrum from 256 real samples into a pre-allocated array.
fn compute_spectrum_256_into(samples: &[f32], out: &mut [f32; 128]) {
    assert!(samples.len() >= 256);

    let hann = &*HANN_WINDOW;
    let tw_re = &*TWIDDLE_RE;
    let tw_im = &*TWIDDLE_IM;

    // Apply Hann window and prepare complex input (imag = 0)
    let mut real = [0.0_f32; 256];
    let mut imag = [0.0_f32; 256];

    for i in 0..256 {
        real[i] = samples[i] * hann[i];
    }

    // Bit-reversal permutation
    bit_reverse_permute(&mut real, &mut imag);

    // Cooley-Tukey radix-2 DIT FFT with pre-computed twiddle factors
    let n = 256;
    let mut size = 2;
    while size <= n {
        let half = size / 2;
        let twiddle_stride = n / size;
        for k in (0..n).step_by(size) {
            for j in 0..half {
                let tw_idx = j * twiddle_stride;
                let wr = tw_re[tw_idx];
                let wi = tw_im[tw_idx];

                let idx1 = k + j;
                let idx2 = k + j + half;

                let tr = wr * real[idx2] - wi * imag[idx2];
                let ti = wr * imag[idx2] + wi * real[idx2];

                real[idx2] = real[idx1] - tr;
                imag[idx2] = imag[idx1] - ti;
                real[idx1] = real[idx1] + tr;
                imag[idx1] = imag[idx1] + ti;
            }
        }
        size *= 2;
    }

    // Compute power spectrum for first 128 bins (positive frequencies)
    // Power (no sqrt) is perceptually equivalent for visualization and saves ~3800 cycles
    const INV_128_SQ: f32 = 1.0 / (128.0 * 128.0);
    for i in 0..128 {
        out[i] = (real[i] * real[i] + imag[i] * imag[i]) * INV_128_SQ;
    }
}

// Pre-computed bit-reversal table for 256-point FFT (avoids 256 reverse_bits calls per FFT)
static BIT_REVERSE_TABLE: LazyLock<[u8; 256]> = LazyLock::new(|| {
    let mut table = [0u8; 256];
    for i in 0..256 {
        table[i] = ((i as u32).reverse_bits() >> 24) as u8;
    }
    table
});

/// Bit-reverse permutation for 256 elements using pre-computed LUT.
fn bit_reverse_permute(real: &mut [f32; 256], imag: &mut [f32; 256]) {
    let table = &*BIT_REVERSE_TABLE;
    for i in 0..256 {
        let j = table[i] as usize;
        if i < j {
            real.swap(i, j);
            imag.swap(i, j);
        }
    }
}

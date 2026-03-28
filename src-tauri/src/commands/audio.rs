use std::sync::Mutex;

use super::LockExt;
use crate::audio::engine::{AudioEngine, EngineCommand, PlaybackState, set_visualization_active};
use crate::audio::sync::BeatGrid;

pub struct AudioEngineState(pub Mutex<AudioEngine>);
pub struct PlaybackReceiver(pub Mutex<std::sync::mpsc::Receiver<PlaybackState>>);

/// Helper: lock engine mutex and send a command.
fn send_cmd(engine: &tauri::State<'_, AudioEngineState>, cmd: EngineCommand) -> Result<(), String> {
    engine.0.acquire()?.send(cmd)
}

#[tauri::command]
pub fn audio_play(engine: tauri::State<'_, AudioEngineState>) -> Result<(), String> {
    send_cmd(&engine, EngineCommand::Play)
}

#[tauri::command]
pub fn audio_pause(engine: tauri::State<'_, AudioEngineState>) -> Result<(), String> {
    send_cmd(&engine, EngineCommand::Pause)
}

#[tauri::command]
pub fn audio_stop(engine: tauri::State<'_, AudioEngineState>) -> Result<(), String> {
    send_cmd(&engine, EngineCommand::Stop)
}

#[tauri::command]
pub fn audio_load(
    engine: tauri::State<'_, AudioEngineState>,
    path: String,
    deck: Option<String>,
) -> Result<(), String> {
    let path = std::path::PathBuf::from(path);
    let cmd = match deck.as_deref() {
        Some("b") | Some("B") => EngineCommand::LoadDeckB(path),
        _ => EngineCommand::LoadDeckA(path),
    };
    send_cmd(&engine, cmd)
}

#[tauri::command]
pub fn audio_seek(engine: tauri::State<'_, AudioEngineState>, position: f64) -> Result<(), String> {
    send_cmd(&engine, EngineCommand::Seek(position))
}

#[tauri::command]
pub fn audio_set_volume(engine: tauri::State<'_, AudioEngineState>, volume: f32) -> Result<(), String> {
    send_cmd(&engine, EngineCommand::SetVolume(volume))
}

#[tauri::command]
pub fn audio_set_playback_rate(engine: tauri::State<'_, AudioEngineState>, rate: f64) -> Result<(), String> {
    send_cmd(&engine, EngineCommand::SetPlaybackRate(rate))
}

#[tauri::command]
pub fn audio_start_crossfade(
    engine: tauri::State<'_, AudioEngineState>,
    duration_secs: f64,
    deck_a_bpm: Option<f64>,
    deck_a_beats: Option<Vec<f64>>,
    deck_a_downbeats: Option<Vec<f64>>,
    deck_b_bpm: Option<f64>,
    deck_b_beats: Option<Vec<f64>>,
    deck_b_downbeats: Option<Vec<f64>>,
) -> Result<(), String> {
    let a_grid = deck_a_bpm.map(|bpm| BeatGrid {
        bpm,
        beats: deck_a_beats.unwrap_or_default(),
        downbeats: deck_a_downbeats.unwrap_or_default(),
    });
    let b_grid = deck_b_bpm.map(|bpm| BeatGrid {
        bpm,
        beats: deck_b_beats.unwrap_or_default(),
        downbeats: deck_b_downbeats.unwrap_or_default(),
    });
    send_cmd(&engine, EngineCommand::StartCrossfade {
        duration_secs,
        deck_a_beats: a_grid,
        deck_b_beats: b_grid,
    })
}

#[tauri::command]
pub fn audio_cancel_crossfade(engine: tauri::State<'_, AudioEngineState>) -> Result<(), String> {
    send_cmd(&engine, EngineCommand::CancelCrossfade)
}

#[tauri::command]
pub fn audio_set_visualization(engine: tauri::State<'_, AudioEngineState>, active: bool) -> Result<(), String> {
    set_visualization_active(active);
    send_cmd(&engine, EngineCommand::SetVisualization(active))
}

#[tauri::command]
pub fn audio_get_state(receiver: tauri::State<'_, PlaybackReceiver>) -> Result<Option<PlaybackState>, String> {
    let rx = receiver.0.acquire()?;
    let mut latest: Option<PlaybackState> = None;
    while let Ok(state) = rx.try_recv() {
        latest = Some(state);
    }
    Ok(latest)
}

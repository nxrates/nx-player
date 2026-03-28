/// Beat grid information for sync calculations.
#[derive(Debug, Clone)]
pub struct BeatGrid {
    pub bpm: f64,
    pub beats: Vec<f64>,
    pub downbeats: Vec<f64>,
}

/// Beat synchronization engine for two decks.
pub struct BeatSync {
    pub deck_a: Option<BeatGrid>,
    pub deck_b: Option<BeatGrid>,
}

impl BeatSync {
    pub fn new() -> Self {
        Self {
            deck_a: None,
            deck_b: None,
        }
    }

    /// Compute the tempo ratio needed to match deck B's tempo to deck A.
    pub fn tempo_ratio(&self) -> f64 {
        match (&self.deck_a, &self.deck_b) {
            (Some(a), Some(b)) if a.bpm > 0.0 && b.bpm > 0.0 => a.bpm / b.bpm,
            _ => 1.0,
        }
    }

    /// Find the nearest value in a sorted slice to the given target.
    fn find_nearest(times: &[f64], target: f64) -> Option<f64> {
        if times.is_empty() { return None; }
        let mut best = times[0];
        let mut best_dist = (target - best).abs();
        for &t in &times[1..] {
            let dist = (target - t).abs();
            if dist < best_dist { best = t; best_dist = dist; }
        }
        Some(best)
    }

    /// Find the nearest beat in the grid to the given time position (seconds).
    pub fn nearest_beat(grid: &BeatGrid, time: f64) -> Option<f64> {
        Self::find_nearest(&grid.beats, time)
    }

    /// Find the nearest downbeat in the grid to the given time position (seconds).
    pub fn nearest_downbeat(grid: &BeatGrid, time: f64) -> Option<f64> {
        Self::find_nearest(&grid.downbeats, time)
    }

    /// Find the next downbeat after the given time position.
    /// Useful for snapping crossfade start to bar boundaries.
    pub fn next_downbeat_after(grid: &BeatGrid, time: f64) -> Option<f64> {
        grid.downbeats.iter().find(|&&db| db > time).copied()
    }

    /// Compute the crossfade snap point: the next bar boundary after `time` on deck A.
    pub fn snap_crossfade_to_bar(&self, time: f64) -> f64 {
        if let Some(ref a) = self.deck_a {
            Self::next_downbeat_after(a, time).unwrap_or(time)
        } else {
            time
        }
    }
}

impl Default for BeatSync {
    fn default() -> Self {
        Self::new()
    }
}

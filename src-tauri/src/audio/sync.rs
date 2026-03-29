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

    /// Find the nearest value in a sorted slice using binary search.
    fn find_nearest(times: &[f64], target: f64) -> Option<f64> {
        if times.is_empty() {
            return None;
        }
        let idx = times.partition_point(|&t| t < target);
        let candidates = [
            if idx > 0 { Some(times[idx - 1]) } else { None },
            if idx < times.len() { Some(times[idx]) } else { None },
        ];
        candidates
            .iter()
            .flatten()
            .copied()
            .min_by(|a, b| {
                (a - target)
                    .abs()
                    .partial_cmp(&(b - target).abs())
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
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
    /// Uses binary search for O(log n) performance.
    pub fn next_downbeat_after(grid: &BeatGrid, time: f64) -> Option<f64> {
        let idx = grid.downbeats.partition_point(|&db| db <= time);
        grid.downbeats.get(idx).copied()
    }

    /// Compute the crossfade snap point: the next bar boundary after `time` on deck A.
    pub fn snap_crossfade_to_bar(&self, time: f64) -> f64 {
        if let Some(ref a) = self.deck_a {
            Self::next_downbeat_after(a, time).unwrap_or(time)
        } else {
            time
        }
    }

    /// Distance from `time` to the nearest beat, as a fraction of the beat interval.
    /// Returns 0.0 when exactly on beat, 0.5 when maximally off beat.
    pub fn beat_phase(grid: &BeatGrid, time: f64) -> f64 {
        if grid.bpm <= 0.0 {
            return 0.0;
        }
        let beat_interval = 60.0 / grid.bpm;
        match Self::nearest_beat(grid, time) {
            Some(nearest) => {
                let offset = (time - nearest).abs();
                (offset / beat_interval).min(0.5)
            }
            None => 0.0,
        }
    }
}

impl Default for BeatSync {
    fn default() -> Self {
        Self::new()
    }
}

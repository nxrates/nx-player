/// Real-time time stretcher wrapping ssstretch.
pub struct TimeStretcher {
    stretcher: ssstretch::Stretch,
    channels: usize,
    sample_rate: u32,
    // Reusable buffers to avoid per-call allocations
    input_bufs: Vec<Vec<f32>>,
    output_bufs: Vec<Vec<f32>>,
    result_buf: Vec<f32>,
}

impl TimeStretcher {
    pub fn new(channels: usize, sample_rate: u32) -> Self {
        let mut stretcher = ssstretch::Stretch::new();
        stretcher.preset_default(channels as i32, sample_rate as f32);
        Self {
            stretcher,
            channels,
            sample_rate,
            input_bufs: vec![Vec::with_capacity(4096); channels],
            output_bufs: vec![Vec::with_capacity(4096); channels],
            result_buf: Vec::with_capacity(8192),
        }
    }

    /// Process interleaved stereo samples with the given tempo ratio.
    /// ratio > 1.0 = faster playback (fewer output samples),
    /// ratio < 1.0 = slower playback (more output samples).
    /// Pass-through when ratio is approximately 1.0.
    pub fn process(&mut self, input: &[f32], ratio: f64) -> Vec<f32> {
        // Pass-through when ratio is close to 1.0 — reuse result_buf to avoid allocation
        if (ratio - 1.0).abs() < 0.005 {
            self.result_buf.clear();
            self.result_buf.extend_from_slice(input);
            return std::mem::take(&mut self.result_buf);
        }

        let frames_in = input.len() / self.channels;
        let frames_out = (frames_in as f64 / ratio).round() as usize;

        if frames_out == 0 || frames_in == 0 {
            return Vec::new();
        }

        // Deinterleave input into reusable per-channel buffers
        for buf in self.input_bufs.iter_mut() {
            buf.clear();
        }
        for frame in 0..frames_in {
            for ch in 0..self.channels {
                self.input_bufs[ch].push(input[frame * self.channels + ch]);
            }
        }

        // Prepare output buffers (resize + zero without realloc if capacity suffices)
        for buf in self.output_bufs.iter_mut() {
            buf.clear();
            buf.resize(frames_out, 0.0);
        }

        self.stretcher.process_vec(
            &self.input_bufs,
            frames_in as i32,
            &mut self.output_bufs,
            frames_out as i32,
        );

        // Re-interleave output into reusable buffer
        self.result_buf.clear();
        self.result_buf.reserve(frames_out * self.channels);
        for frame in 0..frames_out {
            for ch in 0..self.channels {
                self.result_buf.push(self.output_bufs[ch][frame]);
            }
        }

        std::mem::take(&mut self.result_buf)
    }

    /// Reset the stretcher state (e.g., on seek).
    pub fn reset(&mut self) {
        self.stretcher = ssstretch::Stretch::new();
        self.stretcher.preset_default(self.channels as i32, self.sample_rate as f32);
    }
}

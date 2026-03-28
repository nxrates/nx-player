use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use rtrb::Consumer;

pub struct AudioOutput {
    stream: cpal::Stream,
}

impl AudioOutput {
    /// Create a new audio output that reads interleaved stereo f32 from the given consumer.
    /// Returns the output and the device sample rate.
    pub fn new(mut consumer: Consumer<f32>) -> Result<(Self, u32), String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No audio output device found".to_string())?;

        let config = device
            .default_output_config()
            .map_err(|e| format!("Failed to get default output config: {}", e))?;

        let sample_rate = config.sample_rate().0;
        let channels = config.channels() as usize;

        let stream_config = cpal::StreamConfig {
            channels: channels as u16,
            sample_rate: cpal::SampleRate(sample_rate),
            buffer_size: cpal::BufferSize::Default,
        };

        let stream = device
            .build_output_stream(
                &stream_config,
                move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
                    for frame in data.chunks_mut(channels) {
                        let left = consumer.pop().unwrap_or(0.0);
                        let right = consumer.pop().unwrap_or(0.0);
                        // If device has more than 2 channels, fill extras with silence
                        for (i, sample) in frame.iter_mut().enumerate() {
                            *sample = match i {
                                0 => left,
                                1 => right,
                                _ => 0.0,
                            };
                        }
                    }
                },
                |err| {
                    eprintln!("Audio output error: {}", err);
                },
                None,
            )
            .map_err(|e| format!("Failed to build output stream: {}", e))?;

        stream
            .pause()
            .map_err(|e| format!("Failed to pause stream: {}", e))?;

        Ok((Self { stream }, sample_rate))
    }

    pub fn pause(&self) {
        let _ = self.stream.pause();
    }

    pub fn resume(&self) {
        let _ = self.stream.play();
    }
}

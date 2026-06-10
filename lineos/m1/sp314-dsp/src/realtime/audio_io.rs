// src/realtime/audio_io.rs
// cpal device setup and audio callbacks.
// CRITICAL: callbacks must never allocate, lock, or block.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{Stream, StreamConfig};

pub struct AudioConfig {
    pub sample_rate: u32,      // target: 48000
    pub block_size: usize,     // 512 frames
    pub input_device: String,  // "default" or device name
    pub output_device: String, // "default" or device name
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            sample_rate: 48000,
            block_size: 512,
            input_device: "default".to_string(),
            output_device: "default".to_string(),
        }
    }
}

/// Set up cpal input + output streams.
/// Returns (input_stream, output_stream) — keep alive for duration of session.
pub fn setup_streams(
    config: &AudioConfig,
    mut input_producer: ringbuf::HeapProducer<f32>,
    mut output_consumer: ringbuf::HeapConsumer<f32>,
) -> Result<(Stream, Stream), Box<dyn std::error::Error>> {
    let host = cpal::default_host();

    let input_device = if config.input_device == "default" {
        host.default_input_device()
            .ok_or("No default input device found")?
    } else {
        host.input_devices()?
            .find(|x| x.name().unwrap_or_default() == config.input_device)
            .ok_or("Input device not found")?
    };

    let output_device = if config.output_device == "default" {
        host.default_output_device()
            .ok_or("No default output device found")?
    } else {
        host.output_devices()?
            .find(|x| x.name().unwrap_or_default() == config.output_device)
            .ok_or("Output device not found")?
    };

    let in_config = StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(config.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let out_config = StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(config.sample_rate),
        buffer_size: cpal::BufferSize::Default,
    };

    let err_fn = |err| eprintln!("An error occurred on the audio stream: {}", err);

    let in_stream = input_device.build_input_stream(
        &in_config,
        move |data: &[f32], _: &cpal::InputCallbackInfo| {
            // Write input data into the ring buffer
            let _pushed = input_producer.push_slice(data);
        },
        err_fn,
        None,
    )?;

    let out_stream = output_device.build_output_stream(
        &out_config,
        move |data: &mut [f32], _: &cpal::OutputCallbackInfo| {
            let written = output_consumer.pop_slice(data);
            // Fill remaining with silence if underrun
            data[written..].fill(0.0);
        },
        err_fn,
        None,
    )?;

    in_stream.play()?;
    out_stream.play()?;

    Ok((in_stream, out_stream))
}

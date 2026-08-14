use hound::WavReader;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::sliding_overlap_reader::SlidingOverlapReader;

struct TestMemorySource {
    data: Vec<f32>,
    offset: usize,
}

impl sp314_dsp::stft::sliding_overlap_reader::ChunkSource for TestMemorySource {
    fn channels(&self) -> usize {
        2
    }
    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
        let remain = self.data.len() - self.offset;
        if remain == 0 {
            return Ok(0);
        }
        let to_read = remain.min(buffer.len());
        buffer[..to_read].copy_from_slice(&self.data[self.offset..self.offset + to_read]);
        self.offset += to_read;
        Ok(to_read / 2)
    }
}

#[test]
fn test_phi1_observe_smoke() {
    let wav_path = "/tmp/phi1/eq_test_48k.wav";
    if !std::path::Path::new(wav_path).exists() {
        println!("SKIPPED: {} not found", wav_path);
        return;
    }

    let mut reader = WavReader::open(wav_path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => reader.samples::<i16>().map(|s| s.unwrap() as f32).collect(),
    };

    let mono_signal = samples.clone();
    let mut interleaved = Vec::with_capacity(samples.len() * 2);
    for s in samples.iter() {
        interleaved.push(*s);
        interleaved.push(*s); // Duplicate to stereo
    }

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&mono_signal, &mono_signal, 48000, None, None, false);

    let reader_on = SlidingOverlapReader::new(
        TestMemorySource {
            data: interleaved,
            offset: 0,
        },
        10240,
    );

    let mut total_frames = 0;
    let mut frames_with_phi1_some = 0;
    let mut first_10_phi1_values = Vec::new();
    let mut min_phi1 = f32::INFINITY;
    let mut max_phi1 = f32::NEG_INFINITY;

    let mut observer = |obs: sp314_dsp::analysis::vad_model::VadObservation| {
        total_frames += 1;
        if total_frames <= 10 {
            first_10_phi1_values.push(obs.phi1_p);
        }
        if let Some(p) = obs.phi1_p {
            frames_with_phi1_some += 1;
            if p < min_phi1 { min_phi1 = p; }
            if p > max_phi1 { max_phi1 = p; }
        }
    };

    let mut on_voice: Vec<f32> = Vec::new();

    let _ = engine
        .process_stream_with_params(
            reader_on,
            &scout,
            1.0,
            false,
            false,
            &[],
            48000.0,
            None,
            Some(&mut observer),
            true, // phi1_enabled: true
            |chunk| {
                on_voice.extend_from_slice(&chunk.voice.l);
            },
        )
        .unwrap();

    println!("Total frames: {}", total_frames);
    println!("Frames with phi1_some: {}", frames_with_phi1_some);
    println!("First 10 values: {:?}", first_10_phi1_values);
    if frames_with_phi1_some > 0 {
        println!("Min phi1 value: {}", min_phi1);
        println!("Max phi1 value: {}", max_phi1);
    }
    
    assert!(frames_with_phi1_some > 0,
            "sensor produced no output");
    assert_eq!(total_frames - frames_with_phi1_some, 51,
               "context fill should be exactly 51 frames");
    assert!(min_phi1 >= 0.0 && max_phi1 <= 1.0,
            "p out of range: {} .. {}", min_phi1, max_phi1);
}

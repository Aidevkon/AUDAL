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
#[ignore]
fn w14_boundary_check() {
    let wav_path = "/tmp/w9/podcast_realistic.wav";
    if !std::path::Path::new(wav_path).exists() {
        println!("SKIPPED: {} not found", wav_path);
        return;
    }

    let mut reader = WavReader::open(wav_path).unwrap();
    let spec = reader.spec();
    let channels = spec.channels as usize;

    let mono: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => {
            let samples: Vec<f32> = reader.samples::<f32>().map(|s| s.unwrap()).collect();
            if channels == 1 {
                samples
            } else {
                samples.chunks(channels).map(|c| c.iter().sum::<f32>() / channels as f32).collect()
            }
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            let max_val = (1i64 << (bits - 1)) as f32;
            let samples: Vec<i32> = reader.samples::<i32>().map(|s| s.unwrap()).collect();
            if channels == 1 {
                samples.into_iter().map(|s| s as f32 / max_val).collect()
            } else {
                samples.chunks(channels).map(|c| {
                    c.iter().map(|&s| s as f32 / max_val).sum::<f32>() / channels as f32
                }).collect()
            }
        }
    };

    // Build stereo interleaved
    let mut interleaved = Vec::with_capacity(mono.len() * 2);
    for &s in &mono {
        interleaved.push(s);
        interleaved.push(s);
    }

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&mono, &mono, 48000, None, None, false);

    let reader_on = SlidingOverlapReader::new(
        TestMemorySource {
            data: interleaved,
            offset: 0,
        },
        10240,
    );

    let mut all_phi1: Vec<Option<f32>> = Vec::new();

    let mut observer = |obs: sp314_dsp::analysis::vad_model::VadObservation| {
        all_phi1.push(obs.phi1_p);
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
            true, // phi1_enabled
            |chunk| {
                on_voice.extend_from_slice(&chunk.voice);
            },
        )
        .unwrap();

    let total = all_phi1.len();
    let some_count = all_phi1.iter().filter(|x| x.is_some()).count();
    let none_count = total - some_count;

    // Collect indices of all None values
    let none_indices: Vec<usize> = all_phi1
        .iter()
        .enumerate()
        .filter(|(_, x)| x.is_none())
        .map(|(i, _)| i)
        .collect();

    let mut min_phi1 = f32::INFINITY;
    let mut max_phi1 = f32::NEG_INFINITY;
    for p in all_phi1.iter().flatten() {
        if *p < min_phi1 { min_phi1 = *p; }
        if *p > max_phi1 { max_phi1 = *p; }
    }

    println!("total_frames: {}", total);
    println!("frames_with_some: {}", some_count);
    println!("none_count: {} (expected 51)", none_count);
    println!("none_indices: {:?}", none_indices);
    if some_count > 0 {
        println!("min_phi1: {}", min_phi1);
        println!("max_phi1: {}", max_phi1);
    }

    // Check that None only appears at indices 0..51
    let none_only_at_start = none_indices.iter().enumerate().all(|(j, &idx)| idx == j);
    println!("none_only_at_start: {}", none_only_at_start);

    assert_eq!(
        none_count, 51,
        "None frames should appear ONLY at the start"
    );
}

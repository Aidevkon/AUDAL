use hound::WavReader;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_dsp::stft::sliding_overlap_reader::SlidingOverlapReader;
use sp314_dsp::dsp::control_bus::Ducker;
use std::path::Path;

struct TestMemorySource {
    data: Vec<f32>,
    offset: usize,
}

impl sp314_dsp::stft::sliding_overlap_reader::ChunkSource for TestMemorySource {
    fn channels(&self) -> usize { 2 }
    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, String> {
        let remain = self.data.len() - self.offset;
        if remain == 0 { return Ok(0); }
        let to_read = remain.min(buffer.len());
        buffer[..to_read].copy_from_slice(&self.data[self.offset..self.offset + to_read]);
        self.offset += to_read;
        Ok(to_read / 2)
    }
}

fn dz(p: f32, t: f32) -> f32 {
    if p < t { 0.0 } else { (p - t) / (1.0 - t) }
}

fn read_wav_interleaved(path: &str) -> Vec<f32> {
    let mut reader = WavReader::open(path).unwrap();
    let spec = reader.spec();
    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap_or(0.0)).collect(),
        hound::SampleFormat::Int => reader.samples::<i16>().map(|s| s.unwrap_or(0) as f32).collect(),
    };
    if spec.channels == 1 {
        let mut inter = Vec::with_capacity(samples.len() * 2);
        for s in samples {
            inter.push(s);
            inter.push(s);
        }
        inter
    } else {
        samples
    }
}

#[ignore]
#[test]
fn test_phi1_audition() {
    let dir = "/tmp/w6b/AM_Contra_-_Heart_Peripheral__snrp3";
    let mix_path = format!("{}/mix.wav", dir);
    let mix_interleaved = read_wav_interleaved(&mix_path);
    
    let inner_dir = format!("{}/AM_Contra_-_Heart_Peripheral__snrp3", dir);
    let vocals = read_wav_interleaved(&format!("{}/vocals.wav", inner_dir));
    let drums = read_wav_interleaved(&format!("{}/drums.wav", inner_dir));
    let bass = read_wav_interleaved(&format!("{}/bass.wav", inner_dir));
    let other = read_wav_interleaved(&format!("{}/other.wav", inner_dir));

    let len = vocals.len().min(drums.len()).min(bass.len()).min(other.len()).min(mix_interleaved.len());
    
    let mut bed = vec![0.0f32; len];
    for i in 0..len {
        bed[i] = drums[i] + bass[i] + other[i];
    }
    let voice = &vocals[..len];

    let mut mono_mix = Vec::with_capacity(len / 2);
    for i in 0..(len/2) {
        mono_mix.push((mix_interleaved[2*i] + mix_interleaved[2*i+1]) * 0.5);
    }

    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&mono_mix, &mono_mix, 48000, None, None, false);

    let reader_on = SlidingOverlapReader::new(
        TestMemorySource {
            data: mix_interleaved.clone(),
            offset: 0,
        },
        10240,
    );

    let mut old_gains: Vec<f32> = Vec::new();
    let mut new_gains: Vec<f32> = Vec::new();

    let mut d_old = Ducker::new(48000.0);
    let mut d_new = Ducker::new(48000.0);

    let mut observer = |obs: sp314_dsp::analysis::vad_model::VadObservation| {
        let g_old = d_old.update(obs.posterior);
        
        let p_new = obs.phi1_p.unwrap_or(f32::NAN);
        let p_trans = if p_new.is_finite() { dz(p_new, 0.30) } else { f32::NAN };
        let g_new = d_new.update(p_trans);

        old_gains.push(g_old);
        new_gains.push(g_new);
    };

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
            |_chunk| {},
        )
        .unwrap();

    let render = |gains: &[f32], out_path: &str| {
        let mut out = vec![0.0f32; len];
        let hop = 480;
        let mut peak = 0.0f32;
        
        for i in 0..(len / 2) {
            let frame_idx = i / hop;
            let next_frame_idx = (frame_idx + 1).min(gains.len().saturating_sub(1));
            
            let g0 = if frame_idx < gains.len() { gains[frame_idx] } else { 1.0 };
            let g1 = if next_frame_idx < gains.len() { gains[next_frame_idx] } else { 1.0 };
            
            let frac = ((i % hop) as f32) / (hop as f32);
            let g = g0 + (g1 - g0) * frac;
            
            let l_bed = bed[2*i] * g;
            let r_bed = bed[2*i+1] * g;
            
            let l_out = l_bed + voice[2*i];
            let r_out = r_bed + voice[2*i+1];
            
            out[2*i] = l_out;
            out[2*i+1] = r_out;
            
            if l_out.abs() > peak { peak = l_out.abs(); }
            if r_out.abs() > peak { peak = r_out.abs(); }
        }
        
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(out_path, spec).unwrap();
        for sample in &out {
            writer.write_sample(*sample).unwrap();
        }
        writer.finalize().unwrap();
        
        let dur = len as f32 / 2.0 / 48000.0;
        println!("Path: {} | Duration: {:.2}s | Peak: {:.4}", out_path, dur, peak);
    };

    render(&old_gains, "/tmp/audition/A_old_vad.wav");
    render(&new_gains, "/tmp/audition/B_neural_deadzone.wav");
}

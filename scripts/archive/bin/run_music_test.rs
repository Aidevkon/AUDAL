use std::path::PathBuf;
use m0d::domain::episode_render;
use m0d::dsp::DspAdapter;
use lineos_types::mastering::MasteringIntent;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: run_ltass_test <in.wav> <out.pcm>");
        return;
    }
    let wav_path = &args[1];
    let out_path = &args[2];

    let mut reader = hound::WavReader::open(wav_path).unwrap();
    let spec = reader.spec();
    let pcm_path = format!("{}.pcm", wav_path);
    {
        let mut pcm_file = std::fs::File::create(&pcm_path).unwrap();
        use std::io::Write;
        match spec.sample_format {
            hound::SampleFormat::Float => {
                for s in reader.samples::<f32>() {
                    let bytes = s.unwrap().to_le_bytes();
                    pcm_file.write_all(&bytes).unwrap();
                }
            },
            hound::SampleFormat::Int => {
                let max_val = (1i32 << (spec.bits_per_sample - 1)) as f32;
                for s in reader.samples::<i32>() {
                    let f = s.unwrap() as f32 / max_val;
                    let bytes = f.to_le_bytes();
                    pcm_file.write_all(&bytes).unwrap();
                }
            }
        }
    }
    
    let trunk_metrics = sp314_orchestrator::trunk_pass::run_trunk_metrics(std::path::Path::new(&pcm_path)).unwrap();
    let pre = trunk_metrics.to_pre_analysis(-144.0, 0.0, vec![], vec![], vec![]);
    let streaming_features = lineos_types::StemFeatures {
        bass: lineos_types::StemMetrics::default(),
        harmonics: lineos_types::StemMetrics::default(),
        voice: lineos_types::StemMetrics::default(),
        drums: lineos_types::StemMetrics::default(),
        ambience: lineos_types::StemMetrics::default(),
        mix: lineos_types::MixMetrics::default(),
    };

    let icfg = m0d::domain::nodes::dsp_node::build_intent_and_config(
        "music", // preset_id
        "pox_voice", // flavour_id
        None,
        None,
        None,
        None,
        None,
        Some(-16.0),
        &streaming_features,
        &pre,
    ).unwrap();

    let dummy_scout = vec![0.0f32; 48000];
    
    let graph = DspAdapter::build_graph_only(
        &icfg.intent,
        &dummy_scout,
        &dummy_scout,
        spec.sample_rate,
        &[0.2; 5],
        Some(&icfg.dsp_config),
        512,
    ).unwrap();

    let mut source = m0d::dsp::lazy_reader::LazyAudioReader::open(std::path::Path::new(wav_path)).unwrap();
    let res = episode_render::run(&mut source, "music_test", graph, -16.0, &pre, |_| Ok(())).unwrap();
    
    std::fs::copy("/tmp/m0d-mastering-music_test.pcm", out_path).unwrap();
    println!("Rendered to {}", out_path);
    let out_metrics = sp314_orchestrator::trunk_pass::run_trunk_metrics(std::path::Path::new(out_path)).unwrap();
    let out_pre = out_metrics.to_pre_analysis(-144.0, 0.0, vec![], vec![], vec![]);
    println!("OUTPUT LUFS: {:?}", out_metrics.integrated_lufs);
    println!("OUTPUT DYNAMIC RANGE: {}", out_metrics.dynamic_range_db);
    println!("OUTPUT SPECTRAL PROFILE: {:?}", out_pre.spectral_profile_db);
}

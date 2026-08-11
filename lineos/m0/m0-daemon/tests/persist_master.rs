use arc_swap::ArcSwap;
use m0d::domain::dsp_pipeline::run_dsp;
use m0d::handlers::master::MasterRequest;
use std::sync::Arc;
use xaak::repo::DspState;

#[test]
fn test_persist_master() {
    let raw_path = std::env::current_dir()
        .unwrap()
        .join("tests/fixtures/test_stereo_input.wav");

    // 1. First track
    let req1 = MasterRequest {
        audio_path: raw_path.to_string_lossy().to_string(),
        preset_id: "acx".to_string(), // acx is reliable
        project_id: Some("proj-persist".to_string()),
        track_id: Some("track1".to_string()),
        flavour_id: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        intent_tone: None,
        intent_dynamics: None,
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    };

    let state = Arc::new(ArcSwap::from_pointee(DspState::default()));
    let masters_tmp = tempfile::TempDir::new().unwrap();
    let state_tmp = tempfile::TempDir::new().unwrap();

    let (blob1, _, _, _, _, artifacts1) = run_dsp(
        &req1,
        std::time::Instant::now(),
        state.clone(),
        None,
        None,
        "job1".to_string(),
        state_tmp.path().to_str().unwrap(),
        masters_tmp.path().to_str().unwrap(),
    )
    .expect("run_dsp failed");

    let flac_path1 = artifacts1
        .persisted_master
        .expect("Master should be persisted");
    assert!(flac_path1.exists(), "FLAC file must exist on disk");
    assert!(flac_path1.ends_with("proj-persist/track1.flac"));

    // Decode length check (decoded_len >= rendered_len && decoded_len < rendered_len + 4096)
    let decoded_frames1 = {
        let file = std::fs::File::open(&flac_path1).unwrap();
        let mss = symphonia::core::io::MediaSourceStream::new(Box::new(file), Default::default());
        let hint = symphonia::core::probe::Hint::new();
        let probed = symphonia::default::get_probe()
            .format(&hint, mss, &Default::default(), &Default::default())
            .unwrap();
        let mut format = probed.format;
        let track = format.default_track().unwrap();
        let mut decoder = symphonia::default::get_codecs()
            .make(&track.codec_params, &Default::default())
            .unwrap();

        let mut total = 0;
        loop {
            match format.next_packet() {
                Ok(packet) => {
                    if let Ok(decoded) = decoder.decode(&packet) {
                        total += decoded.frames();
                    }
                }
                Err(symphonia::core::errors::Error::IoError(_)) => break,
                Err(_) => break, // EOF or other error
            }
        }
        total
    };

    assert!(
        decoded_frames1 >= blob1.core.num_frames && decoded_frames1 < blob1.core.num_frames + 4096,
        "Decoded frames ({}) mismatch vs rendered ({})",
        decoded_frames1,
        blob1.core.num_frames
    );

    // 2. Second track in same project
    let req2 = MasterRequest {
        audio_path: raw_path.to_string_lossy().to_string(),
        preset_id: "acx".to_string(),
        project_id: Some("proj-persist".to_string()),
        track_id: Some("track2".to_string()),
        flavour_id: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        intent_tone: None,
        intent_dynamics: None,
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    };

    let (_, _, _, _, _, artifacts2) = run_dsp(
        &req2,
        std::time::Instant::now(),
        state.clone(),
        None,
        None,
        "job2".to_string(),
        state_tmp.path().to_str().unwrap(),
        masters_tmp.path().to_str().unwrap(),
    )
    .expect("run_dsp failed");

    let flac_path2 = artifacts2
        .persisted_master
        .expect("Master should be persisted");
    assert!(flac_path2.exists(), "FLAC file must exist on disk");
    assert!(flac_path2.ends_with("proj-persist/track2.flac"));

    // 3. Request without project_id
    let req3 = MasterRequest {
        audio_path: raw_path.to_string_lossy().to_string(),
        preset_id: "acx".to_string(),
        project_id: None,
        track_id: Some("track3".to_string()),
        flavour_id: None,
        persona_id: None,
        tone: None,
        dynamics: None,
        chaos_seed: None,
        intent_tone: None,
        intent_dynamics: None,
        mix_levels: None,
        normalizer_ceiling_db: None,
        preview_id: None,
        restoration_enabled: None,
        macro_router_enabled: None,
        vad_observe_enabled: None,
        use_nmfd: None,
    };

    let (_, _, _, _, _, artifacts3) = run_dsp(
        &req3,
        std::time::Instant::now(),
        state.clone(),
        None,
        None,
        "job3".to_string(),
        state_tmp.path().to_str().unwrap(),
        masters_tmp.path().to_str().unwrap(),
    )
    .expect("run_dsp failed");

    assert!(
        artifacts3.persisted_master.is_none(),
        "Master should not be persisted without project_id"
    );
}

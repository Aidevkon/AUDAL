use sp314_dsp::pipeline::presets::MasteringTarget;

#[test]
fn presets_all_variants_construct_without_panic() {
    let targets = [
        MasteringTarget::Transparent,
        MasteringTarget::SpotifyV3,
        MasteringTarget::PodcastVoice,
        MasteringTarget::ClassicalAcoustic,
        MasteringTarget::LoudMaster,
        MasteringTarget::AggressiveEDM,
        MasteringTarget::BroadcastVideo,
        MasteringTarget::AtscA85,
    ];
    for t in targets.iter() {
        let _config = t.engine_config(48000);
    }
}

#[test]
fn presets_transparent_has_zero_eq_boost() {
    let config = MasteringTarget::Transparent.engine_config(48000);
    for db in config.eq_config.target_db.iter() {
        assert_eq!(*db, 0.0);
    }
    assert_eq!(config.parallel_mix, 0.0);
}

#[test]
fn presets_transparent_has_unity_ratio() {
    let config = MasteringTarget::Transparent.engine_config(48000);
    assert_eq!(config.comp_config.mid_config.ratio, 1.0);
    assert_eq!(config.comp_config.side_config.ratio, 1.0);
}

#[test]
fn presets_target_rms_values_are_ordered() {
    let edm = MasteringTarget::AggressiveEDM.target_lufs().unwrap();
    let loud = MasteringTarget::LoudMaster.target_lufs().unwrap();
    let spotify = MasteringTarget::SpotifyV3.target_lufs().unwrap();
    let podcast = MasteringTarget::PodcastVoice.target_lufs().unwrap();
    let classical = MasteringTarget::ClassicalAcoustic.target_lufs().unwrap();
    let broadcast = MasteringTarget::BroadcastVideo.target_lufs().unwrap();
    let atsc = MasteringTarget::AtscA85.target_lufs().unwrap();

    assert!(edm > loud);
    assert!(loud > spotify);
    assert!(spotify > podcast);
    assert!(podcast > classical);
    assert!(classical > broadcast);
    assert!(broadcast > atsc);
}

#[test]
fn presets_all_use_150hz_crossover() {
    let targets = [
        MasteringTarget::Transparent,
        MasteringTarget::SpotifyV3,
        MasteringTarget::PodcastVoice,
        MasteringTarget::ClassicalAcoustic,
        MasteringTarget::LoudMaster,
        MasteringTarget::AggressiveEDM,
        MasteringTarget::BroadcastVideo,
        MasteringTarget::AtscA85,
    ];
    for t in targets.iter() {
        let config = t.engine_config(48000);
        assert_eq!(config.comp_config.mid_config.crossover_hz, 150.0);
        assert_eq!(config.comp_config.side_config.crossover_hz, 150.0);
    }
}

#[test]
fn presets_all_have_soft_knee() {
    let targets = [
        MasteringTarget::Transparent,
        MasteringTarget::SpotifyV3,
        MasteringTarget::PodcastVoice,
        MasteringTarget::ClassicalAcoustic,
        MasteringTarget::LoudMaster,
        MasteringTarget::AggressiveEDM,
        MasteringTarget::BroadcastVideo,
        MasteringTarget::AtscA85,
    ];
    for t in targets.iter() {
        let config = t.engine_config(48000);
        assert_eq!(config.comp_config.mid_config.knee_db, 2.0);
        assert_eq!(config.comp_config.side_config.knee_db, 2.0);
    }
}

#[test]
fn presets_display_names_are_non_empty() {
    let targets = [
        MasteringTarget::Transparent,
        MasteringTarget::SpotifyV3,
        MasteringTarget::PodcastVoice,
        MasteringTarget::ClassicalAcoustic,
        MasteringTarget::LoudMaster,
        MasteringTarget::AggressiveEDM,
        MasteringTarget::BroadcastVideo,
        MasteringTarget::AtscA85,
    ];
    for t in targets.iter() {
        assert!(t.display_name().len() > 0);
    }
    assert_eq!(
        MasteringTarget::Transparent.display_name(),
        "Natural (No Processing)"
    );
    assert!(MasteringTarget::BroadcastVideo
        .display_name()
        .contains("EBU R128"));
}

#[test]
fn presets_transparent_target_rms_is_none() {
    assert!(MasteringTarget::Transparent.target_lufs().is_none());

    let active_targets = [
        MasteringTarget::SpotifyV3,
        MasteringTarget::PodcastVoice,
        MasteringTarget::ClassicalAcoustic,
        MasteringTarget::LoudMaster,
        MasteringTarget::AggressiveEDM,
        MasteringTarget::BroadcastVideo,
        MasteringTarget::AtscA85,
    ];
    for t in active_targets.iter() {
        assert!(t.target_lufs().is_some());
    }
}

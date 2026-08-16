# Ο ΣΤΟΛΟΣ ΤΩΝ IGNORED

| Αρχείο:Γραμμή | Όνομα Test | Αιτιολογία | Υποδομή |
| `lineos/m0/m0-daemon/src/dsp/orchestrator/nmf_worker.rs:123` | `test_nmf_worker_e2e_throwaway` | ΧΩΡΙΣ | Fixture |
| `lineos/m0/m0-daemon/tests/ab_render_full.rs:5` | `fixture_path` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/ab_render_full.rs:20` | `render_ab_full_pipeline` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/acx_export_rms_window.rs:99` | `acx_export_lifts_quiet_signal_into_rms_window` | ΜΕ: writes mp3, runs LAME encoder | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/acx_export_rms_window.rs:123` | `acx_export_leaves_compliant_signal_alone` | ΜΕ: writes mp3, runs LAME encoder | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/acx_export_rms_window.rs:145` | `acx_export_lowers_hot_signal_into_rms_window` | ΜΕ: writes mp3, runs LAME encoder | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/audition.rs:51` | `audition` | ΜΕ: audition — writes files for listening | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/deliver_acx.rs:169` | `test_deliver_acx_ffprobe` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/e2e_episode_streaming.rs:13` | `generate_podcast_fixture` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/e2e_episode_streaming.rs:236` | `full_pipeline_heap_is_scale_invariant` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/e2e_episode_streaming.rs:328` | `music_pipeline_heap_is_scale_invariant` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/e2e_mastering_quality.rs:147` | `inv_qa_5_nmfd_true_path_active` | ΜΕ: W5.d true-path gate: slow NMFD render | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/e2e_mastering_quality.rs:353` | `inv_qa_3_spectral_balance_full` | ΜΕ: Full-length spectral baseline, runs too slow for routine CI; run manually for alignment verification | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/e2e_mastering_quality.rs:936` | `inv_qa_6_mud_correction` | ΜΕ: X0: μετρούσε παρενέργεια bug. Πριν το fb53431 οι topology parameters αγνοούνταν και ο ReverbNode έτρεχε με constructor defaults — το decorrelation έδινε mud 5.15→3.84 (−25.3%). Το fb53431 διόρθωσε την εφαρμογή των parameters· τώρα ambience_reverb mix=0.0 και ambience_width decorrelation=0.0, γιατί config.ambience είναι None σε αυτό το preset (dsp/mod.rs else branch). Μετρημένο τώρα: 5.15→5.31. Ο MaskingEQ ήταν ΠΑΝΤΑ dormant (gains_db=[0.0;8]) — το ίδιο το docstring το λέει. Το gate χρειάζεται preset με config.ambience=Some, ή αναδιατύπωση σε ό,τι το pipeline ΟΝΤΩΣ υπόσχεται. | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/e2e_streaming_http_hybrid_smoke.rs:10` | `test_streaming_http_hybrid_smoke` | ΜΕ: Requires local untracked flight_clips_stereo dataset | Local flight_clips_stereo dataset (όχι στο repo) |
| `lineos/m0/m0-daemon/tests/export_flac_real.rs:124` | `test_export_flac_ffprobe` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m0/m0-daemon/tests/export_mp3_acx.rs:129` | `test_export_mp3_acx_ffprobe` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/glue_full_render.rs:10` | `glue_full_render` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/integration_router_concurrency.rs:7` | `test_router_concurrency_limit_applies_http_backpressure` | ΜΕ: X0: το /dev/wait endpoint αφαιρέθηκε, το test δεν ενημερώθηκε. Επιστρέφει 404 αντί 200. Χρειάζεται νέο test endpoint ή αναδιατύπωση. | Άγνωστη/Καμία εμφανής |
| `lineos/m0/m0-daemon/tests/inv_det_1_render_determinism.rs:49` | `inv_det_1_render_determinism` | ΜΕ: full render x2 (~15s measured, release) — run explicitly on any audio-touching wire | Fixture |
| `lineos/m0/m0-daemon/tests/inv_det_2_batch_determinism.rs:15` | `inv_det_2_batch_determinism` | ΜΕ: §Β: δεν υπάρχει καλέσιμο batch API — το batch ζει ως ασύγχρονα μηνύματα Operator→Conductor→Executor. Χρειάζεται run_batch() ως συνάρτηση πρώτα. | Άγνωστη/Καμία εμφανής |
| `lineos/m0/m0-daemon/tests/pass1_integration.rs:6` | `test_build_timeline_map` | ΧΩΡΙΣ | Local flight_clips_stereo dataset (όχι στο repo) |
| `lineos/m0/m0-daemon/tests/spatial_folddown_agrees_with_stereo.rs:114` | `spatial_folddown_agrees_with_stereo` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/stem_independence.rs:81` | `the_five_stems_are_distinct_signals` | ΜΕ: runs NMFD separation | Fixture |
| `lineos/m0/m0-daemon/tests/streaming_integration.rs:24` | `streaming_pipeline_ducking_e2e` | ΧΩΡΙΣ | Local flight_clips_stereo dataset (όχι στο repo) |
| `lineos/m0/m0-daemon/tests/vad_validation_real.rs:365` | `vad_narration_dream` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/vad_validation_real.rs:384` | `vad_narration_crossing` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/vad_validation_real.rs:414` | `vad_transient_probe` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w10_duck_dynamics.rs:73` | `w10_duck_dynamics` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w17_bed_sweep.rs:71` | `w17_bed_sweep` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w17_flacenc_bloat.rs:29` | `w17_flacenc_bloat_repro` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w17_flacenc_narrow.rs:25` | `w17_flacenc_narrow` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w17_mix_balance.rs:66` | `w17_mix_balance` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w1_vad_trace.rs:4` | `fixture_path` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w1_vad_trace.rs:19` | `render_vad_trace_vehicle` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:18` | `w2_duck_gate_synth` | ΜΕ: full render + /tmp fixtures, ~2min — run explicitly on any audio-touching wire | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:160` | `w2_duck_gate_real` | ΜΕ: full render + /tmp fixtures, ~2min — run explicitly on any audio-touching wire | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:274` | `w2_duck_gate_synth_variance` | ΜΕ: full render + /tmp fixtures, ~2min — run explicitly on any audio-touching wire | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:338` | `w3b_mix_levels_gate` | ΜΕ: full render + /tmp fixtures, ~2min — run explicitly on any audio-touching wire | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:389` | `w4_ceiling_gate` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:437` | `w4_ceiling_gate` | ΜΕ: full render + /tmp fixtures, ~2min — run explicitly on any audio-touching wire | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w2_duck_gate.rs:599` | `w4_ceiling_sweep` | ΜΕ: full render + /tmp fixtures, ~2min — run explicitly on any audio-touching wire | /tmp fixtures |
| `lineos/m0/m0-daemon/tests/w6c_nmfd_cost.rs:9` | `w6c_nmfd_cost` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/src/analysis/stem_escalation.rs:120` | `test_escalate_to_stems_on_blur_zone` | ΜΕ: Requires local uncommitted flight clips in flight_clips_stereo/ | Local flight_clips_stereo dataset (όχι στο repo) |
| `lineos/m1/sp314-dsp/src/analysis/stem_escalation.rs:178` | `test_orchestrate_escalation_on_transition` | ΜΕ: Requires local uncommitted flight clips in flight_clips_stereo/ | Local flight_clips_stereo dataset (όχι στο repo) |
| `lineos/m1/sp314-dsp/src/stft/two_pass.rs:2741` | `test_streaming_seam_macro_batch` | ΜΕ: slow (8s): proves macro-batch Rayon boundaries do not drop/duplicate history. Run manually via --ignored | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/ab_render_fixture.rs:4` | `fixture_path` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/ab_render_fixture.rs:44` | `render_ab_variant` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/acx_check_real_file.rs:3` | `acx_check_real_file` | ΧΩΡΙΣ | ACX_WAV env var -> local file (όχι στο repo) |
| `lineos/m1/sp314-dsp/tests/acx_check_real_file.rs:33` | `acx_check_real_file` | ΜΕ: needs ACX_WAV env pointing at a local file | ACX_WAV env var -> local file (όχι στο repo) |
| `lineos/m1/sp314-dsp/tests/beta_scout_window.rs:8` | `test_beta_scout_window` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/fixture_factory.rs:142` | `build_fixtures` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/fixture_factory.rs:267` | `build_duck_splice_fixture` | ΧΩΡΙΣ | Fixture |
| `lineos/m1/sp314-dsp/tests/forensic_renders.rs:116` | `forensic_gate` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/forensic_renders.rs:156` | `forensic_deess` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/forensic_renders.rs:186` | `forensic_lowcut` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/forensic_renders.rs:217` | `forensic_dehum` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/forensic_renders.rs:249` | `forensic_combined` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/forensic_renders.rs:328` | `forensic_impulse_response` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/forensic_renders.rs:346` | `forensic_sine_sweep` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/gamma_voice_leak.rs:6` | `test_gamma_voice_leak` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/glue_audition.rs:36` | `test_glue_audition` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/glue_characterize.rs:5` | `test_glue_characterize` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/glue_stem_probe.rs:5` | `test_glue_stem_probe` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/nmfd_erlangen_oracle.rs:106` | `test_nmfd_drift_diagnostic` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/nmfd_fit_cost.rs:131` | `test_nmfd_fit_cost` | ΧΩΡΙΣ | Fixture |
| `lineos/m1/sp314-dsp/tests/pass1_integration.rs:99` | `test_pass1_end_to_end` | ΜΕ: Requires local uncommitted flight clips in flight_clips_stereo/ | Local flight_clips_stereo dataset (όχι στο repo) |
| `lineos/m1/sp314-dsp/tests/phi1_audition.rs:47` | `test_phi1_audition` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/phi1_vs_dsp_jury.rs:22` | `test_phi1_vs_dsp_jury` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/w14_batch_equivalence.rs:6` | `w14_batch_equivalence` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/w14_boundary_check.rs:27` | `w14_boundary_check` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/w14_pcen_warmup.rs:6` | `w14_pcen_warmup` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/w16_mask_inflation.rs:7` | `w16_mask_inflation_real_data` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/w16_mel_inverse_probe.rs:4` | `w16_mel_inverse_probe` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/w16_nmfd_k_mismatch.rs:8` | `w16_nmfd_k_mismatch` | ΧΩΡΙΣ | Άγνωστη/Καμία εμφανής |
| `lineos/m1/sp314-dsp/tests/w17_stem_spectrum.rs:59` | `w17_stem_spectrum` | ΧΩΡΙΣ | /tmp fixtures |
| `lineos/m1/sp314-dsp/tests/w17_voice_activation.rs:74` | `w17_voice_activation` | ΧΩΡΙΣ | /tmp fixtures |

> ΔΙΟΡΘΩΣΗ ΜΕΤΑ ΤΗΝ ΠΑΡΑΔΟΣΗ: ο αρχικός κατάλογος (91) μετρούσε
> και αναφορές μέσα σε .md κείμενα. Εδώ ΜΟΝΟ πραγματικά attributes
> σε .rs — μετρημένα 77 με grep --include=*.rs, 2026-08-16.

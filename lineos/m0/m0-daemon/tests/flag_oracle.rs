//! flag_oracle.rs — Wiring probes for render_node boolean flags.
//!
//! Each test renders the same input twice with one flag toggled. The oracle
//! asserts STRUCTURAL properties of the output, not merely that outputs differ:
//! a "difference > ε" assertion passes even when the flag is ignored because
//! NMF non-determinism or floating-point noise can clear any small threshold.
//!
//! Construction pattern (all tests):
//!   1. Decode real_world_60s.wav (committed in sp314-dsp fixtures).
//!   2. Optionally modify samples in memory (scale quiet sections ×0.01).
//!   3. Write as raw f32 LE to a NamedTempFile — the format RawPcmFileSource
//!      expects (headerless, interleaved, 2 ch). Pattern from raw_pcm_source.rs:83.
//!   4. Build TwoPassEngine::new() + engine.scout(&mono, 48_000) independently
//!      for each render — identical signal → identical, deterministic ScoutResult.
//!   5. Call render_node::run with the flag off, then on.
//!   6. Assert structural properties of the output.
//!
//! NMF fixture note: pure synthetic tones route nearly all energy to the
//! "harmonics" stem, leaving voice and ambience near zero. Gate/pad effects then
//! change nothing — the test passes while the wiring is broken. real_world_60s.wav
//! provides real multi-stem speech content and is the only safe fixture here.
//!
//! OverlapChunk.signal is the mono downmix (L+R)/2 — confirmed at
//! sliding_overlap_reader.rs:10. The bypass path sets voice = raw_chunk = that
//! mono downmix; the correlation reference in test 3 is raw_mono accordingly.

use std::io::Write;

use lineos_corpus::scout::{SegmentBoundary, SegmentType};
use m0d::domain::nodes::render_node::{run, RenderInputs, RenderSettings};
use m0d::handlers::master::MixLevels;
use sp314_dsp::stft::sliding_overlap_reader::SlidingOverlapReader;
use sp314_dsp::stft::two_pass::TwoPassEngine;
use sp314_orchestrator::raw_pcm_source::RawPcmFileSource;
use tempfile::NamedTempFile;

const SR: u32 = 48_000;
const SR_US: usize = 48_000;

// ─── Fixture ─────────────────────────────────────────────────────────────────

fn fixture_path() -> std::path::PathBuf {
    // CARGO_MANIFEST_DIR is set at compile time to the crate root (m0-daemon/).
    // This is absolute and correct regardless of where `cargo test` is invoked.
    std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../m1/sp314-dsp/tests/fixtures/real_world_60s.wav")
}

fn require_fixture() -> std::path::PathBuf {
    let p = fixture_path();
    if !p.exists() {
        panic!(
            "\n\nFIXTURE MISSING: {}\n\
             Ensure lineos/m1/sp314-dsp/tests/fixtures/real_world_60s.wav is present.\n\
             This fixture is committed in the sp314-dsp crate and must not be deleted.\n",
            p.display()
        );
    }
    p
}

/// Decode the first `n_frames` stereo frames from a WAV, returning interleaved f32.
fn decode_first_n(path: &str, n_frames: usize) -> Vec<f32> {
    let (interleaved, _, _) = m0d::handlers::decode::decode_raw_interleaved(path).unwrap();
    let available = interleaved.len() / 2;
    interleaved[..n_frames.min(available) * 2].to_vec()
}

/// Write interleaved f32 stereo samples as raw f32 LE (no header) to a NamedTempFile.
/// Matches the format RawPcmFileSource::new(path, 2) reads.
/// The NamedTempFile must remain alive for the duration of any render that reads it.
fn write_raw_pcm(samples: &[f32]) -> NamedTempFile {
    let mut file = NamedTempFile::new().unwrap();
    let bytes: &[u8] = unsafe {
        std::slice::from_raw_parts(samples.as_ptr() as *const u8, samples.len() * 4)
    };
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
    file
}

// ─── Shared render helper ─────────────────────────────────────────────────────

/// Renders `n_frames` from `pcm_path` using a fresh TwoPassEngine scouted on
/// `scout_left`/`scout_right`. Returns the left output channel.
///
/// Each call constructs an independent engine so the two renders have no shared
/// state. Identical signals produce deterministically identical ScoutResults.
fn render_once(
    pcm_path: &std::path::Path,
    scout_left: &[f32],
    scout_right: &[f32],
    n_frames: usize,
    restoration_enabled: bool,
    macro_router_enabled: bool,
    mix_levels: Option<&MixLevels>,
    boundaries: &[SegmentBoundary],
) -> Vec<f32> {
    let mono: Vec<f32> = scout_left
        .iter()
        .zip(scout_right.iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();
    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&mono, SR);

    let source = RawPcmFileSource::new(pcm_path, 2).unwrap();
    let stream_source = SlidingOverlapReader::new(source, 10240);

    let mut left_out = vec![0.0_f32; n_frames];
    let mut right_out = vec![0.0_f32; n_frames];

    run(
        &mut engine,
        &scout,
        &RenderSettings {
            ducking_gain: 1.0,
            mix_levels,
            flavour_id: None,
            sample_rate: SR,
            noise_floor_dbfs: Some(-45.0),
            restoration_enabled,
            macro_router_enabled,
            boundaries,
            vad_observe_enabled: false,
            blob_id: "flag_oracle",
        },
        RenderInputs {
            original_sum_sq: 0.0,
            total_frames: n_frames,
            stream_source,
        },
        &mut left_out,
        &mut right_out,
        None,
    )
    .unwrap();

    left_out
}

// ─── Statistics ───────────────────────────────────────────────────────────────

fn mse(a: &[f32], b: &[f32]) -> f64 {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (*x as f64 - *y as f64).powi(2))
        .sum::<f64>()
        / a.len() as f64
}

fn mean_sq(a: &[f32]) -> f64 {
    a.iter().map(|x| (*x as f64).powi(2)).sum::<f64>() / a.len() as f64
}

fn pearson(a: &[f32], b: &[f32]) -> f64 {
    let n = a.len() as f64;
    let ma = a.iter().map(|&x| x as f64).sum::<f64>() / n;
    let mb = b.iter().map(|&x| x as f64).sum::<f64>() / n;
    let cov: f64 = a
        .iter()
        .zip(b.iter())
        .map(|(&x, &y)| (x as f64 - ma) * (y as f64 - mb))
        .sum();
    let var_a: f64 = a.iter().map(|&x| (x as f64 - ma).powi(2)).sum();
    let var_b: f64 = b.iter().map(|&x| (x as f64 - mb).powi(2)).sum();
    if var_a == 0.0 || var_b == 0.0 {
        return 0.0;
    }
    cov / (var_a.sqrt() * var_b.sqrt())
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 1 — vocal gate closes in the quiet region
// ─────────────────────────────────────────────────────────────────────────────
//
// ISOLATION: mix_levels = { voice: 1.0, rest: 0.0 }
//   The ambience=0 removes the glue-bus effect from this test.
//   The gate is the only remaining restoration path.
//
// FIXTURE: 3 s of real_world_60s.wav. Frames 48 000–96 000 (second 1→2)
//   scaled ×0.01: original ≈ −20 dBFS → ×0.01 = −40 dB shift → ≈ −60 dBFS,
//   which is 15 dB below the gate threshold (−45 dBFS, linear 0.005623).
//   Scout runs on the ORIGINAL 3 s so NMF learns real voice/stem structure.
//
// GATE TIMING (gate.rs:27-28):
//   Attack 1 ms, release 100 ms (τ = 4 800 samples), hold 50 ms = 2 400 samples.
//   After quiet onset: hold fires (2 400 samples), then release begins.
//   At 0.3 s into the quiet section (frame 62 400): elapsed post-hold ≈ 0.25 s
//   → gain ≈ e^(-0.25/0.1) = e^(-2.5) ≈ 0.082.
//   At 0.7 s into quiet (frame 81 600): gain ≈ e^(-6.5) ≈ 0.0015.
//
// THRESHOLDS: relative to mean_sq(off) for the region.
//   Loud: gate is fully open (target_gain = 1.0, start gain = 1.0 → no delta).
//     Expect diff << 0.1% of off energy. Spatial stage may add fixed gains but
//     applies identically to both runs — cancels in the diff.
//   Quiet: gate closed → on output ≈ 0, off output = voice stem.
//     Expect diff > 10% of off energy, and on is quieter than off.
//     Even 1% of signal energy in the voice stem at −60 dBFS gives a
//     relative diff well above 10%.

#[test]
fn restoration_vocal_gate_closes_in_quiet_region() {
    let fixture = require_fixture();
    let fixture_str = fixture.to_str().unwrap();
    let n_frames = 3 * SR_US;

    // Original 3 s — for scouting. NMF learns from real, un-scaled speech.
    let orig = decode_first_n(fixture_str, n_frames);
    let scout_left: Vec<f32> = orig.iter().step_by(2).copied().collect();
    let scout_right: Vec<f32> = orig.iter().skip(1).step_by(2).copied().collect();

    // Modified 3 s — second 1→2 scaled ×0.01 to create a gatable quiet region.
    let mut modified = orig.clone();
    for i in SR_US..2 * SR_US {
        modified[i * 2] *= 0.01; // L
        modified[i * 2 + 1] *= 0.01; // R
    }

    let pcm_file = write_raw_pcm(&modified);

    let mix_voice_only = MixLevels {
        voice: 1.0,
        drums: 0.0,
        bass: 0.0,
        harmonics: 0.0,
        ambience: 0.0,
    };

    let out_off = render_once(
        pcm_file.path(),
        &scout_left,
        &scout_right,
        n_frames,
        false, // restoration off
        false,
        Some(&mix_voice_only),
        &[],
    );
    let out_on = render_once(
        pcm_file.path(),
        &scout_left,
        &scout_right,
        n_frames,
        true, // restoration on
        false,
        Some(&mix_voice_only),
        &[],
    );

    // Region boundaries:
    //   loud1:  [ 6_000 ..  42_000]  0.125 s – 0.875 s of second 0
    //   quiet:  [62_400 ..  96_000]  0.300 s – 1.000 s into the quiet second
    //   loud2:  [102_000 .. 138_000] 0.125 s – 0.875 s of second 2
    //
    // 0.125 s skip at loud starts: gate opens in ~1 ms but 0.125 s gives
    // margin for any spatial-stage transient at chunk boundaries.
    // 0.300 s skip at quiet start: clears the 50 ms hold + first 250 ms of
    // release (gain ≈ 0.082 at that point — still substantial if wanted, but
    // skipping it avoids the envelope ramp from corrupting the assertion).

    let loud1_msq_off = mean_sq(&out_off[6_000..42_000]);
    let loud1_mse = mse(&out_off[6_000..42_000], &out_on[6_000..42_000]);

    let quiet_msq_off = mean_sq(&out_off[62_400..96_000]);
    let quiet_mse = mse(&out_off[62_400..96_000], &out_on[62_400..96_000]);
    let quiet_msq_on = mean_sq(&out_on[62_400..96_000]);

    let loud2_msq_off = mean_sq(&out_off[102_000..138_000]);
    let loud2_mse = mse(&out_off[102_000..138_000], &out_on[102_000..138_000]);

    println!(
        "\n[test1 / vocal_gate]\n\
         loud1  off_msq={:.3e}  mse={:.3e}  ratio={:.6}\n\
         quiet  off_msq={:.3e}  mse={:.3e}  ratio={:.6}  on_msq={:.3e}\n\
         loud2  off_msq={:.3e}  mse={:.3e}  ratio={:.6}",
        loud1_msq_off,
        loud1_mse,
        loud1_mse / loud1_msq_off.max(1e-30),
        quiet_msq_off,
        quiet_mse,
        quiet_mse / quiet_msq_off.max(1e-30),
        quiet_msq_on,
        loud2_msq_off,
        loud2_mse,
        loud2_mse / loud2_msq_off.max(1e-30),
    );

    // (α) Loud 1: gate open → diff must be < 0.1% of off energy.
    assert!(
        loud1_mse < loud1_msq_off * 0.001,
        "Loud1: gate must be open (diff < 0.1% of off energy). ratio={:.6}",
        loud1_mse / loud1_msq_off.max(1e-30)
    );

    // (β) Quiet: gate closed → diff > 10% of off energy, AND on is quieter.
    assert!(
        quiet_mse > quiet_msq_off * 0.10,
        "Quiet: gate must close (diff > 10% of off energy) — is vocal gate wired in render_node? ratio={:.6}",
        quiet_mse / quiet_msq_off.max(1e-30)
    );
    assert!(
        quiet_msq_on < quiet_msq_off,
        "Quiet: restoration=on must be quieter (gate attenuates, not amplifies). \
         on_msq={:.3e}, off_msq={:.3e}",
        quiet_msq_on,
        quiet_msq_off,
    );

    // (γ) Loud 2: gate re-opened → diff must be < 0.1% of off energy.
    assert!(
        loud2_mse < loud2_msq_off * 0.001,
        "Loud2: gate must re-open (diff < 0.1% of off energy). ratio={:.6}",
        loud2_mse / loud2_msq_off.max(1e-30)
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 2 — glue bus LPF + pad attenuates the ambience stem
// ─────────────────────────────────────────────────────────────────────────────
//
// ISOLATION: mix_levels = { ambience: 1.0, rest: 0.0 }
//   The voice=0 removes the vocal gate from this test.
//   Only the ambience stem reaches the output.
//
// GLUE BUS (render_node.rs restoration path):
//   ambience → rbj_lowpass(6 kHz, Q=0.707) → ×GLUE_PAD_LINEAR (0.125_892_54 ≈ −18 dB)
//   The pad is unconditional — it applies in every chunk regardless of level.
//   No quiet/loud fixture structure is needed; 1 s of unmodified speech suffices.
//
// NMF TRAP: the ambience stem is nonzero only for signals with diffuse, stereo
//   content. A pure mono tone produces near-zero ambience. real_world_60s.wav has
//   genuine room ambience; this test must not be run with a synthetic fixture.
//
// THRESHOLD:
//   GLUE_PAD_LINEAR^2 ≈ 0.01586.
//   LPF at 6 kHz passes >99% of voice-range content → ratio ≈ 0.016 for passband.
//   Assert < 3% of off energy (conservative relative to theoretical 1.6%).

#[test]
fn restoration_glue_bus_pads_ambience() {
    let fixture = require_fixture();
    let fixture_str = fixture.to_str().unwrap();
    let n_frames = SR_US; // 1 s — unconditional pad, no need for quiet sections

    let interleaved = decode_first_n(fixture_str, n_frames);
    let scout_left: Vec<f32> = interleaved.iter().step_by(2).copied().collect();
    let scout_right: Vec<f32> = interleaved.iter().skip(1).step_by(2).copied().collect();

    let pcm_file = write_raw_pcm(&interleaved);

    let mix_ambience_only = MixLevels {
        voice: 0.0,
        drums: 0.0,
        bass: 0.0,
        harmonics: 0.0,
        ambience: 1.0,
    };

    let out_off = render_once(
        pcm_file.path(),
        &scout_left,
        &scout_right,
        n_frames,
        false, // restoration off — ambience passes at 1.0× gain
        false,
        Some(&mix_ambience_only),
        &[],
    );
    let out_on = render_once(
        pcm_file.path(),
        &scout_left,
        &scout_right,
        n_frames,
        true, // restoration on — ambience → LPF → ×0.1259
        false,
        Some(&mix_ambience_only),
        &[],
    );

    let off_msq = mean_sq(&out_off);
    let on_msq = mean_sq(&out_on);
    let ratio = on_msq / off_msq.max(1e-30);

    println!(
        "\n[test2 / glue_bus]\n\
         off_msq={:.3e}  on_msq={:.3e}  ratio={:.6}\n\
         GLUE_PAD_LINEAR^2 ≈ 0.01586  (theoretical ceiling for passband content)",
        off_msq, on_msq, ratio
    );

    // Sanity: ambience stem must be nonzero; if this fails the fixture lacks
    // stereo ambience content and cannot exercise the glue bus.
    assert!(
        off_msq > 1e-10,
        "Ambience stem is zero — does real_world_60s.wav have stereo ambience? off_msq={:.3e}",
        off_msq
    );

    // −18 dB pad reduces energy to ~1.6% of input (passband). Assert < 3%.
    assert!(
        on_msq < off_msq * 0.03,
        "Glue bus: expected on < 3% of off energy — is ambience LPF+pad wired in render_node? ratio={:.6}",
        ratio
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 3 — macro_router_enabled bypasses NMF: exact structural assertions
// ─────────────────────────────────────────────────────────────────────────────
//
// The bypass has an exact signature (two_pass.rs:817-829):
//   voice = raw_chunk (= OverlapChunk.signal = mono downmix (L+R)/2)
//   drums = bass = harmonics = ambience = vec![0.0; len]
//
// (b) DRUMS EXACTLY ZERO (primary assertion):
//   mix_levels = { voice: 0.0, drums: 1.0, rest: 0.0 }
//   bypass active:   mv = raw × 0.0 = 0; md = 0 × 1.0 = 0; all others = 0
//                    → FiveDotOneStage receives all-zero stems
//                    → IEEE 754: 0.0 × any_finite = 0.0 exactly
//                    → StereoRenderer output = 0.0 everywhere
//   bypass inactive: md = NMF_drums × 1.0 ≠ 0
//   This assertion is one line. Only correct wiring can satisfy it.
//
// (a) VOICE CORRELATES WITH RAW MONO INPUT (secondary assertion):
//   mix_levels = { voice: 1.0, rest: 0.0 }
//   bypass active:   voice = raw_chunk = (L+R)/2 → sp_l = (L+R)/2 × spatial_gain
//                    → linear relationship with raw_mono → Pearson ≈ 1.0
//   bypass inactive: voice = NMF_voice_stem (soft decomposition, loses energy
//                    to other stems) → |Pearson(sp_l, raw_mono)| < 0.9999
//
// BOUNDARY: should_bypass_nmf fires for Speech boundaries with
//   avg_confidence ≥ 0.4. Use 0.5 for margin.

#[test]
fn macro_router_bypasses_nmf_exact() {
    let fixture = require_fixture();
    let fixture_str = fixture.to_str().unwrap();
    let n_frames = SR_US; // 1 s

    let interleaved = decode_first_n(fixture_str, n_frames);
    let scout_left: Vec<f32> = interleaved.iter().step_by(2).copied().collect();
    let scout_right: Vec<f32> = interleaved.iter().skip(1).step_by(2).copied().collect();
    // raw_mono = (L+R)/2 — this is what OverlapChunk.signal contains, and what
    // the bypass routes to the voice stem.
    let raw_mono: Vec<f32> = scout_left
        .iter()
        .zip(scout_right.iter())
        .map(|(l, r)| (l + r) * 0.5)
        .collect();

    let pcm_file = write_raw_pcm(&interleaved);

    let boundaries = vec![SegmentBoundary {
        start_sec: 0.0,
        end_sec: 1.0,
        segment_type: SegmentType::Speech,
        avg_leaning: 0.5,
        avg_confidence: 0.5, // ≥ 0.4 → bypass fires
    }];

    // ── (b) Drums-only: bypass must produce exactly zero output everywhere ───

    let mix_drums_only = MixLevels {
        voice: 0.0,
        drums: 1.0,
        bass: 0.0,
        harmonics: 0.0,
        ambience: 0.0,
    };

    let out_bypass_drums = render_once(
        pcm_file.path(),
        &scout_left,
        &scout_right,
        n_frames,
        false,
        true, // macro_router ON — bypass fires, drums stem zeroed
        Some(&mix_drums_only),
        &boundaries,
    );
    let out_nmf_drums = render_once(
        pcm_file.path(),
        &scout_left,
        &scout_right,
        n_frames,
        false,
        false, // macro_router OFF — NMF produces nonzero drums stem
        Some(&mix_drums_only),
        &boundaries,
    );

    let bypass_drums_msq = mean_sq(&out_bypass_drums);
    let nmf_drums_msq = mean_sq(&out_nmf_drums);

    println!(
        "\n[test3 / macro_router drums-zero]\n\
         bypass drums msq = {:.3e}  (must be 0.0)\n\
         nmf    drums msq = {:.3e}  (must be > 0)",
        bypass_drums_msq, nmf_drums_msq
    );

    // Every sample must be exactly 0.0.
    // IEEE 754: 0.0_f32 × any finite = 0.0_f32. FiveDotOneStage and
    // StereoRenderer apply only finite spatial weights → zero in, zero out.
    let first_nonzero = out_bypass_drums
        .iter()
        .enumerate()
        .find(|(_, &s)| s != 0.0);
    assert!(
        first_nonzero.is_none(),
        "macro_router bypass: drums stem must be exactly 0.0 everywhere. \
         First nonzero sample: index={}, value={:?} — is macro_router_enabled \
         reaching should_bypass_nmf?",
        first_nonzero.map_or(0, |(i, _)| i),
        first_nonzero.map(|(_, &v)| v)
    );

    // Sanity: NMF path must produce nonzero drums (otherwise the assertion above
    // would pass vacuously for any input).
    assert!(
        nmf_drums_msq > 1e-10,
        "NMF drums stem is zero — fixture may lack drum-like content. nmf_drums_msq={:.3e}",
        nmf_drums_msq
    );

    // ── (a) Voice-only: bypass output correlates with raw mono input ─────────

    let mix_voice_only = MixLevels {
        voice: 1.0,
        drums: 0.0,
        bass: 0.0,
        harmonics: 0.0,
        ambience: 0.0,
    };

    let out_bypass_voice = render_once(
        pcm_file.path(),
        &scout_left,
        &scout_right,
        n_frames,
        false,
        true, // macro_router ON — voice = raw_chunk = (L+R)/2
        Some(&mix_voice_only),
        &boundaries,
    );
    let out_nmf_voice = render_once(
        pcm_file.path(),
        &scout_left,
        &scout_right,
        n_frames,
        false,
        false, // macro_router OFF — voice = NMF_voice_stem
        Some(&mix_voice_only),
        &boundaries,
    );

    let n = out_bypass_voice.len().min(raw_mono.len());
    let corr_bypass = pearson(&out_bypass_voice[..n], &raw_mono[..n]);
    let corr_nmf = pearson(&out_nmf_voice[..n], &raw_mono[..n]);

    println!(
        "\n[test3 / macro_router voice-correlation]\n\
         bypass  Pearson(out, raw_mono) = {:.6}\n\
         nmf     Pearson(out, raw_mono) = {:.6}",
        corr_bypass, corr_nmf
    );

    // bypass: voice = raw_mono × constant_spatial_gain → Pearson = 1.0 exactly.
    // Allow for floating-point accumulation in the spatial stage.
    assert!(
        corr_bypass.abs() > 0.9999,
        "macro_router bypass: output must correlate with raw mono input at > 0.9999 \
         (bypass sets voice = raw_chunk = (L+R)/2). Got {:.6}",
        corr_bypass
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// TEST 4 — LTASS correction (shape reserved, deferred)
// ─────────────────────────────────────────────────────────────────────────────
//
// DEFERRED: LTASS operates as a topology override at the dsp_pipeline level,
// not as a RenderSettings flag. Write alongside the others once the wiring lands.
//
// Shape when ready:
//   1. Build graph with DspConfig { eq: EqConfig { zone_bands: vec![] } } (off)
//      and with zone_bands populated (on).
//   2. Render a white-noise fixture through both topologies.
//      White noise: broadband → stable per-band RMS; pure tones alias to one bin.
//   3. Compute 8-band RMS at the LTASS centre frequencies in both outputs.
//   4. Assert: on-output RMS matches the compensated gains from
//      apply_topology_overrides within 0.10 dB per band.
//
// #[test]
// fn ltass_correction_changes_spectral_balance() { todo!() }

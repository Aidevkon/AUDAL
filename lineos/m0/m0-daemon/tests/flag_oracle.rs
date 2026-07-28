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

use aether::semantic::zone::EqSource;
use integration::config::{
    DspConfig, DspDynamicsConfig, DspEqConfig, DspSatConfig, DspStereoConfig, ZoneBand,
};
use lineos_corpus::scout::{SegmentBoundary, SegmentType};
use lineos_types::MasteringIntent;
use m0d::domain::nodes::render_node::{run, RenderInputs, RenderSettings};
use m0d::dsp::DspAdapter;
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
    let bytes: &[u8] =
        unsafe { std::slice::from_raw_parts(samples.as_ptr() as *const u8, samples.len() * 4) };
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
#[allow(clippy::too_many_arguments)]
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
    let first_nonzero = out_bypass_drums.iter().enumerate().find(|(_, &s)| s != 0.0);
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
// TEST 4 — LTASS correction changes spectral balance
// ─────────────────────────────────────────────────────────────────────────────
//
// Scaffolding differs from tests 1–3. Those call render_node::run, which drives
// TwoPassEngine. LTASS operates as a DspConfig topology override applied before
// the graph is built, so this test calls DspAdapter::build_graph_only directly
// and drives graph.process_block chunk-by-chunk — the same path the Episode
// streaming render takes in master().
//
// Fixture: 30 s × 48 kHz of deterministic white noise from a fixed-seed Knuth
// LCG (no rand dependency). White noise gives stable energy in every LTASS band.
// A pure tone would alias to one DFT bin; real speech leaves many bands
// near-empty — both make the per-band ratio noisy. White noise is the only safe
// fixture here.
//
// Measurement: 2nd-order RBJ bandpass filter (constant 0 dB peak gain, Q = 2,
// ≈ half-octave bandwidth) applied at each LTASS centre frequency. Mean-square
// is accumulated over the signal minus a 0.5 s lead-in to flush filter transients.
// The ratio 10·log10(on_msq / off_msq) is the delivered spectral change per band.
//
// NOTE: this is NOT the transfer function evaluated at the centre frequency. The
// Q = 2 BPF averages gain over a half-octave band, so the measured ratio will be
// systematically lower in magnitude than the expected composite. Tolerances are
// set after seeing the measured deviations from the first run (below).

// ─── Test 4 helpers ──────────────────────────────────────────────────────────

/// 30 s × 48 kHz deterministic white noise.
/// Knuth's 64-bit multiplicative LCG (same multiplier as pcg64_fast).
/// Not rand — the seed and sequence are fixed so the numbers repeat across runs.
fn lcg_noise(n: usize) -> Vec<f32> {
    let mut state: u64 = 0xDEAD_BEEF_CAFE_1234;
    (0..n)
        .map(|_| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            // High 32 bits interpreted as signed i32, normalised to [-1, 1)
            (state >> 32) as i32 as f32 / i32::MAX as f32
        })
        .collect()
}

/// Hann-windowed DTFT power at exact frequency `freq_hz`.
///
/// Returns |X(f)|² where X(f) = Σ w[k]·x[k]·e^{−j·2π·f·k/fs},
/// w[k] = 0.5·(1 − cos(2πk/(N−1)))  (Hann window).
///
/// Because both on and off signals derive from the SAME noise buffer processed
/// through two linear systems:
///   X_on(f) = H_on(f) · X_noise(f)
///   X_off(f) = H_off(f) · X_noise(f)
/// The ratio |X_on|² / |X_off|² = |H_on/H_off|² regardless of the noise
/// realization — no averaging frames needed. The Hann window further reduces
/// spectral leakage. Both window and noise cancel in the ratio.
///
/// Uses incremental phasor rotation for both the Hann window and the DTFT
/// basis — O(N) time with no per-sample trig calls.
fn hann_dtft_power(signal: &[f32], freq_hz: f32, sample_rate: f32) -> f64 {
    let n = signal.len();
    if n < 2 {
        return 0.0;
    }
    let n_f = (n - 1) as f64;

    // Hann phasor: hre + j·him = e^{j·w_h·k}, w_h = 2π/(N−1)
    // hann[k] = 0.5·(1 − hre)  =  0.5·(1 − cos(2πk/(N−1)))
    let w_h = 2.0 * std::f64::consts::PI / n_f;
    let (ch, sh) = (w_h.cos(), w_h.sin());
    let mut hre = 1.0_f64; // cos(0)
    let mut him = 0.0_f64; // sin(0)

    // DTFT phasor: (p + j·q) = e^{−j·w·k}, w = 2π·f/fs
    // Re(X) = Σ w[k]·x[k]·p,  Im(X) = Σ w[k]·x[k]·q
    let w = 2.0 * std::f64::consts::PI * freq_hz as f64 / sample_rate as f64;
    let (cw, sw) = (w.cos(), w.sin());
    let mut p = 1.0_f64; // Re(e^{−j·w·0}) = 1
    let mut q = 0.0_f64; // Im(e^{−j·w·0}) = 0  (= −sin(0))

    let mut sum_re = 0.0_f64;
    let mut sum_im = 0.0_f64;

    for &x in signal.iter() {
        let hann = 0.5 * (1.0 - hre);
        let xw = x as f64 * hann;
        sum_re += xw * p;
        sum_im += xw * q;

        // Advance Hann phasor by e^{j·w_h}: (hre + j·him)·(ch + j·sh)
        let new_hre = hre * ch - him * sh;
        let new_him = hre * sh + him * ch;
        hre = new_hre;
        him = new_him;

        // Advance DTFT phasor by e^{−j·w}: (p + j·q)·(cw − j·sw)
        //   new_p = p·cw + q·sw
        //   new_q = −p·sw + q·cw
        let new_p = p * cw + q * sw;
        let new_q = -p * sw + q * cw;
        p = new_p;
        q = new_q;
    }

    sum_re * sum_re + sum_im * sum_im
}

/// Build a minimal DspConfig that carries only the given zone_bands.
/// All other parameters are neutral (no extra EQ, no saturation, no ambience).
/// `ambience: None` causes apply_topology_overrides to zero the reverb mix and
/// decorrelation, so the chain is: LtassCorrection → (dry LufsNormalization EQ).
fn make_ltass_config(zone_bands: Vec<ZoneBand>) -> DspConfig {
    DspConfig {
        eq: DspEqConfig {
            low_shelf_gain_db: 0.0,
            low_shelf_freq_hz: 100.0,
            high_shelf_gain_db: 0.0,
            high_shelf_freq_hz: 10_000.0,
            zone_bands,
        },
        dynamics: DspDynamicsConfig {
            comp_threshold_db: -18.0,
            comp_ratio: 4.0,
            comp_attack_ms: 10.0,
            comp_release_ms: 100.0,
        },
        sat: DspSatConfig {
            drive: 0.0,
            mix: 0.0,
        },
        stereo: DspStereoConfig { width: 1.0 },
        ambience: None,
        persona_id: "test".into(),
        chaos_seed: 0,
        instrument_deltas: Default::default(),
    }
}

/// Build the DSP graph for `config`, feed `noise` through it block-by-block,
/// and return the processed left-channel output.
///
/// Unlike tests 1–3 (render_node::run → TwoPassEngine), we call
/// DspAdapter::build_graph_only directly because LTASS is a topology override
/// applied at graph-build time, not a TwoPassEngine flag. The same noise is used
/// as the scout sample (left = right = mono) and as the audio to process.
///
/// Intent: MasteringIntent::podcast (target −16 LUFS). White noise at amplitude
/// ≈ 0.58 RMS triggers LufsTooLoud but that condition adds no extra flavour in
/// the router, so the topology remains LtassCorrection + LufsNormalization.
fn run_ltass_graph(noise: &[f32], config: &DspConfig) -> Vec<f32> {
    const BLOCK: usize = 512;
    let n = noise.len();
    let intent = MasteringIntent::podcast();

    let mut graph = DspAdapter::build_graph_only(
        &intent,
        noise, // scout sample (same signal for L and R)
        noise,
        SR,
        &[0.2, 0.2, 0.2, 0.2, 0.2], // uniform stem ratios
        Some(config),
        BLOCK,
    )
    .expect("DspAdapter::build_graph_only failed");

    // Initialise output buffers with the input noise, then process in-place —
    // the same pattern master() uses (dsp/mod.rs::master()).
    let mut out_l = noise.to_vec();
    let mut out_r = noise.to_vec();

    let mut f = 0;
    while f < n {
        let e = (f + BLOCK).min(n);
        let b_len = e - f;
        if b_len < BLOCK {
            // Pad the final short block with zeros (same as master()).
            let mut pad_l = vec![0.0_f32; BLOCK];
            let mut pad_r = vec![0.0_f32; BLOCK];
            pad_l[..b_len].copy_from_slice(&noise[f..e]);
            pad_r[..b_len].copy_from_slice(&noise[f..e]);
            graph.process_block(&mut pad_l, &mut pad_r);
            out_l[f..e].copy_from_slice(&pad_l[..b_len]);
            out_r[f..e].copy_from_slice(&pad_r[..b_len]);
        } else {
            graph.process_block(&mut out_l[f..e], &mut out_r[f..e]);
        }
        f += b_len;
    }

    out_l // L channel; the LTASS correction is applied equally to both
}

// ─── Test 4 ──────────────────────────────────────────────────────────────────

#[test]
fn ltass_correction_changes_spectral_balance() {
    // 30 s × 48 kHz = 1 440 000 samples.
    // The long signal keeps per-bin variance low for the DTFT measurement.
    const N: usize = 30 * SR_US;

    let noise = lcg_noise(N);

    // ── off: empty zone_bands → ReferenceProfileResolved not pushed
    //          → LtassCorrection flavour not inserted into the topology
    //          → all ltass_band_{0..7} gains remain at 0.0 dB
    let off = run_ltass_graph(&noise, &make_ltass_config(vec![]));

    // ── on: eight measured corrections from the real podcast clip.
    //   q = 0.707 and EqSource::Reference exactly as the aether-bridge resolver
    //   sets them (aether-bridge/src/lib.rs:194–198, REF_Q = 0.707).
    //   ZoneBand fields: center_hz, gain_db, q, source.
    let zone_bands_on = [
        (50.0_f32, 2.04_f32),
        (150.0, -3.02),
        (350.0, 1.66),
        (750.0, 1.04),
        (1500.0, 0.38),
        (3000.0, -2.10),
        (6000.0, -6.00),
        (12000.0, -1.14),
    ]
    .iter()
    .map(|&(cf, gain)| ZoneBand {
        center_hz: cf,
        gain_db: gain,
        q: 0.707,
        source: EqSource::Reference,
    })
    .collect::<Vec<_>>();
    let on = run_ltass_graph(&noise, &make_ltass_config(zone_bands_on));

    // Expected: composite gain the A_INV-compensated chain DELIVERS at each
    // centre frequency (not the raw targets). Computed by compute_ainv.py,
    // forward-verification section. Q = 1.0, fs = 48 kHz.
    const LTASS_CFS: [f32; 8] = [50.0, 150.0, 350.0, 750.0, 1500.0, 3000.0, 6000.0, 12000.0];
    const EXPECTED_DB: [f32; 8] = [2.031, -3.016, 1.648, 1.032, 0.358, -2.137, -6.000, -1.181];

    // Measure transfer function at each LTASS centre frequency using
    // Hann-windowed DTFT. Since on and off come from the same noise buffer,
    // the ratio |X_on(f)|² / |X_off(f)|² equals |H_LTASS(f)|² exactly
    // (the noise DFT cancels). Result: 10·log10(p_on/p_off) = 20·log10(|H(f)|),
    // directly comparable to the composite expected values.
    // Bins averaged: 1 (exact frequency; Hann window suppresses leakage).
    println!(
        "\n[ltass]  {:>8}  {:>14}  {:>14}  {:>10}  {:>10}  {:>8}",
        "freq_hz", "p_off", "p_on", "meas_dB", "exp_dB", "dev_dB"
    );
    let mut measured = [0.0_f32; 8];
    for i in 0..8 {
        let p_off = hann_dtft_power(&off, LTASS_CFS[i], SR as f32);
        let p_on = hann_dtft_power(&on, LTASS_CFS[i], SR as f32);
        // 10·log10(|X_on|²/|X_off|²) = 20·log10(|H_LTASS(f)|)
        let db = (10.0 * (p_on / p_off.max(1e-300)).log10()) as f32;
        measured[i] = db;
        println!(
            "[ltass]  {:>8.0}  {:>14.4e}  {:>14.4e}  {:>+10.3}  {:>+10.3}  {:>+8.3}",
            LTASS_CFS[i],
            p_off,
            p_on,
            db,
            EXPECTED_DB[i],
            db - EXPECTED_DB[i],
        );
    }

    // ── Per-band oracle (tolerance from DTFT measurements, 2026-07-28)
    //
    // Observed DTFT deviations from expected composite with correct wiring:
    //   50 Hz: +0.024   150 Hz: +0.002   350 Hz: 0.000   750 Hz: 0.000
    // 1500 Hz:  0.000  3000 Hz:  0.000  6000 Hz: 0.000  12000 Hz: 0.000
    //
    // Tolerance: 0.10 dB — four times the worst observed deviation at 50 Hz,
    // where FFT resolution is lowest. The no-op (all bands 0.000 dB) deviates
    // by |0.000 − expected[i]| ≥ 0.358 dB on every band, so it fails all
    // eight assertions. Band 4 (1500 Hz, +0.358 dB) is now measurable under
    // DTFT and is included.
    const TOL: f32 = 0.10;
    for i in 0..8 {
        let dev = (measured[i] - EXPECTED_DB[i]).abs();
        assert!(
            dev < TOL,
            "band_{i} ({} Hz): measured {:+.4} dB, expected {:+.4} dB, \
             deviation {:+.4} dB — exceeds {:.2} dB tolerance. \
             Is the LTASS write loop firing?",
            LTASS_CFS[i] as u32,
            measured[i],
            EXPECTED_DB[i],
            measured[i] - EXPECTED_DB[i],
            TOL,
        );
    }
}

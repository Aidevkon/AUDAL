//! TwoPassEngine — Constitutional Memory-Aware Streaming DSP
//! Authority: v3_memory_aware_streaming.md
//!
//! INV-ST-1: W computed ONCE per run (Pass 1)
//! INV-ST-2: W is READ-ONLY during Pass 2
//! INV-ST-3: peak RAM ≤ 50MB for any file length
//! INV-ST-4: OLA overlap = FFT_SIZE / 2
//! INV-ST-5: chunk size = 65536 samples
//! INV-ST-6: EBU R128 output identical to full-load processing
//! INV-AB-1: deterministic — same input → same output

use crate::spatial::channel_assign::StemChannelAssignments;
use crate::spatial::SpatialPreAnalysis;
use crate::stft::hpss::HpssStreamContext;
use crate::stft::nmf::{NmfEngine, N_COMPONENTS};
use crate::stft::stem_renderer::FiveStems;
use crate::stft::{StftEngine, StreamingStftEncoder, FFT_SIZE, HOP_SIZE, N_BINS};
use lineos_corpus::mfcc::MfccAnalyzer;
use lineos_corpus::scout::{SegmentBoundary, SegmentType};
use lineos_types::StemFeatures;

/// Constitutional chunk size — 65536 samples = ~1.37s at 48kHz
pub const CHUNK_FRAMES: usize = 65536;

/// Πόσο αφαιρείται από τα 4 NMFD masks στα cells
/// που το HPSS δίνει στα drums (time-domain,
/// drums_weights). 0.0 = σημερινή συμπεριφορά.
///
/// ΜΕΤΡΗΜΕΝΟ (MASKH-PROBE): με α=0, percussive
/// ενέργεια μπαίνει δύο φορές — drums + 21% voice
/// + 27% amb στα βέβαια percussive cells.
/// Drum de-dup: πόσο αφαιρείται από τα 4 NMFD masks στα cells
/// που το HPSS δίνει στα drums (time-domain, drums_weights).
///
/// ΜΕΤΡΗΜΕΝΟ (MASKH-PROBE 2026-08-15): με α=0, percussive
/// ενέργεια μπαίνει ΔΥΟ φορές στο mix — μία στα drums, μία
/// σκορπισμένη στα masks (voice 21%, amb 27% στα cells με
/// mask_p>0.5). Το double-counting φούσκωνε τα transient
/// peaks → περισσότερη δουλειά στον limiter clamp → χαμένη
/// στάθμη, και έγερνε το ratio (+0.0036 έναντι πηγαίου).
/// Με α=0.5: ratio 1.0394 ≈ decoder 1.0391, LUFS +0.7 dB
/// πιο κοντά στον στόχο. Sweep 0→1.0 ομαλό, monotonic.
///
/// ΟΧΙ renormalization, ΟΧΙ αναδιανομή της «χαμένης»
/// ενέργειας στα drums: ΔΕΝ υπάρχει drums φασματικό mask.
/// Τα drums είναι ΕΚΤΟΣ του partition ΣΚΟΠΙΜΑ (time-domain
/// multiply, η μόνη ανέπαφη φάση — δες S1 commit f20cf28).
/// Η ενέργεια που αφαιρείται εδώ υπάρχει ΗΔΗ εκεί· η
/// αφαίρεση ΕΙΝΑΙ η διόρθωση, όχι τρύπα.
pub const DRUM_DEDUP_ALPHA: f32 = 0.5;


/// Downsample ratio for Pass 1 Scout proxy (~11kHz mono)
pub const SCOUT_DOWNSAMPLE: usize = 4;

const COLLISION_DRUMS_TRANSIENT_THRESHOLD: f32 = 0.25;
const COLLISION_BASS_RMS_THRESHOLD_DB: f32 = -40.0;
const COLLISION_DUCKING_GAIN: f32 = 0.707;
const COLLISION_SMOOTHING_ALPHA: f32 = 0.005;

/// Ο Φ2 (PCEN neural VAD) οδηγεί το ducking αντί για τον
/// DSP classifier. Μετρημένο: median FP στα beds 0.019 vs
/// 0.727 (Phi-1.e, 8caa514). false μέχρι να περάσει τα
/// gates με ήχο.
const USE_NEURAL_VAD: bool = true;

/// Per-stem MFCC fingerprints computed during scout().
/// Captures the timbral identity of each stem BEFORE render.
/// Used by AutoTuningController to set adaptive ducking_gain.
/// INV-AB-1: computed deterministically from scout proxy stems.
#[derive(Debug, Clone)]
pub struct StemMfccs {
    pub voice: [f32; 13],
    pub drums: [f32; 13],
    pub bass: [f32; 13],
    pub harmonics: [f32; 13],
    pub ambience: [f32; 13],
}

impl StemMfccs {
    /// Zero MFCCs — used before computation or as fallback.
    pub fn zero() -> Self {
        Self {
            voice: [0.0f32; 13],
            drums: [0.0f32; 13],
            bass: [0.0f32; 13],
            harmonics: [0.0f32; 13],
            ambience: [0.0f32; 13],
        }
    }

    /// L2 distance between two MFCC vectors.
    /// Used to measure timbral similarity/difference.
    pub fn distance(a: &[f32; 13], b: &[f32; 13]) -> f32 {
        libm::sqrtf(
            a.iter()
                .zip(b.iter())
                .map(|(x, y)| (x - y).powi(2))
                .sum::<f32>(),
        )
    }

    /// Bass vs Drums timbral distance.
    /// High distance = different timbre = low collision risk.
    /// Low distance = similar timbre = high collision risk.
    pub fn bass_drums_distance(&self) -> f32 {
        Self::distance(&self.bass, &self.drums)
    }
}

/// Ένα stem με τα δύο του κανάλια.
///
/// Τα δύο πάνε ΜΑΖΙ εξ ορισμού — δεν μπορεί κάποιος
/// να γεμίσει το ένα και να ξεχάσει το άλλο.
///
/// ΠΡΟΣΤΕΘΗΚΕ ΣΤΟ S1, ΧΡΗΣΙΜΟΠΟΙΕΙΤΑΙ ΣΤΟ S2.
#[derive(Debug, Clone, Default)]
pub struct StereoStem {
    pub l: Vec<f32>,
    pub r: Vec<f32>,
}

impl StereoStem {
    /// ΠΡΟΣΩΡΙΝΟ ΓΕΦΥΡΩΜΑ — το mixing δεν ξέρει
    /// ακόμα stereo.
    ///
    /// ΦΕΥΓΕΙ ΣΤΟ S3, όταν το render_chunk διαβάζει
    /// l και r ξεχωριστά.
    pub fn mono(&self) -> Vec<f32> {
        self.l
            .iter()
            .zip(self.r.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect()
    }
}

/// A single chunk of 5 stems — chunk-sized slices only.
/// Never holds full-file data. Passed to process_chunks callback.
pub struct FiveStemsChunk {
    pub voice: StereoStem,
    pub drums: StereoStem,
    pub bass: StereoStem,
    pub harmonics: StereoStem,
    pub ambience: StereoStem,
}

/// Result of Pass 1 — all static parameters locked.
/// Pass 2 uses these blindly — no recomputation.
/// INV-ST-1: W is read-only after scout() returns.
/// Note: If `tensor_w` is empty, `tau` is 0, and indices are `usize::MAX`,
/// scout ran without NMFD fit. Using the true branch with these is a caller bug and will panic loudly.
#[derive(Clone)]
pub struct ScoutResult {
    /// NMF basis matrix — READ ONLY (flat w for Pass 2 legacy path)
    pub w: Vec<f32>,
    /// NMFD full tensor W (bins * k * tau)
    pub tensor_w: Vec<f32>,
    /// NMFD tau
    pub tau: usize,
    /// Rough RMS from proxy
    pub proxy_rms: f32,
    /// Semantic indices — computed from W
    pub voice_idx: usize,
    pub bass_idx: usize,
    pub harmonics_idx: usize,
    pub ambience_idx: usize,
    /// NMFD K=8 semantic map. Voice = sum of frozen slots 0..NMFD_FROZEN_K (speech prior, δεν χρειάζεται index — fixed group). Drums: HPSS-owned, no NMFD index by design (v2 doctrine).
    pub nmfd_bass_idx: usize,
    pub nmfd_harmonics_idx: usize,
    pub nmfd_ambience_idx: usize,
    /// Locked spatial assignments from proxy stems
    pub assignments: StemChannelAssignments,
    /// Pre-computed firewall scales from proxy energy
    pub rear_scale: f32,
    pub lfe_scale: f32,
    /// Spatial pre-analysis from proxy
    pub spatial_pre: SpatialPreAnalysis,
    /// Per-stem MFCC fingerprints from scout proxy analysis.
    /// StemMfccs::zero() until M-P2 populates them.
    pub stem_mfccs: StemMfccs,
    /// NMF stem features from proxy
    pub features: StemFeatures,
    /// 30s downsampled mono proxy stems (F-044: corpus per-stem
    /// learning). Bounded — proxy scale, never full-file.
    pub proxy_voice: Vec<f32>,
    pub proxy_drums: Vec<f32>,
    pub proxy_bass: Vec<f32>,
    pub proxy_harmonics: Vec<f32>,
    pub proxy_ambience: Vec<f32>,
}

/// Streaming error
#[derive(Debug)]
pub enum StreamError {
    Io(String),
    Empty,
}

impl core::fmt::Display for StreamError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StreamError::Io(e) => write!(f, "StreamError::Io({e})"),
            StreamError::Empty => write!(f, "StreamError::Empty"),
        }
    }
}

#[derive(Debug, Clone, Copy, Default)]
pub struct BandSpatialMetrics {
    pub pan_mean: f32,
    pub pan_width: f32,
}

/// Metadata returned after render — audio on disk, no audio Vec.
pub struct RenderMetadata {
    pub frames_written: usize,
    pub voice_transient_density: f32,
    pub drums_transient_density: f32,
    pub spatial: [BandSpatialMetrics; 5],
}

pub(crate) struct ParallelChunkOut {
    pub stems: FiveStemsChunk,
    pub voice_transient: f32,
    pub drums_transient: f32,
    pub chunk_len: usize,
    pub spatial_sums: [(f32, f32, f32); 5], // [(pan_num, width_num, den); 5]
}

pub(crate) struct SingleChunkData<'a> {
    pub padded_chunk: &'a [f32],
    pub padded_left: &'a [f32],
    pub padded_right: &'a [f32],
    pub core_chunk: &'a [f32],
    /// ΠΡΟΣΤΕΘΗΚΕ ΣΤΟ S1. Δεν διαβάζεται ακόμα.
    pub core_left: &'a [f32],
    pub core_right: &'a [f32],
    pub pad_frames: usize,
    pub use_nmfd: bool,
}

pub(crate) fn process_single_chunk(
    nmf: &NmfEngine,
    scout: &ScoutResult,
    data: SingleChunkData<'_>,
) -> ParallelChunkOut {
    let mut stft_ctx = StreamingStftEncoder::new();
    let mut hpss_ctx = HpssStreamContext::new();
    let mut stft_l = StreamingStftEncoder::new();
    let mut stft_r = StreamingStftEncoder::new();

    let mut chunk_frames_cplx = stft_ctx.feed_chunk(data.padded_chunk);
    chunk_frames_cplx.extend(stft_ctx.finish());
    let chunk_frames: Vec<Vec<f32>> = chunk_frames_cplx
        .into_iter()
        .map(|f| {
            f.into_iter()
                .map(|c| libm::sqrtf(c.re * c.re + c.im * c.im))
                .collect()
        })
        .collect();

    let mut core_frames_l_cplx = stft_l.feed_chunk(data.padded_left);
    core_frames_l_cplx.extend(stft_l.finish());
    // ΤΑ COMPLEX ΤΩΝ L/R ΜΕΝΟΥΝ ΔΙΑΘΕΣΙΜΑ.
    //
    // Τα magnitudes χρησιμεύουν για το pan_mean. Η ΦΑΣΗ
    // χρειάζεται για να εφαρμοστούν τα masks ξεχωριστά
    // σε κάθε κανάλι (S2). Το STFT γίνεται ήδη — μόνο το
    // αποτέλεσμα πετιόταν.
    let core_frames_l: Vec<Vec<f32>> = core_frames_l_cplx
        .iter()
        .map(|f| {
            f.iter()
                .map(|c| libm::sqrtf(c.re * c.re + c.im * c.im))
                .collect()
        })
        .collect();

    let mut core_frames_r_cplx = stft_r.feed_chunk(data.padded_right);
    core_frames_r_cplx.extend(stft_r.finish());
    let core_frames_r: Vec<Vec<f32>> = core_frames_r_cplx
        .iter()
        .map(|f| {
            f.iter()
                .map(|c| libm::sqrtf(c.re * c.re + c.im * c.im))
                .collect()
        })
        .collect();

    let n_frames = chunk_frames.len();
    if n_frames == 0 {
        return ParallelChunkOut {
            stems: FiveStemsChunk {
                voice: StereoStem::default(),
                drums: StereoStem::default(),
                bass: StereoStem::default(),
                harmonics: StereoStem::default(),
                ambience: StereoStem::default(),
            },
            voice_transient: 0.0,
            drums_transient: 0.0,
            chunk_len: 0,
            spatial_sums: [(0.0, 0.0, 0.0); 5],
        };
    }

    let (_mask_h, mask_p) = hpss_ctx.process_chunk(&chunk_frames);
    // h_chunk is used later in the chunk (around line 382) for transient density
    let h_chunk = nmf.transform(&scout.w, &chunk_frames);

    let (voice_mask, bass_mask, harm_mask, amb_mask) = if data.use_nmfd {
        // ΤΟ nmfd_iter ΕΔΩ ΕΙΝΑΙ ΣΚΟΠΙΜΑ ΔΙΑΦΟΡΕΤΙΚΟ ΑΠΟ ΤΟ SCOUT.
        // Το scout τρέχει nmfd_num_iter = 30 και λύνει ΚΑΙ ΤΑ ΔΥΟ,
        // W και H. Εδώ το W είναι κλειδωμένο (scout.tensor_w) και
        // λύνουμε ΜΟΝΟ για το H — πολύ ευκολότερο πρόβλημα.
        //
        // ΜΕΤΡΗΜΕΝΟ 2026-08-13: με 30 αντί για 12 το fold-down ratio
        // πάει από 0.7961 σε 0.7955, δηλαδή τέταρτο δεκαδικό, ενώ
        // το render_node διπλασιάζεται από 621 ms σε 1229 ms.
        // Τα 12 αρκούν.
        //
        // ΗΤΑΝ: «These hardcoded values mirror scout fit + W5.b
        // harness» — έπαψε να ισχύει όταν το scout πήγε στις 30.
        let nmfd_k = scout.tensor_w.len() / (128 * 8);
        let nmfd_iter = 12;
        let init_val = 0.1_f32;

        let mut c_v = vec![0.0_f32; 128 * n_frames];
        for f in 0..n_frames {
            let mel_frame = crate::analysis::mel_128::fold_to_mel(
                chunk_frames[f].as_slice().try_into().unwrap(),
            );
            for b in 0..128 {
                c_v[b * n_frames + f] = mel_frame[b];
            }
        }

        let init_h = vec![init_val; nmfd_k * n_frames];
        let (nmfd_h, _) = crate::stft::nmfd::nmfd_f32_h_only(
            &c_v,
            &scout.tensor_w,
            &init_h,
            128,
            nmfd_k,
            n_frames,
            scout.tau,
            nmfd_iter,
        );

        // W16: nmf comes from NmfEngine::default() (n_components=5)
        // but tensor_w/h_chunk are structured for k=8. The NMFD mask
        // functions use self.n_components as STRIDE for tensor_w
        // indexing — with k=5 they read wrong addresses.
        // Measured: mask sum max 16.59 instead of ≈1.0,
        // bass stem 5.3× and ambience 6.1× above input,
        // −8.7dB in final LUFS (−25.13 vs −16.42).
        let nmfd_engine = crate::stft::nmf::NmfEngine::new(nmfd_k);

        (
            nmfd_engine.nmfd_group_mask_chunk(
                &[0, 1, 2, 3],
                &nmfd_h,
                &scout.tensor_w,
                n_frames,
                N_BINS,
                scout.tau,
            ),
            nmfd_engine.nmfd_component_mask_chunk(
                scout.nmfd_bass_idx,
                &nmfd_h,
                &scout.tensor_w,
                n_frames,
                N_BINS,
                scout.tau,
            ),
            nmfd_engine.nmfd_component_mask_chunk(
                scout.nmfd_harmonics_idx,
                &nmfd_h,
                &scout.tensor_w,
                n_frames,
                N_BINS,
                scout.tau,
            ),
            nmfd_engine.nmfd_component_mask_chunk(
                scout.nmfd_ambience_idx,
                &nmfd_h,
                &scout.tensor_w,
                n_frames,
                N_BINS,
                scout.tau,
            ),
        )
    } else {
        (
            nmf.component_mask_chunk(scout.voice_idx, &h_chunk, n_frames, N_BINS),
            nmf.component_mask_chunk(scout.bass_idx, &h_chunk, n_frames, N_BINS),
            nmf.component_mask_chunk(scout.harmonics_idx, &h_chunk, n_frames, N_BINS),
            nmf.component_mask_chunk(scout.ambience_idx, &h_chunk, n_frames, N_BINS),
        )
    };

    let core_n_frames = n_frames.saturating_sub(data.pad_frames);
    let core_voice_mask = if data.pad_frames < voice_mask.len() {
        &voice_mask[data.pad_frames..]
    } else {
        &[]
    };
    let core_bass_mask = if data.pad_frames < bass_mask.len() {
        &bass_mask[data.pad_frames..]
    } else {
        &[]
    };
    let core_harm_mask = if data.pad_frames < harm_mask.len() {
        &harm_mask[data.pad_frames..]
    } else {
        &[]
    };
    let core_amb_mask = if data.pad_frames < amb_mask.len() {
        &amb_mask[data.pad_frames..]
    } else {
        &[]
    };
    let core_mask_p = if data.pad_frames < mask_p.len() {
        &mask_p[data.pad_frames..]
    } else {
        &[]
    };

    let core_frames_l = if data.pad_frames < core_frames_l.len() {
        &core_frames_l[data.pad_frames..]
    } else {
        &[]
    };
    let core_frames_r = if data.pad_frames < core_frames_r.len() {
        &core_frames_r[data.pad_frames..]
    } else {
        &[]
    };

    // ΤΟ ΙΔΙΟ STFT ΠΟΥ ΠΑΡΗΓΑΓΕ ΤΑ MASKS.
    //
    // Τα masks βγαίνουν από forward(padded_chunk), που
    // βάζει FFT_SIZE/2 μηδενικά μπροστά και πίσω. Ο
    // carrier ΠΡΕΠΕΙ να έχει την ΙΔΙΑ ευθυγράμμιση —
    // αλλιώς το mask πολλαπλασιάζεται σε λάθος frames
    // και το ISTFT ακυρώνει ενέργεια.
    //
    // ΜΕΤΡΗΜΕΝΟ: με τα streaming frames του
    // StreamingStftEncoder (carry με πραγματικά
    // γειτονικά δείγματα) το LUFS έπεσε από −14.15 σε
    // −39.41. Η διαφορά είναι ΜΕΣΑ στο παράθυρο, οπότε
    // κανένα frame slicing δεν τη διορθώνει.
    //
    // ΞΕΧΩΡΙΣΤΟΣ engine ανά κανάλι — κρατάει state.
    let mut eng_l = crate::stft::StftEngine::new();
    let mut eng_r = crate::stft::StftEngine::new();
    let core_l_cplx_vec = eng_l.forward(data.core_left).0;
    let core_r_cplx_vec = eng_r.forward(data.core_right).0;
    let core_l_cplx = core_l_cplx_vec.as_slice();
    let core_r_cplx = core_r_cplx_vec.as_slice();

    // ── DRUM DE-DUP ──
    // Το core_mask_p είναι ΗΔΗ σωστά sliced
    // ([pad_frames..]) — ίδιο grid με τα core masks.
    // ΚΑΝΕΝΑ νέο alignment. Ο δράκος του S2 δεν
    // αγγίζεται.
    let deduped_voice;
    let deduped_bass;
    let deduped_harm;
    let deduped_amb;

    let mut sum_p = 0.0;
    let mut count_p = 0;
    for frame in core_mask_p {
        for &val in frame {
            sum_p += val as f32;
            count_p += 1;
        }
    }
    let mean_p = if count_p > 0 { sum_p / count_p as f32 } else { 0.0 };

    let (v_mask, b_mask, h_mask, a_mask) = if DRUM_DEDUP_ALPHA > 0.0 {
        let dedup = |mask: &[Vec<f32>]| -> Vec<Vec<f32>> {
            mask.iter().enumerate().map(|(t, frame)| {
                frame.iter().enumerate().map(|(b, &m)| {
                    let p = core_mask_p.get(t)
                        .and_then(|f| f.get(b)).copied()
                        .unwrap_or(0.0);
                    m * (1.0 - DRUM_DEDUP_ALPHA * p)
                }).collect()
            }).collect()
        };
        deduped_voice = dedup(core_voice_mask);
        deduped_bass = dedup(core_bass_mask);
        deduped_harm = dedup(core_harm_mask);
        deduped_amb = dedup(core_amb_mask);
        (deduped_voice.as_slice(), deduped_bass.as_slice(), deduped_harm.as_slice(), deduped_amb.as_slice())
    } else {
        deduped_voice = Vec::new();
        deduped_bass = Vec::new();
        deduped_harm = Vec::new();
        deduped_amb = Vec::new();
        (core_voice_mask, core_bass_mask, core_harm_mask, core_amb_mask)
    };

    // ── ΙΔΙΟ MASK, ΔΥΟ ΚΑΝΑΛΙΑ ──
    //
    // Το mask υπολογίστηκε από το mono άθροισμα και λέει
    // ποιο bin ανήκει σε ποιο stem. Εφαρμοσμένο ξεχωριστά
    // σε L και R, κάθε stem κρατάει τη ΔΙΚΗ ΤΟΥ
    // στερεοφωνική εικόνα.
    //
    // ΗΤΑΝ: μία εφαρμογή στο mono core_chunk. Η θέση κάθε
    // οργάνου χανόταν, και δύο μετρήσεις έδειξαν ότι δεν
    // ανακατασκευάζεται (95614c5).
    let voice = StereoStem {
        l: apply_spectral_mask_to_chunk(
            core_l_cplx, v_mask, data.core_left.len()),
        r: apply_spectral_mask_to_chunk(
            core_r_cplx, v_mask, data.core_right.len()),
    };
    let bass = StereoStem {
        l: apply_spectral_mask_to_chunk(
            core_l_cplx, b_mask, data.core_left.len()),
        r: apply_spectral_mask_to_chunk(
            core_r_cplx, b_mask, data.core_right.len()),
    };
    let harmonics = StereoStem {
        l: apply_spectral_mask_to_chunk(
            core_l_cplx, h_mask, data.core_left.len()),
        r: apply_spectral_mask_to_chunk(
            core_r_cplx, h_mask, data.core_right.len()),
    };
    let ambience = StereoStem {
        l: apply_spectral_mask_to_chunk(
            core_l_cplx, a_mask, data.core_left.len()),
        r: apply_spectral_mask_to_chunk(
            core_r_cplx, a_mask, data.core_right.len()),
    };

    let drums_weights: Vec<f32> = (0..data.core_chunk.len())
        .map(|i| {
            let f = i * core_n_frames / data.core_chunk.len().max(1);
            if f < core_mask_p.len() {
                core_mask_p[f].iter().sum::<f32>() / N_BINS as f32
            } else {
                0.0
            }
        })
        .collect();
    
    // Δύο πολλαπλασιασμοί, μηδέν φασματική δουλειά.
    // Το μόνο stem που κρατάει την αρχική φάση ανέπαφη.
    let drums = StereoStem {
        l: data.core_left.iter().zip(drums_weights.iter())
            .map(|(s, w)| s * w).collect(),
        r: data.core_right.iter().zip(drums_weights.iter())
            .map(|(s, w)| s * w).collect(),
    };

    let h_voice: Vec<f32> = (0..core_n_frames)
        .map(|f| {
            let real_f = f + data.pad_frames;
            let idx = scout.voice_idx * n_frames + real_f;
            if idx < h_chunk.len() {
                h_chunk[idx]
            } else {
                0.0
            }
        })
        .collect();

    let v_transient = transient_density(&h_voice);
    let d_transient = core_mask_p
        .iter()
        .map(|f| f.iter().sum::<f32>() / N_BINS as f32)
        .sum::<f32>()
        / core_n_frames.max(1) as f32;

    let mut spatial_sums = [(0.0, 0.0, 0.0); 5]; // [(pan_num, width_num, den); 5]
    for f in 0..core_n_frames {
        if f >= core_frames_l.len() || f >= core_frames_r.len() {
            continue;
        }
        for b in 0..N_BINS {
            let x_l = core_frames_l[f][b];
            let x_r = core_frames_r[f][b];
            let amp = x_l + x_r;
            if amp < 1e-6 {
                continue;
            }
            let p = (x_r - x_l) / (amp + 1e-10_f32);
            let p_abs = p.abs();

            let band_idx = if b <= 10 {
                0 // Lows (0 - 250 Hz)
            } else if b <= 42 {
                1 // Low-Mids (250 - 1000 Hz)
            } else if b <= 170 {
                2 // Mids (1000 - 4000 Hz)
            } else if b <= 341 {
                3 // High-Mids (4000 - 8000 Hz)
            } else {
                4 // Highs (8000+ Hz)
            };

            spatial_sums[band_idx].0 += p * amp;
            spatial_sums[band_idx].1 += p_abs * amp;
            spatial_sums[band_idx].2 += amp;
        }
    }

    ParallelChunkOut {
        stems: FiveStemsChunk {
            voice,
            drums,
            bass,
            harmonics,
            ambience,
        },
        voice_transient: v_transient,
        drums_transient: d_transient,
        chunk_len: data.core_chunk.len(),
        spatial_sums,
    }
}

pub struct TwoPassEngine {
    nmf: NmfEngine,
    /// Psychoacoustic Collision Matrix — smoothed ducking gain state.
    /// Initialized to 1.0 (no ducking). Persists across chunks.
    bass_ducking_gain: f32,
}

pub(crate) fn detect_collision(drums_chunk: &[f32], bass_chunk: &[f32]) -> bool {
    if drums_chunk.is_empty() || bass_chunk.is_empty() {
        return false;
    }
    let drums_sum_sq: f32 = drums_chunk.iter().map(|s| s * s).sum();
    let drums_mean_sq = drums_sum_sq / drums_chunk.len() as f32;
    let drums_diff_sq: f32 = drums_chunk.windows(2).map(|w| (w[1] - w[0]).powi(2)).sum();
    let drums_td = if drums_mean_sq > 1e-10 {
        drums_diff_sq / (drums_mean_sq * drums_chunk.len() as f32)
    } else {
        0.0
    };
    let bass_sum_sq: f32 = bass_chunk.iter().map(|s| s * s).sum();
    let bass_mean_sq = bass_sum_sq / bass_chunk.len() as f32;
    let bass_rms_db = if bass_mean_sq > 1e-15 {
        10.0 * libm::log10f(bass_mean_sq)
    } else {
        -144.0
    };
    drums_td > COLLISION_DRUMS_TRANSIENT_THRESHOLD && bass_rms_db > COLLISION_BASS_RMS_THRESHOLD_DB
}

impl TwoPassEngine {
    pub fn new() -> Self {
        Self {
            nmf: NmfEngine::default(),
            bass_ducking_gain: 1.0,
        }
    }

    // ── Pass 1 — Scout ───────────────────────────────────────────────

    /// Pass 1: proxy analysis → all static parameters locked.
    /// INV-ST-1: only fit() call in the entire run.
    /// Το `left`/`right` χρησιμοποιούνται ΜΟΝΟ για τη
    /// χωρική ανάλυση. Το NMF, το NMFD και τα proxy stems
    /// δουλεύουν στο mono άθροισμα — ο διαχωρισμός δεν
    /// χρειάζεται στερεοφωνία, η ΜΕΤΡΗΣΗ την χρειάζεται.
    ///
    /// ΗΤΑΝ ένα mono `signal`. Το SpatialPreAnalysis
    /// έπαιρνε δύο κλώνους του, μετρούσε (l−r)*0.5 = 0,
    /// και το ms_ratio ήταν σταθερά μηδέν — άρα το
    /// voice_center πάντα 0.95 και το ambience_rear πάντα
    /// 0.30, ενώ ο κώδικας διάβαζε σαν να προσαρμοζόταν
    /// στο υλικό.
    pub fn scout(
        &mut self,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
        quiet_window_start_frame: Option<usize>,
        dump_source: Option<&crate::stft::raw_pcm_source::RawPcmFileSource>,
        run_nmfd: bool,
    ) -> ScoutResult {
        self.scout_with_profile(left, right, sample_rate, quiet_window_start_frame, dump_source, run_nmfd, None)
    }

    pub fn scout_with_profile(
        &mut self,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
        _quiet_window_start_frame: Option<usize>,
        _dump_source: Option<&crate::stft::raw_pcm_source::RawPcmFileSource>,
        run_nmfd: bool,
        profile: Option<crate::spatial::user_profile::UserSpatialProfile>,
    ) -> ScoutResult {
        let t_scout = std::time::Instant::now();

        // Το mono άθροισμα, για ό,τι δεν χρειάζεται κανάλια.
        let signal: Vec<f32> = left
            .iter()
            .zip(right.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();
        let signal = &signal[..];

        // Downsample → ~11kHz mono proxy
        let proxy: Vec<f32> = signal.iter().step_by(SCOUT_DOWNSAMPLE).copied().collect();

        let proxy_rms = if !proxy.is_empty() {
            let sq: f32 = proxy.iter().map(|s| s * s).sum();
            libm::sqrtf(sq / proxy.len() as f32)
        } else {
            0.0
        };

        // STFT on proxy
        let t_stft = std::time::Instant::now();
        // scout-resident by design: this IS the NMF W-fit window (nmfd_streaming_spec.md §2 — W fits on scout, never in-stream)
        let mut ctx = StreamingStftEncoder::new();
        let mut frames_cplx = ctx.feed_chunk(&proxy);
        frames_cplx.extend(ctx.finish());
        let proxy_frames: Vec<Vec<f32>> = frames_cplx
            .into_iter()
            .map(|f| {
                f.into_iter()
                    .map(|c| libm::sqrtf(c.re * c.re + c.im * c.im))
                    .collect()
            })
            .collect();
        eprintln!("[PERF] stft_proxy={}ms", t_stft.elapsed().as_millis());
        let n_frames = proxy_frames.len();
        let n_bins = if n_frames > 0 {
            proxy_frames[0].len()
        } else {
            0
        };
        eprintln!("[PERF] proxy size: {} frames x {} bins", n_frames, n_bins);

        // NMF fit — INV-ST-1: ONLY fit() call
        // W learned at proxy sample rate (~12kHz, SCOUT_DOWNSAMPLE=4)
        let t_fit = std::time::Instant::now();
        let w_proxy = self.nmf.fit(&proxy_frames);
        eprintln!("[PERF] nmf_fit={}ms", t_fit.elapsed().as_millis());

        let (nmfd_tensor_w, nmfd_tau, nmfd_bass_idx, nmfd_harmonics_idx, nmfd_ambience_idx) = if run_nmfd {
            // ── NMFD fit (Rung-C) on FULL-RATE proxy ───────────────────────
            let t_nmfd = std::time::Instant::now();
            let nmfd_n_mels = crate::analysis::mel_128::MEL_BANDS;

            let chunk_size = 48000;
            let mut nmfd_ctx = StreamingStftEncoder::new();
            let mut mel_v_fm = Vec::with_capacity(nmfd_n_mels * (signal.len() / 512 + 2));
            let mut nmfd_n_frames = 0;

            for chunk in signal.chunks(chunk_size) {
                let frames_cplx = nmfd_ctx.feed_chunk(chunk);
                for f in 0..frames_cplx.len() {
                    let mut frame = [0.0_f32; N_BINS];
                    for b in 0..N_BINS {
                        let c = frames_cplx[f][b];
                        frame[b] = libm::sqrtf(c.re * c.re + c.im * c.im);
                    }
                    mel_v_fm.extend_from_slice(&crate::analysis::mel_128::fold_to_mel(&frame));
                    nmfd_n_frames += 1;
                }
            }
            let finish_cplx = nmfd_ctx.finish();
            for f in 0..finish_cplx.len() {
                let mut frame = [0.0_f32; N_BINS];
                for b in 0..N_BINS {
                    let c = finish_cplx[f][b];
                    frame[b] = libm::sqrtf(c.re * c.re + c.im * c.im);
                }
                mel_v_fm.extend_from_slice(&crate::analysis::mel_128::fold_to_mel(&frame));
                nmfd_n_frames += 1;
            }

            let mut mel_v = vec![0.0_f32; nmfd_n_mels * nmfd_n_frames];
            for f in 0..nmfd_n_frames {
                for m in 0..nmfd_n_mels {
                    mel_v[m * nmfd_n_frames + f] = mel_v_fm[f * nmfd_n_mels + m];
                }
            }

            let nmfd_tau = 8;

            // fit_protocol_v3: K=8 (4 frozen slots seeded from LibriSpeech w_speech_v1.bin,
            // 4 free slots seeded randomly), 128 mel bins, all frames, 12 iterations, seed 314159, tau=8.
            let decisions = crate::analysis::scout_scanner::scan_file(&signal, &signal, sample_rate);
            let mut sum_conf = 0.0;
            let mut sum_weighted_lean = 0.0;
            for (_, d) in decisions {
                sum_conf += d.confidence;
                sum_weighted_lean += d.leaning_score * d.confidence;
            }
            let weighted_lean = if sum_conf > 0.0 { sum_weighted_lean / sum_conf } else { 1.0 };
            
            let is_music_profile = if let Some(p) = &profile {
                p.is_music()
            } else {
                false
            };
            
            let use_drums = is_music_profile && weighted_lean < 0.35 && sum_conf > 0.0;

            println!("NMFD_INIT|mode={}|profile_music={}|lean={:.4}|gate={}", 
                if run_nmfd { "active" } else { "bypass" }, 
                is_music_profile,
                weighted_lean,
                if use_drums { "PASS" } else { "BYPASS" });

            let nmfd_k = if use_drums { 11 } else { 8 };
            let nmfd_frozen_k = if use_drums { 7 } else { 4 };
            let free_start = if use_drums { 7 } else { 4 };
            let nmfd_num_iter = 30;
            let nmfd_seed = 314159;

            let mut init_w = vec![0.0_f32; nmfd_n_mels * nmfd_k * nmfd_tau];
            let mut init_h = vec![0.0_f32; nmfd_k * nmfd_n_frames];

            // 1. w_speech_v1.bin (K=4 LibriSpeech tensor; 8kHz limit
            // noted). Embedded at compile time — production builds
            // must not depend on a dev checkout layout. Source of
            // truth: research/w-speech/ (retrains re-copy + SHA-pair).
            static W_SPEECH_V1: &[u8] = include_bytes!("../../assets/w_speech_v1.bin");
            const _: () = assert!(W_SPEECH_V1.len() == 128 * 4 * 8 * 4);
            let mut w_speech = vec![0.0_f32; 128 * 4 * 8];
            for i in 0..128 * 4 * 8 {
                w_speech[i] = f32::from_le_bytes(W_SPEECH_V1[i * 4..(i + 1) * 4].try_into().unwrap());
            }

            // 1b. w_drums_v1.bin (K=3)
            static W_DRUMS_V1: &[u8] = include_bytes!("../../assets/w_drums_v1.bin");
            const _: () = assert!(W_DRUMS_V1.len() == 128 * 3 * 8 * 4);
            let mut w_drums = vec![0.0_f32; 128 * 3 * 8];
            for i in 0..128 * 3 * 8 {
                w_drums[i] = f32::from_le_bytes(W_DRUMS_V1[i * 4..(i + 1) * 4].try_into().unwrap());
            }

            // 2. Fill slots 0-3 frozen (speech)
            for m in 0..128 {
                for r in 0..4 {
                    for tau in 0..nmfd_tau {
                        init_w[(m * nmfd_k * nmfd_tau) + (r * nmfd_tau) + tau] =
                            w_speech[(m * 4 * nmfd_tau) + (r * nmfd_tau) + tau];
                    }
                }
            }
            
            // 2b. Fill slots 4-6 frozen (drums) if used
            if use_drums {
                for m in 0..128 {
                    for r in 0..3 {
                        for tau in 0..nmfd_tau {
                            init_w[(m * nmfd_k * nmfd_tau) + ((r + 4) * nmfd_tau) + tau] =
                                w_drums[(m * 3 * nmfd_tau) + (r * nmfd_tau) + tau];
                        }
                    }
                }
            }

            // 3. Seed 4 free slots randomly
            let mut lcg_state: u32 = nmfd_seed;
            let mut next_rand = || -> f32 {
                lcg_state = lcg_state.wrapping_mul(1664525).wrapping_add(1013904223);
                (lcg_state as f32) / (u32::MAX as f32)
            };

            for m in 0..128 {
                for r in free_start..(free_start + 4) {
                    for tau in 0..nmfd_tau {
                        init_w[(m * nmfd_k * nmfd_tau) + (r * nmfd_tau) + tau] = next_rand();
                    }
                }
            }
            for x in init_h.iter_mut() {
                *x = next_rand();
            }

            let mut current_w = init_w.clone();
            let mut current_h = init_h.clone();
            let mut final_cost = 0.0;
            for iter in 1..=nmfd_num_iter {
                let (next_w, next_h, cost) = crate::stft::nmfd::nmfd_f32_partial_frozen(
                    &mel_v,
                    &current_w,
                    &current_h,
                    nmfd_n_mels,
                    nmfd_frozen_k,
                    nmfd_k,
                    nmfd_n_frames,
                    nmfd_tau,
                    1,
                );
                current_w = next_w;
                current_h = next_h;
                final_cost = cost;
                println!("SCOUTFIT_ITER|iter={}|cost={:.8e}", iter, cost);
            }
            let nmfd_tensor_w = current_w;

            eprintln!(
                "[PERF] SCOUTFIT|ms={}|cost={}",
                t_nmfd.elapsed().as_millis(),
                final_cost
            );

            // ── Free Slots Semantic Assignment (Report Only) ───────────────────────────────
            let free_count = 4;
            let mut free_flatness = [0.0f32; 4];
            let mut free_centroids = [0.0f32; 4];

            for i in 0..free_count {
                let c = free_start + i;
                let mut log_sum = 0.0f32;
                let mut arith = 0.0f32;
                let mut sum_mw = 0.0f32;
                let eps = 1e-10f32;

                for m in 0..128 {
                    let mut avg_t = 0.0_f32;
                    for t in 0..nmfd_tau {
                        avg_t += nmfd_tensor_w[m * (nmfd_k * nmfd_tau) + c * nmfd_tau + t];
                    }
                    avg_t /= nmfd_tau as f32;

                    log_sum += libm::logf(avg_t + eps);
                    arith += avg_t;
                    sum_mw += m as f32 * avg_t;
                }
                let geom = libm::expf(log_sum / 128.0);
                let mean = arith / 128.0;
                free_flatness[i] = if mean > eps {
                    (geom / mean).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                free_centroids[i] = if mean > eps { sum_mw / arith } else { 0.0 };
            }

            let ambience_idx_free = (0..free_count)
                .max_by(|&a, &b| free_flatness[a].partial_cmp(&free_flatness[b]).unwrap())
                .unwrap_or(0);
            let remaining_free: Vec<usize> = (0..free_count)
                .filter(|&i| i != ambience_idx_free)
                .collect();
            let mut sorted_by_centroid = remaining_free.clone();
            sorted_by_centroid
                .sort_by(|&a, &b| free_centroids[a].partial_cmp(&free_centroids[b]).unwrap());

            let bass_idx_free = sorted_by_centroid[0];
            let drums_idx_free = sorted_by_centroid[1];
            let harmonics_idx_free = sorted_by_centroid[2];

            println!(
                "FREE_SLOTS|role=bass|slot={}|centroid={:.2}|flatness={:.4}",
                bass_idx_free + free_start,
                free_centroids[bass_idx_free],
                free_flatness[bass_idx_free]
            );
            println!(
                "FREE_SLOTS|role=drums|slot={}|centroid={:.2}|flatness={:.4}",
                drums_idx_free + free_start,
                free_centroids[drums_idx_free],
                free_flatness[drums_idx_free]
            );
            println!(
                "FREE_SLOTS|role=harmonics|slot={}|centroid={:.2}|flatness={:.4}",
                harmonics_idx_free + free_start,
                free_centroids[harmonics_idx_free],
                free_flatness[harmonics_idx_free]
            );
            println!(
                "FREE_SLOTS|role=ambience|slot={}|centroid={:.2}|flatness={:.4}",
                ambience_idx_free + free_start,
                free_centroids[ambience_idx_free],
                free_flatness[ambience_idx_free]
            );

            // ── ΠΟΥ ΖΕΙ ΚΑΘΕ STEM ΦΑΣΜΑΤΙΚΑ ──
            //
            // Το W matrix ΕΙΝΑΙ το φασματικό πρότυπο κάθε
            // component. Δεν χρειάζεται φίλτρο ούτε δεύτερη
            // ανάλυση — υπάρχει ήδη, σε πλήρες εύρος.
            //
            // ΤΟ drums_idx_free ΣΥΜΠΕΡΙΛΑΜΒΑΝΕΤΑΙ παρότι η
            // παραγωγή παίρνει drums από HPSS. Είναι το μόνο
            // φασματικό πρότυπο για κρουστά που υπάρχει στο
            // scout, και το σχίσμα είναι καταγεγραμμένο (0ef3eb2).
            //
            // ΜΗ ΣΥΝΔΕΔΕΜΕΝΟ. Τυπώνεται, δεν διαβάζεται.

            let (mel_band, mel_width): ([usize; 128], [f32; 128]) = {
                let mut band = [0usize; 128];
                let mut width = [0.0_f32; 128];
                for m in 0..crate::analysis::mel_128::MEL_BANDS {
                    let mut num = 0.0_f32;
                    let mut den = 0.0_f32;
                    for b in 0..crate::analysis::mel_128::N_BINS {
                        let w = crate::analysis::mel_128::MEL_128_MATRIX[m][b];
                        num += w * b as f32;
                        den += w;
                    }
                    let c = if den > 1e-10 { num / den } else { 0.0 };
                    width[m] = den.max(1e-10);
                    band[m] = if c <= 10.0 {
                        0
                    } else if c <= 42.0 {
                        1
                    } else if c <= 170.0 {
                        2
                    } else if c <= 341.0 {
                        3
                    } else {
                        4
                    };
                }
                (band, width)
            };

            // ΠΟΣΑ mel bins και ΠΟΣΟ ΕΥΡΟΣ ανά ζώνη. Αν μια ζώνη
            // έχει λίγα bins αλλά μεγάλο εύρος, η μέτρησή της
            // είναι χονδροειδής.
            {
                let mut cnt = [0usize; 5];
                let mut wid = [0.0_f32; 5];
                for m in 0..crate::analysis::mel_128::MEL_BANDS {
                    cnt[mel_band[m]] += 1;
                    wid[mel_band[m]] += mel_width[m];
                }
                eprintln!(
                    "[MEL-MAP] bins: {} {} {} {} {} · width: {:.1} {:.1} {:.1} {:.1} {:.1}",
                    cnt[0], cnt[1], cnt[2], cnt[3], cnt[4],
                    wid[0], wid[1], wid[2], wid[3], wid[4]
                );
            }

            let roles: [(&str, usize); 4] = [
                ("bass", bass_idx_free + free_start),
                ("harmonics", harmonics_idx_free + free_start),
                ("ambience", ambience_idx_free + free_start),
                ("drums", drums_idx_free + free_start),
            ];

            for (name, c) in roles.iter() {
                let mut e = [0.0_f32; 5];
                for m in 0..nmfd_n_mels {
                    let mut avg_t = 0.0_f32;
                    for t in 0..nmfd_tau {
                        avg_t += nmfd_tensor_w[m * (nmfd_k * nmfd_tau) + *c * nmfd_tau + t];
                    }
                    e[mel_band[m]] += (avg_t / nmfd_tau as f32) / mel_width[m];
                }
                let tot: f32 = e.iter().sum::<f32>().max(1e-10);
                eprintln!(
                    "[STEM-BAND] {}: lows={:.4} low_mid={:.4} mid={:.4} \
                     high_mid={:.4} high={:.4}",
                    name, e[0] / tot, e[1] / tot, e[2] / tot, e[3] / tot, e[4] / tot
                );
            }

            // ΤΟ VOICE είναι το ΑΘΡΟΙΣΜΑ των frozen slots 0..4.
            let mut ev = [0.0_f32; 5];
            for m in 0..nmfd_n_mels {
                let mut acc = 0.0_f32;
                for c in 0..nmfd_frozen_k {
                    for t in 0..nmfd_tau {
                        acc += nmfd_tensor_w[m * (nmfd_k * nmfd_tau) + c * nmfd_tau + t];
                    }
                }
                ev[mel_band[m]] += (acc / (nmfd_tau * nmfd_frozen_k) as f32) / mel_width[m];
            }
            let tot: f32 = ev.iter().sum::<f32>().max(1e-10);
            eprintln!(
                "[STEM-BAND] voice: lows={:.4} low_mid={:.4} mid={:.4} \
                 high_mid={:.4} high={:.4}",
                ev[0] / tot, ev[1] / tot, ev[2] / tot, ev[3] / tot, ev[4] / tot
            );

            // ── ΠΟΣΟ ΑΠΟΦΑΣΙΣΤΙΚΑ ΜΟΙΡΑΖΕΤΑΙ ΚΑΘΕ BIN ──
            //
            // Το W matrix δίνει, για κάθε mel bin, μια κατανομή
            // ενέργειας πάνω στα k components. Αν ένα component
            // κυριαρχεί, ο διαχωρισμός είναι καθαρός εκεί. Αν
            // μοιράζονται, κάθε stem παίρνει κομμάτι του ίδιου
            // bin — και τότε τα stems ΔΕΝ μπορούν να ξεχωρίσουν
            // φασματικά, όσο καλά κι αν ξεχωρίζουν χρονικά.
            //
            // ΓΙΑΤΙ ΜΕΤΡΑΤΑΙ: τρία προβλήματα κόλλησαν εδώ —
            // spatial panning, EQ masking, micro-VAD. Κοινή
            // υποψία ότι το soft Wiener masking είναι θολό εξ
            // ορισμού.
            //
            // ΜΗ ΣΥΝΔΕΔΕΜΕΝΟ. Τυπώνεται, δεν διαβάζεται.
            {
                let mut ent_sum = 0.0_f32;
                let mut top_sum = 0.0_f32;
                let mut top2_sum = 0.0_f32;
                let mut counted = 0usize;

                // ΚΑΤΑΝΟΜΗ ΤΟΥ ΚΥΡΙΑΡΧΟΥ ΣΕ ΚΑΔΟΥΣ, ώστε να
                // φανεί αν υπάρχουν ΚΑΙ καθαρά ΚΑΙ θολά bins,
                // ή αν είναι όλα μέτρια.
                let mut hist = [0usize; 5]; // <0.3 · <0.5 · <0.7 · <0.9 · ≥0.9

                for m in 0..nmfd_n_mels {
                    // Ενέργεια κάθε component σε αυτό το mel bin,
                    // αθροισμένη πάνω στο tau.
                    let mut e = vec![0.0_f32; nmfd_k];
                    for c in 0..nmfd_k {
                        let mut acc = 0.0_f32;
                        for t in 0..nmfd_tau {
                            acc += nmfd_tensor_w[m * (nmfd_k * nmfd_tau) + c * nmfd_tau + t];
                        }
                        e[c] = acc;
                    }
                    let tot: f32 = e.iter().sum();
                    if tot < 1e-9 {
                        continue;
                    }

                    // Εντροπία, κανονικοποιημένη στο [0,1].
                    let mut h = 0.0_f32;
                    for c in 0..nmfd_k {
                        let p = e[c] / tot;
                        if p > 1e-9 {
                            h -= p * libm::logf(p);
                        }
                    }
                    let h_norm = h / libm::logf(nmfd_k as f32);

                    // Μερίδιο κυρίαρχου και δεύτερου.
                    let mut sorted = e.clone();
                    sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
                    let top = sorted[0] / tot;
                    let top2 = if nmfd_k > 1 { sorted[1] / tot } else { 0.0 };

                    ent_sum += h_norm;
                    top_sum += top;
                    top2_sum += top2;
                    counted += 1;

                    let idx = if top < 0.3 {
                        0
                    } else if top < 0.5 {
                        1
                    } else if top < 0.7 {
                        2
                    } else if top < 0.9 {
                        3
                    } else {
                        4
                    };
                    hist[idx] += 1;
                }

                let n = counted.max(1) as f32;
                eprintln!(
                    "[MASK-SHARP] entropy={:.4} top={:.4} top2={:.4} bins={}",
                    ent_sum / n,
                    top_sum / n,
                    top2_sum / n,
                    counted
                );
                eprintln!(
                    "[MASK-HIST] <0.3={} <0.5={} <0.7={} <0.9={} >=0.9={}",
                    hist[0], hist[1], hist[2], hist[3], hist[4]
                );
            }

            // ── ΤΟ ΠΡΑΓΜΑΤΙΚΟ MASK, ΟΧΙ ΜΟΝΟ ΤΟ W ──
            //
            // Το [MASK-SHARP] μέτρησε το W: πόσο μοιράζεται
            // κάθε mel bin μεταξύ components, αθροισμένο σε όλο
            // τον χρόνο. Αλλά το mask που εφαρμόζεται είναι
            // W·H — και ένα component διάχυτο φασματικά μπορεί
            // να ενεργοποιείται μόνο σε λίγα frames, οπότε το
            // τελικό mask να είναι αιχμηρό εκεί που μετράει.
            //
            // ΑΥΤΟ ΜΕΤΡΑΕΙ ΤΟ ΓΙΝΟΜΕΝΟ, ανά (bin, frame), και
            // ΣΤΑΘΜΙΖΕΙ ΜΕ ΤΗΝ ΕΝΕΡΓΕΙΑ — ένα frame σιωπής δεν
            // πρέπει να μετράει όσο ένα δυνατό.
            //
            // ΜΗ ΣΥΝΔΕΔΕΜΕΝΟ. Τυπώνεται, δεν διαβάζεται.
            {
                let n_frames = nmfd_n_frames;

                let mut ent_w = 0.0_f32;   // εντροπία, σταθμισμένη
                let mut top_w = 0.0_f32;
                let mut wsum  = 0.0_f32;   // συνολικό βάρος
                let mut hist = [0usize; 5];
                let mut counted = 0usize;

                // ΔΕΙΓΜΑΤΟΛΗΨΙΑ: κάθε 16ο frame, αλλιώς είναι
                // 128 bins × μερικές χιλιάδες frames × 8 comps.
                const FRAME_STEP: usize = 16;

                for f in (0..n_frames).step_by(FRAME_STEP) {
                    for m in 0..nmfd_n_mels {
                        // Συνεισφορά κάθε component σε αυτό το
                        // (bin, frame): Σ_tau W[m,c,tau]·H[c,f−tau]
                        let mut e = vec![0.0_f32; nmfd_k];
                        for c in 0..nmfd_k {
                            let mut acc = 0.0_f32;
                            for t in 0..nmfd_tau {
                                if f < t {
                                    continue;
                                }
                                let w = nmfd_tensor_w
                                    [m * (nmfd_k * nmfd_tau) + c * nmfd_tau + t];
                                let h = current_h[c * nmfd_n_frames + (f - t)];
                                acc += w * h;
                            }
                            e[c] = acc;
                        }
                        let tot: f32 = e.iter().sum();
                        if tot < 1e-9 {
                            continue;
                        }

                        let mut hh = 0.0_f32;
                        for c in 0..nmfd_k {
                            let p = e[c] / tot;
                            if p > 1e-9 {
                                hh -= p * libm::logf(p);
                            }
                        }
                        let h_norm = hh / libm::logf(nmfd_k as f32);

                        let mut top = 0.0_f32;
                        for c in 0..nmfd_k {
                            let p = e[c] / tot;
                            if p > top {
                                top = p;
                            }
                        }

                        // ΣΤΑΘΜΙΣΗ ΜΕ ΤΗΝ ΕΝΕΡΓΕΙΑ ΤΟΥ BIN.
                        ent_w += h_norm * tot;
                        top_w += top * tot;
                        wsum  += tot;
                        counted += 1;

                        let idx = if top < 0.3 {
                            0
                        } else if top < 0.5 {
                            1
                        } else if top < 0.7 {
                            2
                        } else if top < 0.9 {
                            3
                        } else {
                            4
                        };
                        hist[idx] += 1;
                    }
                }

                let w = wsum.max(1e-10);
                eprintln!(
                    "[MASK-REAL] entropy={:.4} top={:.4} cells={} frames_step={}",
                    ent_w / w,
                    top_w / w,
                    counted,
                    FRAME_STEP
                );
                eprintln!(
                    "[MASK-REAL-HIST] <0.3={} <0.5={} <0.7={} <0.9={} >=0.9={}",
                    hist[0], hist[1], hist[2], hist[3], hist[4]
                );
            }

            (nmfd_tensor_w, nmfd_tau, bass_idx_free + free_start, harmonics_idx_free + free_start, ambience_idx_free + free_start)
        } else {
            (
                Vec::new(),
                0, // 0 is recognizably invalid (not 8)
                usize::MAX,
                usize::MAX,
                usize::MAX,
            )
        };

        // ── ΠΟΥ ΓΕΡΝΕΙ ΚΑΘΕ ΖΩΝΗ ΣΤΟ ΠΡΩΤΟΤΥΠΟ ──
        //
        // ΙΔΙΟΣ ΥΠΟΛΟΓΙΣΜΟΣ με two_pass.rs:429 — p =
        // (|X_r| − |X_l|) / (|X_l| + |X_r|), σταθμισμένο με
        // amp ανά bin, ίδια όρια ζωνών κατά bin index.
        //
        // ΓΙΑΤΙ ΕΔΩ: στην Pass 2 υπολογίζεται ήδη, αλλά η
        // StemChannelAssignments::compute() αποφασίζει στο
        // scout. Η πληροφορία έφτανε πάντα αργά.
        //
        // ΓΙΑΤΙ STFT ΚΑΙ ΟΧΙ ΦΙΛΤΡΑ: μετρήθηκε ότι το LR4
        // δίνει RMS ανά ζώνη ενώ αυτό σταθμίζει ανά bin —
        // διαφορετικά μεγέθη. Τα 24 κομμάτια της 2026-08-12
        // δεν θα ίσχυαν ως αναφορά.
        //
        // ΜΗ ΣΥΝΔΕΔΕΜΕΝΟ. Τυπώνεται, δεν διαβάζεται.
        let chunk_size = 48000;
        let mut stft_l = StreamingStftEncoder::new();
        let mut stft_r = StreamingStftEncoder::new();
        let mut band_sums = [(0.0_f32, 0.0_f32, 0.0_f32); 5];

        for (cl, cr) in left.chunks(chunk_size).zip(right.chunks(chunk_size)) {
            let fl = stft_l.feed_chunk(cl);
            let fr = stft_r.feed_chunk(cr);
            for (frame_l, frame_r) in fl.iter().zip(fr.iter()) {
                for b in 0..N_BINS {
                    let x_l = libm::sqrtf(
                        frame_l[b].re * frame_l[b].re + frame_l[b].im * frame_l[b].im,
                    );
                    let x_r = libm::sqrtf(
                        frame_r[b].re * frame_r[b].re + frame_r[b].im * frame_r[b].im,
                    );
                    let amp = x_l + x_r;
                    if amp < 1e-6 {
                        continue;
                    }
                    let p = (x_r - x_l) / (amp + 1e-10_f32);
                    // ΙΔΙΑ ΟΡΙΑ με two_pass.rs:432-442.
                    let band = if b <= 10 {
                        0
                    } else if b <= 42 {
                        1
                    } else if b <= 170 {
                        2
                    } else if b <= 341 {
                        3
                    } else {
                        4
                    };
                    band_sums[band].0 += p * amp;
                    band_sums[band].1 += libm::fabsf(p) * amp;
                    band_sums[band].2 += amp;
                }
            }
        }

        let mut scout_pan = [0.0_f32; 5];
        let mut scout_width = [0.0_f32; 5];
        for i in 0..5 {
            let den = band_sums[i].2.max(1e-10);
            scout_pan[i] = band_sums[i].0 / den;
            scout_width[i] = band_sums[i].1 / den;
        }

        eprintln!(
            "[SCOUT-PAN] lows={:.4} low_mid={:.4} mid={:.4} high_mid={:.4} high={:.4}",
            scout_pan[0], scout_pan[1], scout_pan[2], scout_pan[3], scout_pan[4]
        );
        eprintln!(
            "[SCOUT-WIDTH] lows={:.4} low_mid={:.4} mid={:.4} high_mid={:.4} high={:.4}",
            scout_width[0], scout_width[1], scout_width[2], scout_width[3], scout_width[4]
        );

        // ── W-matrix bin mapping: proxy (12kHz) → full (48kHz) ──────
        // SCOUT_DOWNSAMPLE=4 is an integer → b_proxy = b_full * 4 exactly.
        // No floating-point interpolation needed — direct index lookup.
        //
        // Bin mapping:
        //   b_full × 23.4 Hz = b_proxy × 5.86 Hz  (same physical frequency)
        //   b_proxy = b_full * SCOUT_DOWNSAMPLE
        //
        // Bins above proxy Nyquist (~6kHz): filled with EPS.
        // NMF will treat them as "no template" — leaves them untouched.
        // INV-AB-1: deterministic — integer mapping, no rounding.
        // ── W-matrix energy averaging: proxy (12kHz) → full (48kHz) ─
        // proxy bin spacing: 48000 / (4 × 2048) = 5.86 Hz/bin
        // full  bin spacing: 48000 / 2048       = 23.44 Hz/bin
        // SCOUT_DOWNSAMPLE=4: average 4 proxy bins → 1 full bin
        // Preserves micro-harmonic energy — no information lost below ~6kHz
        // Bins above proxy Nyquist (~6kHz): filled with EPS (no template)
        // INV-AB-1: deterministic — integer arithmetic only
        let k = N_COMPONENTS;
        let mut w_full = vec![1e-10_f32; N_BINS * k];
        for c in 0..k {
            for b_full in 0..N_BINS {
                let proxy_start = b_full * SCOUT_DOWNSAMPLE;
                if proxy_start < N_BINS {
                    let mut sum = 0.0_f32;
                    let mut count = 0usize;
                    for offset in 0..SCOUT_DOWNSAMPLE {
                        let b_proxy = proxy_start + offset;
                        if b_proxy < N_BINS {
                            sum += w_proxy[b_proxy * k + c];
                            count += 1;
                        }
                    }
                    if count > 0 {
                        w_full[b_full * k + c] = (sum / count as f32).max(1e-10_f32);
                    }
                }
                // proxy_start >= N_BINS → above ~6kHz → remains EPS
            }
        }
        let w = w_full.clone();
        self.nmf.w = w_full;

        // ── Semantic assignment from W ───────────────────────────────
        let n_bins = N_BINS;
        let mut flatness = [0.0f32; N_COMPONENTS];
        for c in 0..N_COMPONENTS {
            let mut log_sum = 0.0f32;
            let mut arith = 0.0f32;
            let eps = 1e-10f32;
            for b in 0..n_bins {
                let w_val = w_proxy[b * N_COMPONENTS + c];
                log_sum += libm::logf(w_val + eps);
                arith += w_val;
            }
            let geom = libm::expf(log_sum / n_bins as f32);
            let mean = arith / n_bins as f32;
            flatness[c] = if mean > eps {
                (geom / mean).clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        // Use proxy W for centroids — relative frequency ordering preserved
        // (proxy bins have correct relative ordering even at 12kHz)
        let w_saved = self.nmf.w.clone();
        self.nmf.w = w_proxy.clone();
        let centroids = self.nmf.centroids(n_bins);
        self.nmf.w = w_saved;

        let ambience_idx = (0..N_COMPONENTS)
            .max_by(|&a, &b| flatness[a].partial_cmp(&flatness[b]).unwrap())
            .unwrap_or(0);
        let remaining: Vec<usize> = (0..N_COMPONENTS).filter(|&i| i != ambience_idx).collect();

        // Sort the remaining 4 components by spectral centroid (lowest to highest)
        let mut sorted_by_centroid = remaining.clone();
        sorted_by_centroid.sort_by(|&a, &b| centroids[a].partial_cmp(&centroids[b]).unwrap());

        // Semantic mapping based on frequency ordering
        let bass_idx = sorted_by_centroid[0]; // Lowest centroid
        let drums_idx = sorted_by_centroid[1]; // 2nd lowest centroid
        let harmonics_idx = sorted_by_centroid[2]; // Mid/High frequencies
        let voice_idx = sorted_by_centroid[3]; // Highest centroid

        // ── Proxy stems for spatial + feature analysis ───────────────
        let proxy_h = self.nmf.transform(&w, &proxy_frames);
        self.nmf.h = proxy_h;

        let proxy_n = proxy_frames.len();
        let t_extraction = std::time::Instant::now();
        let proxy_voice = self.proxy_stem(voice_idx, proxy_n, n_bins, &proxy);
        let proxy_drums = self.proxy_stem(drums_idx, proxy_n, n_bins, &proxy);
        let proxy_bass = self.proxy_stem(bass_idx, proxy_n, n_bins, &proxy);
        let proxy_harm = self.proxy_stem(harmonics_idx, proxy_n, n_bins, &proxy);
        let proxy_amb = self.proxy_stem(ambience_idx, proxy_n, n_bins, &proxy);
        eprintln!(
            "[PERF] proxy_stem_extraction={}ms",
            t_extraction.elapsed().as_millis()
        );

        // M-P2: Compute per-stem MFCC fingerprints from proxy stems.
        // Only first 4096 samples (~85ms) — keeps scout() latency minimal.
        // 4096 samples is sufficient for timbral fingerprint.
        // INV-AB-1: deterministic — same proxy → same MFCCs.
        // M-P2: Compute per-stem MFCC fingerprints.
        // Proxy stems are now exactly the 2-second chorus window passed into scout().
        // INV-AB-1: deterministic — same slice → same MFCCs.
        let stem_mfccs = {
            let mut mfcc = MfccAnalyzer::new();
            StemMfccs {
                voice: mfcc.compute(&proxy_voice),
                drums: mfcc.compute(&proxy_drums),
                bass: mfcc.compute(&proxy_bass),
                harmonics: mfcc.compute(&proxy_harm),
                ambience: mfcc.compute(&proxy_amb),
            }
        };

        // ΤΟ ΠΡΑΓΜΑΤΙΚΟ STEREO, downsampled όπως το proxy.
        //
        // ΗΤΑΝ: proxy_voice.clone() δύο φορές. Το analyze
        // μετράει side = (l−r)*0.5, οπότε με ταυτόσημα
        // κανάλια το side ήταν μηδέν και το ms_ratio σταθερά
        // 0.0 — μετρημένο 2026-08-12.
        //
        // Το SCOUT_DOWNSAMPLE εφαρμόζεται ΚΑΙ ΕΔΩ ώστε το
        // sample_rate που περνάει παρακάτω να αντιστοιχεί:
        // τα IIR φίλτρα των 80 Hz μέσα στο analyze
        // υπολογίζουν alpha από αυτό.
        let spatial_l: Vec<f32> =
            left.iter().step_by(SCOUT_DOWNSAMPLE).copied().collect();
        let spatial_r: Vec<f32> =
            right.iter().step_by(SCOUT_DOWNSAMPLE).copied().collect();

        // scout-resident by design: outputs steer Pass-2 (rear/lfe scales); a full-file version is a Cycle-5 question
        let spatial_pre = SpatialPreAnalysis::analyze(
            &spatial_l,
            &spatial_r,
            sample_rate / SCOUT_DOWNSAMPLE as u32,
        );


        // StemFeatures from proxy
        let proxy_fivs = FiveStems {
            voice: proxy_voice,
            drums: proxy_drums,
            bass: proxy_bass,
            harmonics: proxy_harm,
            ambience: proxy_amb,
            voice_transient_density: 0.0,
            drums_transient_density: 0.0,
            bass_transient_density: 0.0,
            harmonics_transient_density: 0.0,
            ambience_transient_density: 0.0,
        };

        use crate::analysis::StemFeatureAnalyzer;
        // scout-resident by design: centroid/flatness feed corpus features; full-file spectral truth is TrunkMetrics' 8-band profile (Cycle 4b decision 2026-07-31)
        let features =
            StemFeatureAnalyzer::analyze(&proxy_fivs, sample_rate / SCOUT_DOWNSAMPLE as u32);

        let assignments = StemChannelAssignments::compute(&features, &spatial_pre);

        // Compute firewall scales from proxy energy (locked for Pass 2)
        let (rear_scale, lfe_scale) = compute_firewall_scales(&proxy_fivs, &assignments);

        // Transform is NOT explicit in scout, but wait...
        // The scout pass returns a ScoutResult. Where does nmf_transform happen?
        // Wait, fit() does fit_transform, which does both W and H learning.
        // It does NOT call transform(). Wait, two_pass.rs only calls fit()!
        eprintln!("[PERF] scout_total={}ms", t_scout.elapsed().as_millis());

        ScoutResult {
            w,
            tensor_w: nmfd_tensor_w,
            tau: nmfd_tau,
            proxy_rms,
            voice_idx,
            bass_idx,
            harmonics_idx,
            ambience_idx,
            nmfd_bass_idx,
            nmfd_harmonics_idx,
            nmfd_ambience_idx,
            assignments,
            rear_scale,
            lfe_scale,
            spatial_pre,
            stem_mfccs,
            features,
            proxy_voice: proxy_fivs.voice,
            proxy_drums: proxy_fivs.drums,
            proxy_bass: proxy_fivs.bass,
            proxy_harmonics: proxy_fivs.harmonics,
            proxy_ambience: proxy_fivs.ambience,
        }
    }

    /// Extract proxy stem from NMF component mask * proxy signal.
    fn proxy_stem(
        &self,
        component: usize,
        n_frames: usize,
        n_bins: usize,
        proxy: &[f32],
    ) -> Vec<f32> {
        let mask = self
            .nmf
            .component_mask_chunk(component, &self.nmf.h, n_frames, n_bins);
        apply_mask_to_chunk(proxy, &mask, n_frames)
    }

    // ── Pass 2 — process_chunks ──────────────────────────────────────

    fn should_bypass_nmf(
        offset: usize,
        chunk_frames: usize,
        sample_rate: f32,
        boundaries: &[SegmentBoundary],
    ) -> bool {
        let start_sec = offset as f32 / sample_rate;
        let end_sec = (offset + chunk_frames) as f32 / sample_rate;

        let mut found_any = false;
        for b in boundaries {
            let intersects = start_sec < b.end_sec && end_sec > b.start_sec;
            if intersects {
                found_any = true;
                if b.avg_confidence < 0.4 || b.segment_type != SegmentType::Speech {
                    return false;
                }
            }
        }
        found_any
    }

    /// Pass 2: chunk-by-chunk processing with locked ScoutResult.
    /// Callback receives FiveStemsChunk per chunk.
    /// All DSP context is stateful across chunks.
    /// INV-ST-2: W never modified.
    /// INV-ST-3: peak RAM ~2MB/chunk.
    /// Process with adaptive ducking_gain from Maestro.
    /// ducking_gain: [0.3, 1.0] — replaces COLLISION_DUCKING_GAIN.
    /// Use process_chunks for default behavior (ducking_gain=0.707).
    /// Process audio from a streaming `ChunkSource` using macro-batching.
    /// WHY NO SEAM RISK: Macro-batching introduces no seam risk because
    /// SlidingOverlapReader is one continuous stream. Batching is purely a
    /// Rayon parallelism grouping, not a reader boundary. The reader naturally
    /// carries its history buffer across macro-batch boundaries, and process_single_chunk
    /// relies purely on this history padding to warm up fresh DSP contexts per chunk.
    #[allow(clippy::too_many_arguments)]
    pub fn process_stream_with_params<S: crate::stft::sliding_overlap_reader::ChunkSource, F>(
        &mut self,
        mut reader: crate::stft::sliding_overlap_reader::SlidingOverlapReader<S>,
        scout: &ScoutResult,
        ducking_gain: f32,
        macro_router_enabled: bool,
        use_nmfd: bool,
        boundaries: &[SegmentBoundary],
        sample_rate: f32,
        noise_floor_dbfs: Option<f32>,
        mut vad_observer: Option<&mut dyn FnMut(crate::analysis::vad_model::VadObservation)>,
        phi1_enabled: bool,
        mut callback: F,
    ) -> Result<RenderMetadata, StreamError>
    where
        F: FnMut(&FiveStemsChunk),
    {
        use rayon::prelude::*;

        struct OwnedChunkData {
            padded_chunk: Vec<f32>,
            padded_left: Vec<f32>,
            padded_right: Vec<f32>,
            core_chunk: Vec<f32>,
            /// ΠΡΟΣΤΕΘΗΚΕ ΣΤΟ S1. Δεν διαβάζεται ακόμα.
            core_left: Vec<f32>,
            core_right: Vec<f32>,
            pad_frames: usize,
            offset: usize,
        }

        let macro_batch_size = rayon::current_num_threads() * 2;
        let mut frames_written = 0usize;
        let mut voice_transient_sum = 0.0f32;
        let mut drums_transient_sum = 0.0f32;
        let mut chunk_count = 0usize;
        let mut global_spatial_sums = [(0.0, 0.0, 0.0); 5];

        let mut vad = if vad_observer.is_some() {
            Some((
                crate::analysis::vad_features::VadFeatureExtractor::new(),
                crate::analysis::vad_model::VadClassifier::new(
                    crate::analysis::vad_model::FixedPriors,
                ),
            ))
        } else {
            None
        };
        let phi1_active = phi1_enabled || USE_NEURAL_VAD;
        let mut phi1 = if phi1_active && vad_observer.is_some() {
            Some((
                crate::analysis::phi1_sensor::Phi2StreamingFrontend::new(),
                crate::analysis::phi1_sensor::Phi2Pcen::new(),
                crate::analysis::phi1_sensor::Phi2Sensor::new(),
                Vec::<[f32; 64]>::new(),  // pending pcen frames
            ))
        } else { None };
        let mut phi1_queue: std::collections::VecDeque<f32> = Default::default();
        let mut phi1_carry: usize = 0;
        let mut pending_obs: Vec<(
            crate::analysis::vad_features::VadFeatures,
            crate::analysis::vad_model::VadDecision,
        )> = Vec::new();
        let mut vad_frame_index: u64 = 0;
        let noise_floor = noise_floor_dbfs.unwrap_or(-144.0);
        let mut t_read = 0u128;
        let mut t_vad_dsp = 0u128;
        let mut t_phi1 = 0u128;
        let mut t_stems = 0u128;
        let mut t_callback = 0u128;

        loop {
            // 1. Pull a macro-batch of owned chunks
            let mut batch = Vec::with_capacity(macro_batch_size);
            for _ in 0..macro_batch_size {
                let t0 = std::time::Instant::now();
                let chunk_res = reader.next_chunk(CHUNK_FRAMES);
                t_read += t0.elapsed().as_millis();
                match chunk_res {
                    Ok(Some(overlap_chunk)) => {
                        let pad_frames = if overlap_chunk.start < overlap_chunk.offset {
                            (overlap_chunk.offset - overlap_chunk.start + (FFT_SIZE / 2)) / HOP_SIZE
                        } else {
                            0
                        };
                        let new_len = overlap_chunk.end - overlap_chunk.offset;
                        let offset_idx = overlap_chunk.offset - overlap_chunk.start;

                        if let Some((ext, clf)) = vad.as_mut() {
                            let m = &overlap_chunk.signal[offset_idx..offset_idx + new_len];
                            let l = &overlap_chunk.left[offset_idx..offset_idx + new_len];
                            let r = &overlap_chunk.right[offset_idx..offset_idx + new_len];
                            debug_assert_eq!(
                                m.len(),
                                l.len(),
                                "Stage (b) Contract: mono and left must be aligned"
                            );
                            debug_assert_eq!(
                                l.len(),
                                r.len(),
                                "Stage (b) Contract: left and right must be aligned"
                            );
                            // τρέφουμε τον Φ1 — ΜΟΝΟ συσσώρευση pcen frames
                            let t1_phi1 = std::time::Instant::now();
                            if let Some((fe, pcen, _, pending)) = phi1.as_mut() {
                                for mel_pow in fe.push(m) {
                                    pending.push(pcen.process(&mel_pow));
                                }
                            }
                            t_phi1 += t1_phi1.elapsed().as_millis();

                            let t1_dsp = std::time::Instant::now();
                            for f in ext.process_chunk(m, l, r) {
                                let d = clf.process(&f, noise_floor);
                                pending_obs.push((f, d));
                            }
                            t_vad_dsp += t1_dsp.elapsed().as_millis();
                        }

                        batch.push(OwnedChunkData {
                            padded_chunk: overlap_chunk.signal.to_vec(),
                            padded_left: overlap_chunk.left.to_vec(),
                            padded_right: overlap_chunk.right.to_vec(),
                            core_chunk: overlap_chunk.signal[offset_idx..offset_idx + new_len]
                                .to_vec(),
                            core_left: overlap_chunk.left[offset_idx..offset_idx + new_len].to_vec(),
                            core_right: overlap_chunk.right[offset_idx..offset_idx + new_len].to_vec(),
                            pad_frames,
                            offset: overlap_chunk.offset,
                        });
                    }
                    Ok(None) => break, // EOF
                    Err(e) => return Err(StreamError::Io(e)),
                }
            }

            if batch.is_empty() {
                break; // EOF reached
            }

            // Batch inference for phi1 sensor
            let t1_phi1_batch = std::time::Instant::now();
            if let Some((_, _, sensor, pending)) = phi1.as_mut() {
                if pending.len() > phi1_carry {
                    let out = sensor.infer_batch(pending);
                    for p in out.into_iter().skip(phi1_carry) {
                        phi1_queue.push_back(p.unwrap_or(f32::NAN));
                    }
                    let keep = (crate::analysis::phi1_sensor::PHI1_CONTEXT_FRAMES - 1)
                        .min(pending.len());
                    let drop_n = pending.len() - keep;
                    pending.drain(0..drop_n);
                    phi1_carry = keep;
                }
            }
            t_phi1 += t1_phi1_batch.elapsed().as_millis();

            // Emit observations now that phi1_queue is populated
            let n_emit = if phi1.is_some() {
                pending_obs.len().min(phi1_queue.len())
            } else {
                pending_obs.len()
            };
            for (f, d) in pending_obs.drain(0..n_emit) {
                let phi1_p_value = phi1_queue.pop_front()
                    .filter(|p| p.is_finite());
                if let Some(obs) = vad_observer.as_deref_mut() {
                    obs(crate::analysis::vad_model::VadObservation {
                        frame_index: vad_frame_index,
                        posterior: if USE_NEURAL_VAD {
                            phi1_p_value.unwrap_or(d.posterior)
                        } else {
                            d.posterior
                        },
                        is_speech: d.is_speech,
                        duck_gain: d.duck_gain,
                        rms_db: f.rms_db,
                        spectral_flatness: f.spectral_flatness,
                        mid_side_ratio: f.mid_side_ratio,
                        rms_delta_30ms: d.rms_delta_30ms,
                        noise_floor_dbfs: noise_floor,
                        phi1_p: phi1_p_value,
                    });
                }
                vad_frame_index += 1;
            }

            // 2. Parallel Transform Phase (Heavy Math)
            let t2 = std::time::Instant::now();
            let parallel_results: Vec<ParallelChunkOut> = batch
                .into_par_iter()
                .map(|mut owned| {
                    if macro_router_enabled
                        && Self::should_bypass_nmf(
                            owned.offset,
                            owned.core_chunk.len(),
                            sample_rate,
                            boundaries,
                        )
                    {
                        let len = owned.core_chunk.len();
                        ParallelChunkOut {
                            stems: FiveStemsChunk {
                                voice: StereoStem {
                                    l: std::mem::take(&mut owned.core_left),
                                    r: std::mem::take(&mut owned.core_right),
                                },
                                drums: StereoStem { l: vec![0.0; len], r: vec![0.0; len] },
                                bass: StereoStem { l: vec![0.0; len], r: vec![0.0; len] },
                                harmonics: StereoStem { l: vec![0.0; len], r: vec![0.0; len] },
                                ambience: StereoStem { l: vec![0.0; len], r: vec![0.0; len] },
                            },
                            voice_transient: 0.0,
                            drums_transient: 0.0,
                            chunk_len: len,
                            spatial_sums: [(0.0, 0.0, 0.0); 5],
                        }
                    } else {
                        process_single_chunk(
                            &self.nmf,
                            scout,
                            SingleChunkData {
                                padded_chunk: &owned.padded_chunk,
                                padded_left: &owned.padded_left,
                                padded_right: &owned.padded_right,
                                core_chunk: &owned.core_chunk,
                                core_left: &owned.core_left,
                                core_right: &owned.core_right,
                                pad_frames: owned.pad_frames,
                                use_nmfd,
                            },
                        )
                    }
                })
                .collect();
            t_stems += t2.elapsed().as_millis();

            // 3. Serial Stitch Phase (Stateful processing + callback)
            for mut out in parallel_results {
                if out.chunk_len == 0 {
                    continue;
                }

                voice_transient_sum += out.voice_transient;
                drums_transient_sum += out.drums_transient;
                chunk_count += 1;

                for (g, s) in global_spatial_sums.iter_mut().zip(&out.spatial_sums) {
                    g.0 += s.0;
                    g.1 += s.1;
                    g.2 += s.2;
                }

                let tmp_drums = out.stems.drums.mono();
                let tmp_bass = out.stems.bass.mono();
                let collision = detect_collision(&tmp_drums, &tmp_bass);
                let target_gain = if collision { ducking_gain } else { 1.0_f32 };
                let alpha = COLLISION_SMOOTHING_ALPHA;

                for (sl, sr) in out.stems.bass.l.iter_mut().zip(out.stems.bass.r.iter_mut()) {
                    self.bass_ducking_gain += alpha * (target_gain - self.bass_ducking_gain);
                    *sl *= self.bass_ducking_gain;
                    *sr *= self.bass_ducking_gain;
                }

                frames_written += out.stems.voice.l.len();
                let t3 = std::time::Instant::now();
                callback(&out.stems);
                t_callback += t3.elapsed().as_millis();
            }
        }

        // EOF: emit any remaining observations
        for (f, d) in pending_obs.drain(..) {
            let phi1_p_value = phi1_queue.pop_front()
                .filter(|p| p.is_finite());
            if let Some(obs) = vad_observer.as_deref_mut() {
                obs(crate::analysis::vad_model::VadObservation {
                    frame_index: vad_frame_index,
                    posterior: if USE_NEURAL_VAD {
                        phi1_p_value.unwrap_or(d.posterior)
                    } else {
                        d.posterior
                    },
                    is_speech: d.is_speech,
                    duck_gain: d.duck_gain,
                    rms_db: f.rms_db,
                    spectral_flatness: f.spectral_flatness,
                    mid_side_ratio: f.mid_side_ratio,
                    rms_delta_30ms: d.rms_delta_30ms,
                    noise_floor_dbfs: noise_floor,
                    phi1_p: phi1_p_value,
                });
            }
            vad_frame_index += 1;
        }

        let avg = chunk_count.max(1) as f32;
        let mut final_spatial = [BandSpatialMetrics::default(); 5];
        for i in 0..5 {
            let den = global_spatial_sums[i].2.max(1e-10);
            final_spatial[i].pan_mean = global_spatial_sums[i].0 / den;
            final_spatial[i].pan_width = global_spatial_sums[i].1 / den;
        }

        eprintln!(
            "[PERF-RENDER] read={}ms vad_dsp={}ms vad_phi1={}ms stems={}ms callback={}ms",
            t_read, t_vad_dsp, t_phi1, t_stems, t_callback
        );

        eprintln!(
            "[BAND-WIDTH] lows={:.4} low_mid={:.4} mid={:.4} high_mid={:.4} high={:.4}",
            final_spatial[0].pan_width,
            final_spatial[1].pan_width,
            final_spatial[2].pan_width,
            final_spatial[3].pan_width,
            final_spatial[4].pan_width
        );
        eprintln!(
            "[BAND-MEAN] lows={:.4} low_mid={:.4} mid={:.4} high_mid={:.4} high={:.4}",
            final_spatial[0].pan_mean,
            final_spatial[1].pan_mean,
            final_spatial[2].pan_mean,
            final_spatial[3].pan_mean,
            final_spatial[4].pan_mean
        );

        Ok(RenderMetadata {
            frames_written,
            voice_transient_density: voice_transient_sum / avg,
            drums_transient_density: drums_transient_sum / avg,
            spatial: final_spatial,
        })
    }

    pub fn process_slices_with_params<F>(
        &mut self,
        signal: &[f32],
        left: &[f32],
        right: &[f32],
        scout: &ScoutResult,
        ducking_gain: f32,
        use_nmfd: bool,
        mut callback: F,
    ) -> Result<RenderMetadata, StreamError>
    where
        F: FnMut(&FiveStemsChunk),
    {
        if signal.is_empty() {
            return Err(StreamError::Empty);
        }

        let n_total = signal.len();
        use rayon::prelude::*;

        // 1. Prepare overlapping input chunks
        struct ChunkInput {
            start: usize,
            offset: usize,
            end: usize,
        }
        let mut chunk_inputs = Vec::new();
        let mut offset = 0usize;
        while offset < n_total {
            let end = (offset + CHUNK_FRAMES).min(n_total);
            let start = offset.saturating_sub(10240); // 10240 pure HPSS history (1024-sample encoder pad is handled in pad_frames)
            chunk_inputs.push(ChunkInput { start, offset, end });
            offset = end;
        }

        // 2. Parallel Transform Phase (Heavy Math)
        let parallel_results: Vec<ParallelChunkOut> = chunk_inputs
            .into_par_iter()
            .map(|chunk_in| {
                let padded_chunk = &signal[chunk_in.start..chunk_in.end];
                let padded_left = if chunk_in.start < left.len() {
                    let end = chunk_in.end.min(left.len());
                    &left[chunk_in.start..end]
                } else {
                    &[]
                };
                let padded_right = if chunk_in.start < right.len() {
                    let end = chunk_in.end.min(right.len());
                    &right[chunk_in.start..end]
                } else {
                    &[]
                };
                let core_chunk = &signal[chunk_in.offset..chunk_in.end];
                let core_left = &left[chunk_in.offset..chunk_in.end];
                let core_right = &right[chunk_in.offset..chunk_in.end];
                let pad_frames = if chunk_in.start < chunk_in.offset {
                    (chunk_in.offset - chunk_in.start + (FFT_SIZE / 2)) / HOP_SIZE
                } else {
                    0
                };

                process_single_chunk(
                    &self.nmf,
                    scout,
                    SingleChunkData {
                        padded_chunk,
                        padded_left,
                        padded_right,
                        core_chunk,
                        core_left,
                        core_right,
                        pad_frames,
                        use_nmfd,
                    },
                )
            })
            .collect();

        // 3. Serial Stitch Phase (Stateful processing + callback)
        let mut frames_written = 0usize;
        let mut voice_transient_sum = 0.0f32;
        let mut drums_transient_sum = 0.0f32;
        let mut chunk_count = 0usize;

        let mut global_spatial_sums = [(0.0, 0.0, 0.0); 5];

        for mut out in parallel_results {
            if out.chunk_len == 0 {
                continue;
            }

            voice_transient_sum += out.voice_transient;
            drums_transient_sum += out.drums_transient;
            chunk_count += 1;

            for (g, s) in global_spatial_sums.iter_mut().zip(&out.spatial_sums) {
                g.0 += s.0;
                g.1 += s.1;
                g.2 += s.2;
            }

            // Psychoacoustic Collision Matrix — smoothed micro-ducking sequentially across chunks.
            let tmp_drums = out.stems.drums.mono();
            let tmp_bass = out.stems.bass.mono();
            let collision = detect_collision(&tmp_drums, &tmp_bass);
            let target_gain = if collision { ducking_gain } else { 1.0_f32 };
            let alpha = COLLISION_SMOOTHING_ALPHA;

            for (sl, sr) in out.stems.bass.l.iter_mut().zip(out.stems.bass.r.iter_mut()) {
                self.bass_ducking_gain += alpha * (target_gain - self.bass_ducking_gain);
                *sl *= self.bass_ducking_gain;
                *sr *= self.bass_ducking_gain;
            }

            frames_written += out.stems.voice.l.len();
            callback(&out.stems);
        }

        let avg = chunk_count.max(1) as f32;
        let mut final_spatial = [BandSpatialMetrics::default(); 5];
        for i in 0..5 {
            let den = global_spatial_sums[i].2.max(1e-10);
            final_spatial[i].pan_mean = global_spatial_sums[i].0 / den;
            final_spatial[i].pan_width = global_spatial_sums[i].1 / den;
        }

        eprintln!(
            "[BAND-WIDTH] lows={:.4} low_mid={:.4} mid={:.4} high_mid={:.4} high={:.4}",
            final_spatial[0].pan_width,
            final_spatial[1].pan_width,
            final_spatial[2].pan_width,
            final_spatial[3].pan_width,
            final_spatial[4].pan_width
        );
        eprintln!(
            "[BAND-MEAN] lows={:.4} low_mid={:.4} mid={:.4} high_mid={:.4} high={:.4}",
            final_spatial[0].pan_mean,
            final_spatial[1].pan_mean,
            final_spatial[2].pan_mean,
            final_spatial[3].pan_mean,
            final_spatial[4].pan_mean
        );

        Ok(RenderMetadata {
            frames_written,
            voice_transient_density: voice_transient_sum / avg,
            drums_transient_density: drums_transient_sum / avg,
            spatial: final_spatial,
        })
    }

    /// Default process_chunks — uses COLLISION_DUCKING_GAIN (0.707).
    /// Backward compatible with all existing callers.
    pub fn process_chunks<F>(
        &mut self,
        signal: &[f32],
        scout: &ScoutResult,
        use_nmfd: bool,
        callback: F,
    ) -> Result<RenderMetadata, StreamError>
    where
        F: FnMut(&FiveStemsChunk),
    {
        self.process_slices_with_params(
            signal,
            signal,
            signal,
            scout,
            COLLISION_DUCKING_GAIN,
            use_nmfd,
            callback,
        )
    }
}

impl Default for TwoPassEngine {
    fn default() -> Self {
        Self::new()
    }
}

// ── Helpers ───────────────────────────────────────────────────────────

/// Compute firewall scales from proxy FiveStems energy.
/// Returns (rear_scale, lfe_scale) — locked in ScoutResult.
fn compute_firewall_scales(stems: &FiveStems, assignments: &StemChannelAssignments) -> (f32, f32) {
    use crate::spatial::five_dot_one::{FiveDotOneStage, SpatialFirewall};
    let stage = FiveDotOneStage::render(stems, assignments, &SpatialFirewall::default());
    let firewall = SpatialFirewall::default();

    let len = stage.l.len();
    if len == 0 {
        return (1.0, 1.0);
    }

    // Compute rear scale
    let front_e_sq: f32 = stage
        .l
        .iter()
        .zip(stage.r.iter())
        .zip(stage.c.iter())
        .map(|((l, r), c)| l * l + r * r + c * c)
        .sum::<f32>();
    let front_e = libm::sqrtf(front_e_sq);
    let rear_e_sq: f32 = stage
        .ls
        .iter()
        .zip(stage.rs.iter())
        .map(|(l, r)| l * l + r * r)
        .sum::<f32>();
    let rear_e = libm::sqrtf(rear_e_sq);
    let rear_scale = if front_e > 1e-6 && rear_e > 1e-6 {
        let ratio = rear_e / front_e;
        if ratio > firewall.max_rear_energy {
            firewall.max_rear_energy / ratio
        } else {
            1.0
        }
    } else {
        1.0
    };

    // Compute lfe scale
    let max_lfe_linear = libm::powf(10.0, firewall.max_lfe_db / 20.0);
    let lfe_max = stage.lfe.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    let lfe_scale = if lfe_max > max_lfe_linear {
        max_lfe_linear / lfe_max
    } else {
        1.0
    };

    (rear_scale, lfe_scale)
}

/// Apply per-frame magnitude mask to time-domain chunk.
fn apply_mask_to_chunk(chunk: &[f32], mask: &[Vec<f32>], n_frames: usize) -> Vec<f32> {
    if n_frames == 0 || mask.is_empty() || chunk.is_empty() {
        return chunk.to_vec();
    }
    let frame_weights: Vec<f32> = mask
        .iter()
        .map(|frame| {
            if frame.is_empty() {
                0.0
            } else {
                frame.iter().sum::<f32>() / frame.len() as f32
            }
        })
        .collect();
    let n = chunk.len();
    let weights: Vec<f32> = (0..n)
        .map(|i| {
            let pos = i as f32 * (n_frames - 1).max(1) as f32 / n.max(1) as f32;
            let idx0 = pos.floor() as usize;
            let idx1 = (idx0 + 1).min(frame_weights.len() - 1);
            let frac = pos - idx0 as f32;
            let w0 = frame_weights.get(idx0).copied().unwrap_or(0.0);
            let w1 = frame_weights.get(idx1).copied().unwrap_or(0.0);
            w0 * (1.0 - frac) + w1 * frac
        })
        .collect();
    chunk
        .iter()
        .zip(weights.iter())
        .map(|(s, w)| s * w)
        .collect()
}

/// Cross-chunk OLA state for spectral reconstruction.
/// Size: FFT_SIZE/2 = 1024 f32 = 4096 bytes per stem.
/// One instance per stem that uses spectral masking.
pub struct SpectralOlaState {
    /// Tail from previous chunk's iSTFT output, to be added
    /// to the start of the next chunk's output.
    pub overlap: Vec<f32>,
}

impl SpectralOlaState {
    pub fn new() -> Self {
        Self {
            overlap: vec![0.0_f32; FFT_SIZE / 2],
        }
    }

    /// Mix previous overlap into `output`, save the new tail.
    /// Returns the clean, overlap-corrected samples.
    pub fn process(&mut self, output: &[f32]) -> Vec<f32> {
        let overlap_len = self.overlap.len();
        let out_len = output.len();
        let mut result = output.to_vec();

        let mix_len = overlap_len.min(out_len);
        for (r, o) in result[..mix_len].iter_mut().zip(&self.overlap[..mix_len]) {
            *r += o;
        }

        if out_len >= overlap_len {
            self.overlap
                .copy_from_slice(&result[out_len - overlap_len..]);
            result.truncate(out_len - overlap_len);
        } else {
            self.overlap[..out_len].copy_from_slice(&result);
            self.overlap[out_len..].fill(0.0_f32);
            result.clear();
        }
        result
    }

    pub fn flush(&self) -> Vec<f32> {
        self.overlap.clone()
    }
}

/// Spectral reconstruction: Y_stem[k,t] = mask[k,t] * X[k,t], then iSTFT.
///
/// Unlike `apply_mask_to_chunk` (which collapses each mask frame to a single
/// broadband scalar), this preserves per-bin spectral separation through
/// proper complex masking + inverse STFT + overlap-add.
///
/// ## STFT parameters (must match analysis side):
///   FFT_SIZE = 2048, HOP_SIZE = 512, N_BINS = 1025
///   Window: periodic Hann, w[n] = 0.5 - 0.5*cos(2πn/N)
///
/// ## COLA verification (periodic Hann, R=512, N=2048, overlap=75%):
///   Σ_m w²[n - mR] = constant for all n.
///   For periodic Hann with N/R = 4 (75% overlap):
///     w²[n] + w²[n-R] + w²[n-2R] + w²[n-3R]
///     = 0.25*(1-cos(θ))² summed at 4 phases spaced π/2 apart
///     = 4 * 3/8 = 3/2 = 1.5  (proven by trig identity)
///   StftEngine::inverse normalizes by window_sum = Σ w²,
///   so the WOLA denominator is 1.5 everywhere in steady state.
///
/// ## Memory per chunk:
///   Complex STFT bins: n_frames * N_BINS * 8 bytes (Complex<f32>)
///   For CHUNK_FRAMES=65536: n_frames ≈ (65536 + 2*1024) / 512 ≈ 132
///   → 132 * 1025 * 8 ≈ 1.08 MB per complex STFT
///   Masked copy: same → ~1.08 MB
///   Total ~2.2 MB per stem call (transient, freed after iSTFT)
///
/// ## Cross-chunk state:
///   SpectralOlaState holds 1024 f32 = 4 KB overlap tail per stem.
pub fn apply_spectral_mask_to_chunk(
    complex_frames: &[Vec<rustfft::num_complex::Complex<f32>>],
    mask: &[Vec<f32>],
    output_len: usize,
) -> Vec<f32> {
    use rustfft::num_complex::Complex;

    let n_frames = complex_frames.len();
    if n_frames == 0 || mask.is_empty() || output_len == 0 {
        return vec![0.0; output_len];
    }

    // Apply mask in spectral domain: Y[k,t] = mask[k,t] * X[k,t]
    let masked_frames: Vec<Vec<Complex<f32>>> = complex_frames
        .iter()
        .enumerate()
        .map(|(t, frame)| {
            let mask_frame = if t < mask.len() {
                &mask[t]
            } else {
                &mask[mask.len() - 1]
            };
            frame
                .iter()
                .enumerate()
                .map(|(b, &x)| {
                    let m = if b < mask_frame.len() {
                        mask_frame[b]
                    } else {
                        0.0
                    };
                    Complex::new(x.re * m, x.im * m)
                })
                .collect()
        })
        .collect();

    // iSTFT via StftEngine::inverse (WOLA with window² normalization)
    let mut engine = StftEngine::new();
    engine.inverse(&masked_frames, output_len)
}

/// Compute transient density — deterministic, INV-AB-1.
fn transient_density(h: &[f32]) -> f32 {
    let n = h.len();
    if n < 2 {
        return 0.0;
    }
    let deltas: Vec<f32> = (1..n).map(|i| (h[i] - h[i - 1]).abs()).collect();
    let mean = deltas.iter().sum::<f32>() / deltas.len() as f32;
    let var = deltas.iter().map(|d| (d - mean).powi(2)).sum::<f32>() / deltas.len() as f32;
    let thr = mean + libm::sqrtf(var);
    deltas.iter().filter(|&&d| d > thr).count() as f32 / n as f32
}

// ── Tests ─────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collision_matrix_rms_detection() {
        // Alternating signal: high variance = transient-like
        // diff between adjacent samples is always 1.0 → high drums_td
        let loud_chunk: Vec<f32> = (0..512)
            .map(|i| if i % 2 == 0 { 0.5f32 } else { -0.5f32 })
            .collect();
        // Amplitude 0.0001 → RMS ≈ -80 dBFS (below threshold)
        let quiet_chunk = vec![0.0001f32; 512];

        // Scenario 1: Both loud → collision expected
        assert!(
            super::detect_collision(&loud_chunk, &loud_chunk),
            "Should detect collision when both drums and bass have high RMS"
        );
        // Scenario 2: Loud drums, quiet bass → no collision
        assert!(
            !super::detect_collision(&loud_chunk, &quiet_chunk),
            "Should NOT detect collision when bass is quiet"
        );
        // Scenario 3: Quiet drums, loud bass → no collision
        assert!(
            !super::detect_collision(&quiet_chunk, &loud_chunk),
            "Should NOT detect collision when drums transient is low"
        );
        // Scenario 4: Empty chunks → safe, no panic
        assert!(
            !super::detect_collision(&[], &loud_chunk),
            "Should safely return false on empty chunks"
        );
    }

    fn sine(freq: f32, n: usize) -> Vec<f32> {
        (0..n)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * freq * i as f32 / 48000.0))
            .collect()
    }

    #[test]
    fn w_bin_mapping_produces_full_size_w() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);
        assert_eq!(
            scout.w.len(),
            N_BINS * N_COMPONENTS,
            "W must be N_BINS × N_COMPONENTS after bin mapping"
        );
        assert!(
            scout.w.iter().all(|&v| v >= 1e-10_f32),
            "All W values must be >= EPS"
        );
    }

    #[test]
    fn w_bin_mapping_is_deterministic() {
        // INV-AB-1: same signal → same mapped W
        let signal = sine(440.0, 48000);
        let mut e1 = TwoPassEngine::new();
        let mut e2 = TwoPassEngine::new();
        let s1 = e1.scout(&signal, &signal, 48000, None, None, false);
        let s2 = e2.scout(&signal, &signal, 48000, None, None, false);
        for (a, b) in s1.w.iter().zip(s2.w.iter()) {
            assert!(
                (a - b).abs() < 1e-10,
                "INV-AB-1: W must be bit-identical for same input"
            );
        }
    }

    #[test]
    fn w_bins_above_6khz_are_eps() {
        // Bins above proxy Nyquist must be EPS (no template)
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);
        // Proxy Nyquist = 48000 / (2 * SCOUT_DOWNSAMPLE) = 6000 Hz
        // Bin at 6kHz = 6000 * N_BINS * 2 / 48000 = ~256
        for b in N_BINS.div_ceil(SCOUT_DOWNSAMPLE)..N_BINS {
            for c in 0..N_COMPONENTS {
                let v = scout.w[b * N_COMPONENTS + c];
                assert!(
                    v <= 1e-10_f32 + 1e-12_f32,
                    "Bin {b} component {c} should be EPS, got {v}"
                );
            }
        }
    }

    #[test]
    fn scout_produces_locked_assignments() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);
        assert_eq!(scout.w.len(), N_BINS * N_COMPONENTS);
        assert!(scout.voice_idx < N_COMPONENTS);
        assert!(scout.rear_scale >= 0.0 && scout.rear_scale <= 1.0);
        assert!(scout.lfe_scale >= 0.0 && scout.lfe_scale <= 1.0);
    }

    #[test]
    fn scout_is_deterministic() {
        let signal = sine(1000.0, 48000);
        let mut e1 = TwoPassEngine::new();
        let mut e2 = TwoPassEngine::new();
        let s1 = e1.scout(&signal, &signal, 48000, None, None, false);
        let s2 = e2.scout(&signal, &signal, 48000, None, None, false);
        for (a, b) in s1.w.iter().zip(s2.w.iter()) {
            assert!((a - b).abs() < 1e-6, "INV-AB-1: W must be identical");
        }
        assert_eq!(s1.voice_idx, s2.voice_idx);
        assert_eq!(s1.rear_scale, s2.rear_scale);
    }

    #[test]
    fn process_chunks_produces_callback_calls() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);

        let mut call_count = 0usize;
        let result = engine.process_chunks(&signal, &scout, false, |chunk| {
            call_count += 1;
            assert!(!chunk.voice.l.is_empty());
            assert_eq!(chunk.voice.l.len(), chunk.bass.l.len());
        });

        assert!(result.is_ok());
        assert!(call_count > 0, "Must call callback at least once");
    }

    #[test]
    fn w_read_only_during_process() {
        let signal = sine(440.0, 48000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);
        let w_before = scout.w.clone();
        let _ = engine.process_chunks(&signal, &scout, false, |_| {});
        assert_eq!(w_before, scout.w, "INV-ST-2: W must not change");
    }

    #[test]
    fn five_stems_chunk_all_same_length() {
        let signal = sine(440.0, 96000);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);
        let _ = engine.process_chunks(&signal, &scout, false, |chunk| {
            assert_eq!(chunk.voice.l.len(), chunk.drums.l.len());
            assert_eq!(chunk.voice.l.len(), chunk.bass.l.len());
            assert_eq!(chunk.voice.l.len(), chunk.harmonics.l.len());
            assert_eq!(chunk.voice.l.len(), chunk.ambience.l.len());
        });
    }

    #[test]
    fn test_no_hardcoded_tail_flush() {
        let n_total = 48000;
        let signal = sine(440.0, n_total);
        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);

        let mut total_output_samples = 0;
        let meta = engine
            .process_slices_with_params(&signal, &signal, &signal, &scout, 1.0, false, |chunk| {
                total_output_samples += chunk.voice.l.len();
            })
            .unwrap();

        assert_eq!(
            total_output_samples, n_total,
            "Total output samples ({}) must exactly match input samples ({}) with no legacy OLA tail",
            total_output_samples, n_total
        );
        assert_eq!(meta.frames_written, n_total);
    }

    #[test]
    fn test_two_pass_streaming_stft_boundary_frames_unpolluted() {
        let history_len = 10240;
        let real_len = 20000;
        let mut signal = vec![0.0f32; history_len + real_len];

        // 100Hz for history
        for (i, val) in signal.iter_mut().enumerate().take(history_len) {
            *val = libm::sinf(2.0 * core::f32::consts::PI * 100.0 * i as f32 / 48000.0);
        }
        // 2000Hz for real data
        for i in 0..real_len {
            signal[history_len + i] =
                libm::sinf(2.0 * core::f32::consts::PI * 2000.0 * i as f32 / 48000.0);
        }

        use crate::stft::StreamingStftEncoder;
        use crate::stft::{FFT_SIZE, HOP_SIZE};

        let mut encoder = StreamingStftEncoder::new();
        let mut frames_cplx = encoder.feed_chunk(&signal);
        frames_cplx.extend(encoder.finish());

        let frames: Vec<Vec<f32>> = frames_cplx
            .into_iter()
            .map(|f| {
                f.into_iter()
                    .map(|c| libm::sqrtf(c.re * c.re + c.im * c.im))
                    .collect()
            })
            .collect();

        let pad_frames = (history_len + FFT_SIZE / 2) / HOP_SIZE;
        let core_frames = &frames[pad_frames..];

        let frame0 = &core_frames[0];

        let mut energy_100hz = 0.0;
        for &val in frame0.iter().take(6 + 1).skip(2) {
            energy_100hz += val;
        }
        let mut energy_2000hz = 0.0;
        for &val in frame0.iter().take(87 + 1).skip(83) {
            energy_2000hz += val;
        }

        assert!(
            energy_100hz < 1.0,
            "History (100Hz) leaked into first core frame! Energy: {}",
            energy_100hz
        );
        assert!(
            energy_2000hz > 100.0,
            "Real data (2000Hz) missing from first core frame! Energy: {}",
            energy_2000hz
        );
    }

    /// Verifies that the spectrum at chunk boundaries is NOT polluted by
    /// STFT zero-padding artifacts. Each chunk is fed to a fresh
    /// StreamingStftEncoder; results must be bit-identical (f32 ==) to
    /// StftEngine::forward() on the same chunk, since StreamingStftEncoder
    /// is proven bit-identical to forward().
    #[test]
    fn two_pass_streaming_stft_boundary_frames_unpolluted() {
        use crate::stft::{StftEngine, StreamingStftEncoder, FFT_SIZE, HOP_SIZE, N_BINS};

        // 2*FFT_SIZE + HOP_SIZE samples of a single tone — spectrum easy to inspect.
        let n = 2 * FFT_SIZE + HOP_SIZE;
        let signal: Vec<f32> = (0..n)
            .map(|i| libm::sinf(2.0 * core::f32::consts::PI * 440.0 * i as f32 / 48000.0))
            .collect();

        // Three chunks: FFT_SIZE, FFT_SIZE, HOP_SIZE — boundaries at those indices.
        let chunks: [&[f32]; 3] = [
            &signal[..FFT_SIZE],
            &signal[FFT_SIZE..2 * FFT_SIZE],
            &signal[2 * FFT_SIZE..],
        ];

        // Offline oracle per chunk: StftEngine::forward(chunk) → magnitude.
        let offline: Vec<Vec<Vec<f32>>> = chunks
            .iter()
            .map(|chunk| {
                let mut engine = StftEngine::new();
                let (complex_frames, n_frames) = engine.forward(chunk);
                complex_frames
                    .into_iter()
                    .take(n_frames)
                    .map(|frame| {
                        frame
                            .iter()
                            .take(N_BINS)
                            .map(|c| c.norm())
                            .collect::<Vec<f32>>()
                    })
                    .collect()
            })
            .collect();

        // Streaming per-chunk encoder — same pattern as the new two_pass.rs code.
        let streaming: Vec<Vec<Vec<f32>>> = chunks
            .iter()
            .map(|chunk| {
                let mut enc = StreamingStftEncoder::new();
                let mut frames = enc.feed_chunk(chunk);
                frames.extend(enc.finish());
                frames
                    .into_iter()
                    .map(|frame| {
                        frame
                            .iter()
                            .take(N_BINS)
                            .map(|c| c.norm())
                            .collect::<Vec<f32>>()
                    })
                    .collect()
            })
            .collect();

        // Strict f32 bit-pattern equality — no tolerance.
        // Any discrepancy means a boundary frame is polluted.
        for (ci, (off_chunk, str_chunk)) in offline.iter().zip(streaming.iter()).enumerate() {
            assert_eq!(
                off_chunk.len(),
                str_chunk.len(),
                "chunk {ci}: frame count mismatch (offline={} streaming={})",
                off_chunk.len(),
                str_chunk.len()
            );
            for (fi, (off_frame, str_frame)) in off_chunk.iter().zip(str_chunk.iter()).enumerate() {
                for (bi, (o, s)) in off_frame.iter().zip(str_frame.iter()).enumerate() {
                    assert_eq!(
                        o.to_bits(),
                        s.to_bits(),
                        "chunk={ci} frame={fi} bin={bi}: offline={o} streaming={s}"
                    );
                }
            }
        }

        // Chunk sizes used — suppress unused warning.
        let _ = HOP_SIZE;
    }

    #[test]
    fn macro_router_bypasses_nmf_on_high_conf_speech() {
        let n_total = CHUNK_FRAMES * 2;
        let left: Vec<f32> = (0..n_total)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let freq = 100.0 + (100.0 * t);
                libm::sinf(2.0 * core::f32::consts::PI * freq * t)
            })
            .collect();
        let right: Vec<f32> = (0..n_total)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let freq = 200.0 + (200.0 * t);
                libm::sinf(2.0 * core::f32::consts::PI * freq * t)
            })
            .collect();
        let signal: Vec<f32> = left
            .iter()
            .zip(right.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();
        let mut interleaved = Vec::with_capacity(n_total * 2);
        for i in 0..n_total {
            interleaved.push(left[i]);
            interleaved.push(right[i]);
        }

        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);
        let source = TestMemorySource {
            data: interleaved,
            offset: 0,
        };
        use crate::stft::sliding_overlap_reader::SlidingOverlapReader;
        let reader = SlidingOverlapReader::new(source, 10240);

        let boundaries = vec![
            SegmentBoundary {
                start_sec: 0.0,
                end_sec: 2.0,
                segment_type: SegmentType::Speech,
                avg_leaning: 0.0,
                avg_confidence: 0.8,
            },
            SegmentBoundary {
                start_sec: 2.0,
                end_sec: 4.0,
                segment_type: SegmentType::Music,
                avg_leaning: 0.0,
                avg_confidence: 0.9,
            },
        ];

        let mut captured_chunks = Vec::new();
        engine
            .process_stream_with_params(
                reader,
                &scout,
                1.0,
                true,
                false,
                &boundaries,
                48000.0,
                None,
                None::<&mut dyn FnMut(_)>,
                false,
                |stems| {
                    captured_chunks.push((
                        stems.voice.clone(),
                        stems.drums.clone(),
                        stems.bass.clone(),
                    ));
                },
            )
            .unwrap();

        assert_eq!(captured_chunks.len(), 2);

        // Το signal είναι το mono των left/right, άρα η
        // σύγκριση θέλει mono(). ΚΑΙ ΤΟ TEST ΕΓΙΝΕ
        // ΙΣΧΥΡΟΤΕΡΟ: πριν επιβεβαίωνε ότι το bypass
        // περνάει το σήμα αυτούσιο· τώρα επιβεβαιώνει
        // ΕΠΙΠΛΕΟΝ ότι τα δύο κανάλια ανακατασκευάζουν
        // το mono.
        let chunk1_voice = captured_chunks[0].0.mono();
        let chunk1_drums = captured_chunks[0].1.mono();
        let chunk1_bass  = captured_chunks[0].2.mono();
        assert_eq!(chunk1_voice.len(), CHUNK_FRAMES);
        assert_eq!(chunk1_voice[100], signal[100]); // raw value (with pad offset skipped by reader output)
        assert_eq!(chunk1_drums[100], 0.0); // drums zeroed
        assert_eq!(chunk1_bass[100], 0.0); // bass zeroed

        let chunk2_voice = captured_chunks[1].0.mono();
        let chunk2_drums = captured_chunks[1].1.mono();
        assert_eq!(chunk2_voice.len(), CHUNK_FRAMES);
        assert_ne!(chunk2_voice[100], signal[CHUNK_FRAMES + 100]); // nmf ran
        assert_ne!(chunk2_drums[100], 0.0); // drums has signal
    }

    struct TestMemorySource {
        data: Vec<f32>,
        offset: usize,
    }

    impl crate::stft::sliding_overlap_reader::ChunkSource for TestMemorySource {
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
    #[ignore = "slow (8s): proves macro-batch Rayon boundaries do not drop/duplicate history. Run manually via --ignored"]
    fn test_streaming_seam_macro_batch() {
        // rayon::current_num_threads() = 8 on this test machine (as probed earlier).
        // macro_batch_size = 16. CHUNK_FRAMES = 81920.
        // A single macro batch covers 1,310,720 frames (about 27 seconds).
        // To span 2-3 macro-batch boundaries, we need > 2,621,440 frames.
        // We'll use 3,000,000 frames (62.5 seconds of audio) to guarantee
        // at least two Rayon macro-batch boundaries are crossed during processing.

        let n_total = 3_000_000;

        // DISTINGUISHABLE CONTENT for L/R
        let left: Vec<f32> = (0..n_total)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let freq = 100.0 + (100.0 * t);
                libm::sinf(2.0 * core::f32::consts::PI * freq * t)
            })
            .collect();

        let right: Vec<f32> = (0..n_total)
            .map(|i| {
                let t = i as f32 / 48000.0;
                let freq = 200.0 + (200.0 * t);
                libm::sinf(2.0 * core::f32::consts::PI * freq * t)
            })
            .collect();

        let signal: Vec<f32> = left
            .iter()
            .zip(right.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();

        let mut interleaved = Vec::with_capacity(n_total * 2);
        for i in 0..n_total {
            interleaved.push(left[i]);
            interleaved.push(right[i]);
        }

        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);

        // Run the OLD path to establish the exact reference
        let mut old_voice: Vec<f32> = Vec::with_capacity(n_total);
        let _ = engine
            .process_slices_with_params(&signal, &left, &right, &scout, 1.0, false, |chunk| {
                old_voice.extend_from_slice(&chunk.voice.mono());
            })
            .unwrap();

        // Reset state
        engine.bass_ducking_gain = 1.0;

        // Run the NEW path
        let mut new_voice: Vec<f32> = Vec::with_capacity(n_total);
        use crate::stft::sliding_overlap_reader::SlidingOverlapReader;
        let source = TestMemorySource {
            data: interleaved,
            offset: 0,
        };
        let reader = SlidingOverlapReader::new(source, 10240);

        let _ = engine
            .process_stream_with_params(
                reader,
                &scout,
                1.0,
                false,
                false,
                &[],
                48000.0,
                None,
                None::<&mut dyn FnMut(_)>,
                false,
                |chunk| {
                    new_voice.extend_from_slice(&chunk.voice.mono());
                },
            )
            .unwrap();

        assert_eq!(
            new_voice.len(),
            n_total,
            "Seam test lost or duplicated frames!"
        );
        assert_eq!(old_voice.len(), n_total, "Reference lost frames!");

        // This is the critical proof: by comparing every single float of the 3 million frame
        // output against the reference O(N) slice path, we prove that crossing multiple
        // macro-batch boundaries (at Rayon thread boundaries) does not drop, duplicate,
        // or misalign any DSP state/history.
        for i in 0..n_total {
            assert_eq!(
                old_voice[i].to_bits(),
                new_voice[i].to_bits(),
                "Bit mismatch at frame {} (macro-batch seam glitch!)",
                i
            );
        }
    }

    #[test]
    fn oracle_vad_observe_only() {
        let n_total = 25600; // 50 chunks of 512 roughly

        let left: Vec<f32> = (0..n_total).map(|i| (i as f32 * 0.1).sin()).collect();
        let right: Vec<f32> = (0..n_total).map(|i| (i as f32 * 0.2).cos()).collect();
        let signal: Vec<f32> = left
            .iter()
            .zip(right.iter())
            .map(|(l, r)| (l + r) * 0.5)
            .collect();

        let mut interleaved = Vec::with_capacity(n_total * 2);
        for i in 0..n_total {
            interleaved.push(left[i]);
            interleaved.push(right[i]);
        }

        let mut engine = TwoPassEngine::new();
        let scout = engine.scout(&signal, &signal, 48000, None, None, false);

        // Run with observer OFF
        let mut off_voice: Vec<f32> = Vec::new();
        use crate::stft::sliding_overlap_reader::SlidingOverlapReader;
        let reader_off = SlidingOverlapReader::new(
            TestMemorySource {
                data: interleaved.clone(),
                offset: 0,
            },
            10240,
        );
        let _ = engine
            .process_stream_with_params(
                reader_off,
                &scout,
                1.0,
                false,
                false,
                &[],
                48000.0,
                None,
                None::<&mut dyn FnMut(_)>,
                false,
                |chunk| {
                    off_voice.extend_from_slice(&chunk.voice.l);
                },
            )
            .unwrap();

        // Run with observer ON
        let mut on_voice: Vec<f32> = Vec::new();
        let reader_on = SlidingOverlapReader::new(
            TestMemorySource {
                data: interleaved,
                offset: 0,
            },
            10240,
        );
        let mut indices = Vec::new();

        let mut observer = |obs: crate::analysis::vad_model::VadObservation| {
            indices.push(obs.frame_index);
            assert!(!obs.posterior.is_nan(), "Posterior must not be NaN");
            assert!(!obs.rms_db.is_nan(), "RMS must not be NaN");
            assert!(!obs.duck_gain.is_nan(), "Duck gain must not be NaN");
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
                false,
                |chunk| {
                    on_voice.extend_from_slice(&chunk.voice.l);
                },
            )
            .unwrap();

        // 1. Assert frame_index is strictly sequential from 0
        for (i, &idx) in indices.iter().enumerate() {
            assert_eq!(
                idx, i as u64,
                "frame_index is not strictly sequential without gaps"
            );
        }

        // 2. Assert count equals floor(total_output_frames / 480)
        let sum_of_new_len = on_voice.len();
        let expected_frames = sum_of_new_len / 480;
        assert_eq!(
            indices.len(),
            expected_frames,
            "Observation count does not equal floor(sum_of_new_len / 480)"
        );

        // 3. Assert Bit-Identical output
        assert_eq!(
            on_voice.len(),
            off_voice.len(),
            "Observer perturbed the output length"
        );
        for i in 0..on_voice.len() {
            assert_eq!(
                on_voice[i].to_bits(),
                off_voice[i].to_bits(),
                "Observer perturbed the output float bits at index {}",
                i
            );
        }
    }
}

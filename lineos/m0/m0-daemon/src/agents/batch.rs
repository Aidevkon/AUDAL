//! agents/batch — Standalone batch mastering function (§Β).
//!
//! Extracted from Conductor to allow direct callability without Tokio agent channels.

use arc_swap::ArcSwap;
use std::sync::Arc;
use tokio::sync::broadcast;
use xaak::repo::DspState;

use crate::agents::operator::{
    AnalysisResult, BatchTrackOutput, ConductorError, MasteringParams, StreamingPlan,
};
use crate::app_state::AlbumEvent;
use crate::blob_store::BlobStore;
use crate::domain::nodes::album_certificate_node::{AlbumCertificate, TrackSummary};
use sp314_dsp::analysis::ear_fatigue::EarFatigueModel;

/// Standalone batch mastering function (§Β).
///
/// Runs cohesion pre-pass, computes relative loudness targets, sequentially applies
/// EarFatigue state updates to `head_state_ptr`, renders tracks via `execute_streaming_plan`,
/// and generates the `AlbumCertificate`.
pub async fn run_batch(
    batch_id: &str,
    items: Vec<MasteringParams>,
    head_state_ptr: Arc<ArcSwap<DspState>>,
    blob_store: &BlobStore,
    certs_path: &str,
    album_tx: &broadcast::Sender<AlbumEvent>,
) -> Result<Vec<BatchTrackOutput>, ConductorError> {
    let total = items.len();
    let mut outputs: Vec<BatchTrackOutput> = Vec::with_capacity(total);

    // ── Album Cohesion Pre-Pass ──────────────────────────
    // Step 1: analyze all tracks to get integrated LUFS
    // Step 2: find Anchor Track (loudest)
    // Step 3: compute per-track target offsets
    // INV-AB-1: deterministic — same inputs → same targets
    let global_target = items.first().map(|p| p.target_lufs).unwrap_or(-14.0_f32);

    let mut track_lufs: Vec<f32> = Vec::with_capacity(total);
    let mut track_analyses: Vec<AnalysisResult> = Vec::with_capacity(total);

    for params in &items {
        let audio_path = params.audio_path.clone();
        let session_id = params.session_id.clone();

        let analysis_res = tokio::task::spawn_blocking(move || {
            let path = std::path::Path::new(&audio_path);
            let metrics = crate::dsp::input_lufs::measure_input_metrics(path).map_err(|e| {
                ConductorError::ExecutorFailed(format!("input measurement failed: {e}"))
            })?;
            let integrated_lufs = metrics.integrated_lufs.unwrap_or(-144.0);
            Ok::<AnalysisResult, ConductorError>(AnalysisResult {
                session_id,
                integrated_lufs,
                true_peak_dbtp: metrics.true_peak_dbtp,
                bpm: metrics.bpm,
            })
        })
        .await;

        match analysis_res {
            Ok(Ok(analysis)) => {
                tracing::info!(
                    batch_id = %batch_id,
                    "Cohesion pre-pass: {} → {:.1} LUFS",
                    params.audio_path,
                    analysis.integrated_lufs
                );
                track_lufs.push(analysis.integrated_lufs);
                let _ = album_tx.send(AlbumEvent::Forensic {
                    track: track_lufs.len(),
                    lufs: analysis.integrated_lufs,
                });
                track_analyses.push(analysis);
            }
            _ => {
                tracing::warn!(
                    batch_id = %batch_id,
                    "Cohesion pre-pass failed for {} — using global target",
                    params.audio_path
                );
                track_lufs.push(global_target);
                track_analyses.push(AnalysisResult {
                    session_id: params.session_id.clone(),
                    integrated_lufs: global_target,
                    true_peak_dbtp: 0.0,
                    bpm: 0.0,
                });
            }
        }
    }

    // Anchor = loudest track (highest LUFS = least negative)
    let anchor_lufs = track_lufs
        .iter()
        .copied()
        .filter(|l| l.is_finite() && *l > -70.0)
        .fold(f32::NEG_INFINITY, f32::max);

    let anchor_lufs = if anchor_lufs.is_finite() {
        anchor_lufs
    } else {
        global_target
    };

    // per_track_target = global_target - (anchor_lufs - track_lufs)
    let per_track_targets: Vec<f32> = track_lufs
        .iter()
        .map(|&lufs| {
            let offset = anchor_lufs - lufs;
            (global_target - offset).clamp(-40.0, 0.0)
        })
        .collect();

    let _ = album_tx.send(AlbumEvent::Cohesion {
        per_track_targets: per_track_targets.clone(),
    });

    tracing::info!(
        batch_id = %batch_id,
        "Cohesion: anchor={:.1} LUFS, targets={:?}",
        anchor_lufs,
        per_track_targets
    );
    // ── End Album Cohesion Pre-Pass ──────────────────────

    // EarFatigue uses integrated_lufs as proxy
    let ear_model = EarFatigueModel::default();
    let proxy_analyses: Vec<lineos_types::pre_analysis::PreAnalysisData> = track_lufs
        .iter()
        .map(|&lufs| lineos_types::pre_analysis::PreAnalysisData {
            integrated_lufs: lufs,
            ..lineos_types::pre_analysis::PreAnalysisData::silent()
        })
        .collect();

    for (index, params) in items.into_iter().enumerate() {
        let cohesion_target = per_track_targets
            .get(index)
            .copied()
            .unwrap_or(global_target);
        tracing::info!(
            batch_id = %batch_id,
            "Conductor: batch track {}/{} — {}",
            index + 1, total, params.audio_path
        );

        let ear_delta = if index > 0 {
            proxy_analyses
                .get(index - 1)
                .map(|prev| ear_model.compute_delta(prev))
                .unwrap_or_default()
        } else {
            sp314_dsp::analysis::ear_fatigue::EarFatigueDelta::default()
        };

        if ear_delta.fatigue_detected {
            tracing::info!(
                batch_id = %batch_id,
                "EarFatigue: track {} recovery (prev LUFS={:.1})",
                index + 1,
                track_lufs.get(index.saturating_sub(1)).copied().unwrap_or(-14.0),
            );

            let current = head_state_ptr.load_full();
            let adjusted = DspState {
                ducking_depth: (current.ducking_depth * ear_delta.ducking_multiplier)
                    .clamp(0.3, 1.0),
                ms_width: (current.ms_width * ear_delta.width_multiplier).clamp(0.5, 2.0),
                sidechain_hold: current.sidechain_hold,
                lfe_gain: current.lfe_gain,
            };
            head_state_ptr.store(std::sync::Arc::new(adjusted));
            tracing::info!(
                batch_id = %batch_id,
                "AB-P6: EarFatigue ArcSwap — ducking={:.3} width={:.3}",
                adjusted.ducking_depth,
                adjusted.ms_width,
            );
        }

        let ducking = if ear_delta.fatigue_detected {
            let current = head_state_ptr.load();
            (current.ducking_depth * ear_delta.ducking_multiplier).clamp(0.3, 1.0)
        } else {
            1.0
        };
        let bpm = track_analyses.get(index).map(|a| a.bpm).unwrap_or(0.0);
        let _ = album_tx.send(AlbumEvent::PreAnalysis {
            track: index + 1,
            bpm,
            ducking_gain: ducking,
        });

        let width = if ear_delta.fatigue_detected {
            let current = head_state_ptr.load();
            (current.ms_width * ear_delta.width_multiplier).clamp(0.5, 2.0)
        } else {
            1.0
        };
        let _ = album_tx.send(AlbumEvent::Fatigue {
            track: index + 1,
            ducking,
            width,
        });

        let plan = StreamingPlan {
            audio_path: params.audio_path.clone(),
            preset_id: params.preset_id.clone(),
            flavour_id: params.flavour_id.clone(),
            intent_tone: params.intent_tone,
            intent_dynamics: params.intent_dynamics,
            target_lufs_override: Some(cohesion_target),
            session_id: params.session_id.clone(),
        };

        // Render via execute_streaming_plan
        let render_res = tokio::task::spawn_blocking(move || {
            crate::agents::executor::execute_streaming_plan(&plan, None)
        })
        .await;

        match render_res {
            Ok(Ok((streaming_output, blob))) => {
                blob_store.insert(blob);
                outputs.push(BatchTrackOutput {
                    track_index: index,
                    session_id: params.session_id,
                    blob_id: streaming_output.blob_id,
                    status: "ok",
                    error: None,
                    pcm_blake3: streaming_output.pcm_blake3,
                    output_lufs: streaming_output.output_lufs,
                });
            }
            Ok(Err(e)) => {
                outputs.push(BatchTrackOutput {
                    track_index: index,
                    session_id: params.session_id,
                    blob_id: String::new(),
                    status: "error",
                    error: Some(format!("{:?}", e)),
                    pcm_blake3: String::new(),
                    output_lufs: 0.0,
                });
            }
            Err(e) => {
                outputs.push(BatchTrackOutput {
                    track_index: index,
                    session_id: params.session_id,
                    blob_id: String::new(),
                    status: "error",
                    error: Some(format!("spawn_blocking join error: {e}")),
                    pcm_blake3: String::new(),
                    output_lufs: 0.0,
                });
            }
        }
    }

    let ok_count = outputs.iter().filter(|o| o.status == "ok").count();
    tracing::info!(
        batch_id = %batch_id,
        "Conductor: batch complete — {}/{} ok",
        ok_count, total
    );

    // AB-P7: Generate AlbumCertificate from batch results
    let fatigue_map: Vec<bool> = proxy_analyses
        .windows(2)
        .map(|w| {
            let model = sp314_dsp::analysis::ear_fatigue::EarFatigueModel::default();
            model.compute_delta(&w[0]).fatigue_detected
        })
        .chain(std::iter::once(false))
        .collect();

    let album_cert_path = {
        let summaries: Vec<TrackSummary> = outputs
            .iter()
            .map(|o| TrackSummary {
                blob_id: o.blob_id.clone(),
                content_hash: o.pcm_blake3.clone(),
                integrated_lufs: o.output_lufs,
            })
            .collect();

        let anchor_idx = proxy_analyses
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.integrated_lufs.partial_cmp(&b.integrated_lufs).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0);

        let cert = AlbumCertificate::from_tracks(batch_id, &summaries, anchor_idx, &fatigue_map);
        let path = cert.write_to_disk(batch_id, certs_path);
        tracing::info!(
            batch_id = %batch_id,
            "AB-P7: AlbumCertificate written → {:?}",
            path
        );
        path
    };
    let _ = album_cert_path;

    Ok(outputs)
}

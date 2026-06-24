//! AlbumConductor — the brain of album mastering.
//! Orchestrates: PhantomMaster + EarFatigue + MorphCurve
//! Authority: aether-black-spec-v1_0.md AB-P4
//! INV-AB-1: same album → same plan. Always.

use crate::analysis::ear_fatigue::EarFatigueModel;
use crate::analysis::morph_curve::MorphCurve;
use crate::analysis::phantom_master::PhantomMaster;
use lineos_types::pre_analysis::PreAnalysisData;
use xaak::repo::DspState;

/// Per-track mastering plan computed by AlbumConductor.
#[derive(Debug, Clone)]
pub struct TrackPlan {
    pub track_idx: usize,
    /// Target LUFS for this track (relative to anchor)
    pub target_lufs: f32,
    /// DspState for the opening of this track
    pub opening_state: DspState,
    /// Transition frames from previous track (empty for track 0)
    pub morph_frames: Vec<DspState>,
    /// Was ear fatigue detected from previous track?
    pub fatigue_applied: bool,
    /// Distance from PhantomMaster (0 = perfect, higher = needs more work)
    pub phantom_distance: f32,
}

pub struct AlbumConductor {
    pub ear_model: EarFatigueModel,
    pub morph: MorphCurve,
    /// Anchor LUFS — loudest track, all others relative to this
    pub anchor_lufs: f32,
}

impl Default for AlbumConductor {
    fn default() -> Self {
        Self {
            ear_model: EarFatigueModel::default(),
            morph: MorphCurve::default(),
            anchor_lufs: -14.0,
        }
    }
}

impl AlbumConductor {
    pub fn new(ear_model: EarFatigueModel, morph: MorphCurve) -> Self {
        Self {
            ear_model,
            morph,
            anchor_lufs: -14.0,
        }
    }

    /// Phase 1: Find the anchor track (loudest = reference).
    /// All other tracks are offset relative to anchor.
    /// INV-AB-1: deterministic — max by integrated_lufs.
    pub fn find_anchor(analyses: &[PreAnalysisData]) -> usize {
        analyses
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.integrated_lufs.partial_cmp(&b.integrated_lufs).unwrap())
            .map(|(i, _)| i)
            .unwrap_or(0)
    }

    /// Phase 2: Compute relative LUFS target for each track.
    /// Preserves artist's dynamic arc — never flattens album.
    pub fn relative_lufs_target(
        anchor_lufs: f32,
        track_lufs: f32,
        export_target: f32, // e.g. -14.0 LUFS (Spotify standard)
        floor_lufs: f32,    // minimum allowed (e.g. -23.0)
    ) -> f32 {
        // How many LU softer is this track vs anchor?
        let offset_from_anchor = anchor_lufs - track_lufs;
        // Apply same offset to export target
        // Anchor → export_target, others → export_target - offset
        (export_target - offset_from_anchor).max(floor_lufs)
    }

    /// Phase 3: Plan the full album — one TrackPlan per track.
    pub fn plan_album(
        &self,
        analyses: &[PreAnalysisData],
        base_state: &DspState,
        morph_frames_n: usize,
    ) -> Vec<TrackPlan> {
        if analyses.is_empty() {
            return vec![];
        }

        let phantom = PhantomMaster::from_tracks(analyses);
        let anchor_idx = Self::find_anchor(analyses);
        let anchor_lufs = analyses[anchor_idx].integrated_lufs;

        let mut plans = Vec::with_capacity(analyses.len());
        let mut prev_analysis: Option<&PreAnalysisData> = None;
        let mut prev_state = *base_state;

        for (i, analysis) in analyses.iter().enumerate() {
            // Relative LUFS target
            let target_lufs =
                Self::relative_lufs_target(anchor_lufs, analysis.integrated_lufs, -14.0, -23.0);

            // EarFatigue from previous track
            let (opening_state, fatigue_applied) = if let Some(prev) = prev_analysis {
                let delta = self.ear_model.compute_delta(prev);
                let adjusted = self.ear_model.apply_delta(base_state, &delta);
                (adjusted, delta.fatigue_detected)
            } else {
                (*base_state, false)
            };

            // MorphCurve transition frames from previous state
            let morph_frames = if i > 0 {
                self.morph
                    .generate_frames(&prev_state, &opening_state, morph_frames_n)
            } else {
                vec![]
            };

            // Distance from PhantomMaster
            let phantom_distance = phantom
                .as_ref()
                .map(|pm| pm.distance_from(analysis))
                .unwrap_or(0.0);

            plans.push(TrackPlan {
                track_idx: i,
                target_lufs,
                opening_state,
                morph_frames,
                fatigue_applied,
                phantom_distance,
            });

            prev_analysis = Some(analysis);
            prev_state = opening_state;
        }

        plans
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_analysis(lufs: f32, td: f32) -> PreAnalysisData {
        PreAnalysisData {
            integrated_lufs: lufs,
            transient_density: td,
            ..PreAnalysisData::silent()
        }
    }

    #[test]
    fn find_anchor_returns_loudest() {
        let analyses = vec![
            make_analysis(-14.0, 1.0),
            make_analysis(-8.0, 3.0), // loudest → anchor
            make_analysis(-16.0, 1.0),
        ];
        assert_eq!(AlbumConductor::find_anchor(&analyses), 1);
    }

    #[test]
    fn relative_lufs_preserves_offset() {
        // Anchor=-8, track=-14 (6 LU softer), export_target=-14
        // Track should be mastered to: -14 - 6 = -20 LUFS
        let target = AlbumConductor::relative_lufs_target(-8.0, -14.0, -14.0, -23.0);
        assert!(
            (target - (-20.0)).abs() < 0.001,
            "6 LU gap should be preserved: expected -20.0, got {}",
            target
        );
    }

    #[test]
    fn anchor_track_hits_export_target() {
        // Anchor track should always hit exactly export_target
        let target = AlbumConductor::relative_lufs_target(-8.0, -8.0, -14.0, -23.0);
        assert!(
            (target - (-14.0)).abs() < 0.001,
            "Anchor should hit export_target: expected -14.0, got {}",
            target
        );
    }

    #[test]
    fn plan_album_correct_count() {
        let conductor = AlbumConductor::default();
        let base = DspState::default();
        let analyses = vec![
            make_analysis(-14.0, 1.0),
            make_analysis(-8.0, 4.0),
            make_analysis(-16.0, 1.0),
        ];
        let plans = conductor.plan_album(&analyses, &base, 10);
        assert_eq!(plans.len(), 3);
    }

    #[test]
    fn first_track_has_no_morph_frames() {
        let conductor = AlbumConductor::default();
        let base = DspState::default();
        let analyses = vec![make_analysis(-14.0, 1.0), make_analysis(-8.0, 4.0)];
        let plans = conductor.plan_album(&analyses, &base, 10);
        assert!(plans[0].morph_frames.is_empty());
        assert_eq!(plans[1].morph_frames.len(), 10);
    }

    #[test]
    fn aggressive_track_triggers_fatigue_on_next() {
        let conductor = AlbumConductor::default();
        let base = DspState::default();
        let analyses = vec![
            make_analysis(-7.0, 5.0),  // aggressive → triggers fatigue
            make_analysis(-14.0, 1.0), // next track gets recovery
        ];
        let plans = conductor.plan_album(&analyses, &base, 5);
        assert!(!plans[0].fatigue_applied, "First track has no previous");
        assert!(plans[1].fatigue_applied, "Second track should have fatigue");
        assert!(
            plans[1].opening_state.ducking_depth < base.ducking_depth,
            "Fatigue should soften ducking"
        );
    }
}

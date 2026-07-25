use crate::spatial::five_dot_one::FiveDotOneStage;

pub struct StereoRenderer;

impl StereoRenderer {
    /// ITU downmix: L+R with center and surround fold
    /// INV-SP-2: mono-compatible output
    pub fn render(stage: &FiveDotOneStage) -> (Vec<f32>, Vec<f32>) {
        let len = stage.l.len();
        let mut l_out = vec![0.0; len];
        let mut r_out = vec![0.0; len];

        let c_gain = 0.707_f32;
        let s_gain = 0.707_f32;
        // lfe_blend = LFE * 0.316 (-10dB)
        let lfe_gain = libm::powf(10.0_f32, -10.0 / 20.0);

        for i in 0..len {
            let lfe_blend = stage.lfe[i] * lfe_gain;
            l_out[i] = stage.l[i] + stage.c[i] * c_gain + stage.ls[i] * s_gain + lfe_blend;
            r_out[i] = stage.r[i] + stage.c[i] * c_gain + stage.rs[i] * s_gain + lfe_blend;
        }

        (l_out, r_out)
    }
}

pub struct FiveDotOneRenderer;

impl FiveDotOneRenderer {
    /// Write 6-channel stage output directly
    /// into pre-allocated mutable slices.
    /// Zero heap allocation — no intermediate Vec.
    ///
    /// offset: frame offset within the output
    ///         buffers (chunk write position)
    /// Each out_* slice must have length >=
    ///   offset + stage.l.len()
    // allow: 8 args; a params-struct refactor is deliberately deferred — not done as a clippy side-fix
    #[allow(clippy::too_many_arguments)]
    pub fn render_into(
        stage: &FiveDotOneStage,
        out_l: &mut [f32],
        out_r: &mut [f32],
        out_c: &mut [f32],
        out_lfe: &mut [f32],
        out_ls: &mut [f32],
        out_rs: &mut [f32],
        offset: usize,
    ) {
        let n = stage.l.len();
        let end = offset + n;
        out_l[offset..end].copy_from_slice(&stage.l);
        out_r[offset..end].copy_from_slice(&stage.r);
        out_c[offset..end].copy_from_slice(&stage.c);
        out_lfe[offset..end].copy_from_slice(&stage.lfe);
        out_ls[offset..end].copy_from_slice(&stage.ls);
        out_rs[offset..end].copy_from_slice(&stage.rs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_stage() -> FiveDotOneStage {
        FiveDotOneStage {
            l: vec![0.0; 100],
            r: vec![0.0; 100],
            c: vec![0.0; 100],
            ls: vec![0.0; 100],
            rs: vec![0.0; 100],
            lfe: vec![0.0; 100],
        }
    }

    #[test]
    fn stereo_output_is_mono_compatible() {
        let mut stage = mock_stage();
        for i in 0..100 {
            stage.l[i] = 1.0;
            stage.r[i] = 1.0;
            stage.ls[i] = 0.5;
            stage.rs[i] = 0.5;
            stage.c[i] = 0.8;
            stage.lfe[i] = 0.2;
        }
        let (l_out, r_out) = StereoRenderer::render(&stage);

        assert_eq!(l_out, r_out);
    }

    #[test]
    fn center_energy_in_both_channels() {
        let mut stage = mock_stage();
        stage.c = vec![1.0; 100];
        let (l_out, r_out) = StereoRenderer::render(&stage);
        assert!(l_out.iter().sum::<f32>() > 0.0);
        assert!(r_out.iter().sum::<f32>() > 0.0);
        assert_eq!(l_out, r_out);
    }

    #[test]
    fn lfe_blended_at_minus_10db() {
        let mut stage = mock_stage();
        stage.lfe = vec![1.0; 100];
        let (l_out, _) = StereoRenderer::render(&stage);

        let val = l_out[0];
        assert!(
            (val - 0.31622776).abs() < 1e-5,
            "Expected ~0.316, got {}",
            val
        );
    }

    #[test]
    fn stereo_renderer_deterministic() {
        let mut stage1 = mock_stage();
        stage1.c = vec![0.5; 100];
        let mut stage2 = mock_stage();
        stage2.c = vec![0.5; 100];

        let r1 = StereoRenderer::render(&stage1);
        let r2 = StereoRenderer::render(&stage2);
        assert_eq!(r1, r2);
    }
}

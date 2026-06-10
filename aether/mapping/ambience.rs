// aether/mapping/ambience.rs
use super::types::{AmbienceMacroControls, AmbienceMicroDelta};

pub struct AmbienceMicroMapper;

impl AmbienceMicroMapper {
    pub fn map(macros: &AmbienceMacroControls) -> AmbienceMicroDelta {
        let space = macros.space.clamp(0.0, 1.0);
        let width = macros.width.clamp(0.0, 1.0);
        let tone = macros.tone.clamp(0.0, 1.0);
        let loudness = macros.loudness.clamp(0.0, 1.0);

        // 4.1 SPACE
        let reverb_time_delta_s = 0.8 * space;
        let pre_delay_delta_ms = 12.0 * space;
        let diffusion_delta = 0.4 * space;
        let high_shelf_gain_db = 2.0 * space;
        let high_shelf_freq_delta = 200.0 * space;
        let low_shelf_cut_db = -1.5 * space;
        let reverb_send_level = 0.3 * space;

        // 4.2 WIDTH
        let decorrelation = 0.6 * width;
        let side_gain_db = 0.4 * width;
        let phase_variance = 0.3 * width;
        let mono_comp_shelf_db = -0.5 * width;

        // 4.3 TONE
        let hf_damping_db = -1.5 * tone;
        let low_mid_cut_db = -tone;

        // 4.4 LOUDNESS
        let tail_density_delta = 0.2 * loudness;
        let output_gain_db = 1.0 * loudness;
        let hf_tail_cut_db = -loudness;

        AmbienceMicroDelta {
            reverb_time_delta_s,
            pre_delay_delta_ms,
            diffusion_delta,
            high_shelf_gain_db,
            high_shelf_freq_delta,
            low_shelf_cut_db,
            reverb_send_level,
            decorrelation,
            side_gain_db,
            phase_variance,
            mono_comp_shelf_db,
            hf_damping_db,
            low_mid_cut_db,
            tail_density_delta,
            output_gain_db,
            hf_tail_cut_db,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: f32, b: f32) {
        assert!((a - b).abs() < 1e-4, "{} != {}", a, b);
    }

    #[test]
    fn test_ambience_mapper_zeros() {
        let macros = AmbienceMacroControls {
            space: 0.0,
            width: 0.0,
            tone: 0.0,
            loudness: 0.0,
        };
        let delta = AmbienceMicroMapper::map(&macros);
        assert_close(delta.reverb_time_delta_s, 0.0);
        assert_close(delta.pre_delay_delta_ms, 0.0);
        assert_close(delta.decorrelation, 0.0);
        assert_close(delta.hf_damping_db, 0.0);
        assert_close(delta.output_gain_db, 0.0);
    }

    #[test]
    fn test_ambience_mapper_mid() {
        let macros = AmbienceMacroControls {
            space: 0.5,
            width: 0.5,
            tone: 0.5,
            loudness: 0.5,
        };
        let delta = AmbienceMicroMapper::map(&macros);
        assert_close(delta.reverb_time_delta_s, 0.4);
        assert_close(delta.pre_delay_delta_ms, 6.0);
        assert_close(delta.diffusion_delta, 0.2);
        assert_close(delta.high_shelf_gain_db, 1.0);
        assert_close(delta.high_shelf_freq_delta, 100.0);
        assert_close(delta.low_shelf_cut_db, -0.75);
        assert_close(delta.reverb_send_level, 0.15);

        assert_close(delta.decorrelation, 0.3);
        assert_close(delta.side_gain_db, 0.2);
        assert_close(delta.phase_variance, 0.15);
        assert_close(delta.mono_comp_shelf_db, -0.25);

        assert_close(delta.hf_damping_db, -0.75);
        assert_close(delta.low_mid_cut_db, -0.5);

        assert_close(delta.tail_density_delta, 0.1);
        assert_close(delta.output_gain_db, 0.5);
        assert_close(delta.hf_tail_cut_db, -0.5);
    }

    #[test]
    fn test_ambience_mapper_max() {
        let macros = AmbienceMacroControls {
            space: 1.0,
            width: 1.0,
            tone: 1.0,
            loudness: 1.0,
        };
        let delta = AmbienceMicroMapper::map(&macros);
        assert_close(delta.reverb_time_delta_s, 0.8);
        assert_close(delta.pre_delay_delta_ms, 12.0);
        assert_close(delta.diffusion_delta, 0.4);
        assert_close(delta.high_shelf_gain_db, 2.0);
        assert_close(delta.high_shelf_freq_delta, 200.0);
        assert_close(delta.low_shelf_cut_db, -1.5);
        assert_close(delta.reverb_send_level, 0.3);

        assert_close(delta.decorrelation, 0.6);
        assert_close(delta.side_gain_db, 0.4);
        assert_close(delta.phase_variance, 0.3);
        assert_close(delta.mono_comp_shelf_db, -0.5);

        assert_close(delta.hf_damping_db, -1.5);
        assert_close(delta.low_mid_cut_db, -1.0);

        assert_close(delta.tail_density_delta, 0.2);
        assert_close(delta.output_gain_db, 1.0);
        assert_close(delta.hf_tail_cut_db, -1.0);
    }
}

use lineos_types::pre_analysis::PreAnalysisData;

#[derive(Debug, Clone, PartialEq)]
pub struct SimulationDelta {
    pub predicted_peak: f32,
    pub predicted_gr: f32,
    pub crest_risk: f32,
    pub isp_risk: f32,
}

impl SimulationDelta {
    pub fn zero() -> Self {
        Self {
            predicted_peak: 0.0,
            predicted_gr: 0.0,
            crest_risk: 0.0,
            isp_risk: 0.0,
        }
    }
}

pub struct SimulationLayer;

impl SimulationLayer {
    pub fn run(pre: &PreAnalysisData, target_lufs: f32) -> SimulationDelta {
        // 4.1 predicted_peak
        let peak_margin = if pre.global_crest_factor_db > 14.0 {
            0.5
        } else {
            0.0
        };
        let predicted_peak = (pre.true_peak_dbtp + peak_margin).clamp(-40.0, 3.0);

        // 4.2 predicted_gr
        let predicted_gr = (pre.integrated_lufs - target_lufs).clamp(0.0, 12.0) * 0.5;

        // 4.3 crest_risk
        let crest_db = pre.global_crest_factor_db;
        let crest_risk = if crest_db > 20.0 {
            1.0
        } else if crest_db > 14.0 {
            (crest_db - 14.0) / 6.0
        } else {
            0.0
        };

        // 4.4 isp_risk
        let isp_risk = if pre.true_peak_dbtp > -1.0 {
            1.0
        } else if pre.true_peak_dbtp > -3.0 {
            (pre.true_peak_dbtp + 3.0) / 2.0
        } else {
            0.0
        };

        SimulationDelta {
            predicted_peak,
            predicted_gr,
            crest_risk,
            isp_risk,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use lineos_types::pre_analysis::PreAnalysisData;

    fn mock_pre(lufs: f32, peak: f32, crest: f32) -> PreAnalysisData {
        PreAnalysisData {
            integrated_lufs: lufs,
            true_peak_dbtp: peak,
            global_crest_factor_db: crest,
            ..PreAnalysisData::silent()
        }
    }

    #[test]
    fn simulation_deterministic() {
        let pre = mock_pre(-10.0, -0.5, 18.0);
        let d1 = SimulationLayer::run(&pre, -14.0);
        let d2 = SimulationLayer::run(&pre, -14.0);
        assert_eq!(d1.predicted_gr, d2.predicted_gr);
        assert_eq!(d1.crest_risk, d2.crest_risk);
    }

    #[test]
    fn predicted_gr_hot_signal() {
        // Signal at -10 LUFS, target -14 → delta = +4 → gr = 2.0
        let pre = mock_pre(-10.0, -1.5, 10.0);
        let d = SimulationLayer::run(&pre, -14.0);
        assert!(
            (d.predicted_gr - 2.0).abs() < 0.01,
            "Expected 2.0, got {}",
            d.predicted_gr
        );
    }

    #[test]
    fn isp_risk_high_when_peak_exceeds() {
        let pre = mock_pre(-14.0, 0.5, 10.0); // peak > -1.0
        let d = SimulationLayer::run(&pre, -14.0);
        assert_eq!(d.isp_risk, 1.0);
    }

    #[test]
    fn crest_risk_zero_when_low() {
        let pre = mock_pre(-14.0, -2.0, 8.0); // crest < 14dB
        let d = SimulationLayer::run(&pre, -14.0);
        assert_eq!(d.crest_risk, 0.0);
    }

    #[test]
    fn all_outputs_within_bounds() {
        let pre = mock_pre(-8.0, 1.0, 25.0);
        let d = SimulationLayer::run(&pre, -14.0);
        assert!(d.predicted_peak >= -40.0 && d.predicted_peak <= 3.0);
        assert!(d.predicted_gr >= 0.0 && d.predicted_gr <= 6.0);
        assert!(d.crest_risk >= 0.0 && d.crest_risk <= 1.0);
        assert!(d.isp_risk >= 0.0 && d.isp_risk <= 1.0);
    }
}

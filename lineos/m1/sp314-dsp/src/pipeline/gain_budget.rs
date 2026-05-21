//! Cross-stage gain budget — sp314-dsp v2.9 §Cross-Stage Gain Budget.
//! libm-only arithmetic; quantized after every allocation.

use crate::types::units::Decibels;

const QUANTIZE_STEP: f32 = 0.000001;

#[inline(always)]
fn quantize(value: f32, step: f32) -> f32 {
    libm::roundf(value / step) * step
}

/// Per-stage boost allocation against a shared total budget (default 6 dB).
pub struct GainBudget {
    pub max_total_boost_db: Decibels,
    pub stage3_max_db:      Decibels,
    pub stage4_max_db:      Decibels,
    pub stage5_max_db:      Decibels,
    pub allocated_stage3:   Decibels,
    pub allocated_stage4:   Decibels,
    pub allocated_stage5:   Decibels,
}

impl Default for GainBudget {
    fn default() -> Self {
        Self {
            max_total_boost_db: Decibels(6.0),
            stage3_max_db:      Decibels(2.0),
            stage4_max_db:      Decibels(4.0),
            stage5_max_db:      Decibels(2.0),
            allocated_stage3:   Decibels(0.0),
            allocated_stage4:   Decibels(0.0),
            allocated_stage5:   Decibels(0.0),
        }
    }
}

impl GainBudget {
    pub fn request_stage3(&mut self, requested: Decibels) -> Decibels {
        let cap = Decibels(libm::fminf(
            self.stage3_max_db.0,
            self.max_total_boost_db.0
                - self.allocated_stage3.0
                - self.allocated_stage4.0
                - self.allocated_stage5.0,
        ));
        let granted = Decibels(libm::fminf(requested.0, libm::fmaxf(0.0, cap.0)));
        self.allocated_stage3 = Decibels(quantize(
            self.allocated_stage3.0 + granted.0,
            QUANTIZE_STEP,
        ));
        granted
    }

    pub fn request_stage4(&mut self, requested: Decibels) -> Decibels {
        let remaining = self.max_total_boost_db.0
            - self.allocated_stage3.0
            - self.allocated_stage4.0
            - self.allocated_stage5.0;
        let cap = Decibels(libm::fminf(self.stage4_max_db.0, remaining));
        let granted = Decibels(libm::fminf(requested.0, libm::fmaxf(0.0, cap.0)));
        self.allocated_stage4 = Decibels(quantize(
            self.allocated_stage4.0 + granted.0,
            QUANTIZE_STEP,
        ));
        granted
    }

    /// Stage 4 surplus rolls into stage 5 effective cap; stage 3 surplus does not.
    pub fn request_stage5(&mut self, requested: Decibels) -> Decibels {
        let stage4_surplus = self.stage4_max_db.0 - self.allocated_stage4.0;
        let effective_cap = self.stage5_max_db.0 + libm::fmaxf(0.0, stage4_surplus);
        let remaining = self.max_total_boost_db.0
            - self.allocated_stage3.0
            - self.allocated_stage4.0
            - self.allocated_stage5.0;
        let cap = Decibels(libm::fminf(effective_cap, remaining));
        let granted = Decibels(libm::fminf(requested.0, libm::fmaxf(0.0, cap.0)));
        self.allocated_stage5 = Decibels(quantize(
            self.allocated_stage5.0 + granted.0,
            QUANTIZE_STEP,
        ));
        granted
    }

    pub fn is_exceeded(&self) -> bool {
        (self.allocated_stage3.0 + self.allocated_stage4.0 + self.allocated_stage5.0)
            >= self.max_total_boost_db.0
    }

    pub fn reset(&mut self) {
        self.allocated_stage3 = Decibels(0.0);
        self.allocated_stage4 = Decibels(0.0);
        self.allocated_stage5 = Decibels(0.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_stage3_cap() {
        let mut budget = GainBudget::default();
        let granted = budget.request_stage3(Decibels(5.0));
        assert_eq!(granted.0, 2.0, "stage3 capped at 2 dB");
        assert_eq!(budget.allocated_stage3.0, 2.0);
    }

    #[test]
    fn test_request_stage5_rollover_from_stage4_surplus() {
        let mut budget = GainBudget::default();
        let g4 = budget.request_stage4(Decibels(1.0));
        assert_eq!(g4.0, 1.0);
        // stage4 surplus = 4 - 1 = 3 → effective stage5 cap = 2 + 3 = 5
        let g5 = budget.request_stage5(Decibels(4.0));
        assert_eq!(g5.0, 4.0, "stage5 may use stage4 surplus up to remaining total");
        assert_eq!(budget.allocated_stage5.0, 4.0);
    }

    #[test]
    fn test_is_exceeded() {
        let mut budget = GainBudget::default();
        budget.request_stage3(Decibels(2.0));
        budget.request_stage4(Decibels(4.0));
        budget.request_stage5(Decibels(2.0));
        assert!(budget.is_exceeded());
    }

    #[test]
    fn test_reset_clears_allocated() {
        let mut budget = GainBudget::default();
        budget.request_stage3(Decibels(1.0));
        budget.request_stage4(Decibels(1.0));
        budget.reset();
        assert_eq!(budget.allocated_stage3.0, 0.0);
        assert_eq!(budget.allocated_stage4.0, 0.0);
        assert_eq!(budget.allocated_stage5.0, 0.0);
        assert!(!budget.is_exceeded());
    }
}

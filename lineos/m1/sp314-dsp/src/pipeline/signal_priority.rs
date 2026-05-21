//! Signal priority classification — sp314-dsp v2.9 §Signal Priority System.
//! Hysteresis via previous state; computed in Stage 1.5a (lookup-only downstream).

/// Per-block signal class for compressor / saturation behaviour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignalPriority {
    Transient,
    Body,
    Noise,
}

/// Crest-factor thresholds derived from median_cf (Stage 1.5a).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SignalPriorityThresholds {
    pub transient_enter: f32,
    pub transient_exit:  f32,
    pub noise_enter:     f32,
    pub noise_exit:      f32,
}

/// Classify with hysteresis from `prev` (pipeline runs start at `Body`).
pub fn classify_signal_priority(
    crest_factor: f32,
    thresholds:   &SignalPriorityThresholds,
    prev:         SignalPriority,
) -> SignalPriority {
    match prev {
        SignalPriority::Transient => {
            if crest_factor < thresholds.transient_exit {
                SignalPriority::Body
            } else {
                SignalPriority::Transient
            }
        }
        SignalPriority::Noise => {
            if crest_factor > thresholds.noise_exit {
                SignalPriority::Body
            } else {
                SignalPriority::Noise
            }
        }
        SignalPriority::Body => {
            if crest_factor > thresholds.transient_enter {
                SignalPriority::Transient
            } else if crest_factor < thresholds.noise_enter {
                SignalPriority::Noise
            } else {
                SignalPriority::Body
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_thresholds() -> SignalPriorityThresholds {
        SignalPriorityThresholds {
            transient_enter: 10.0,
            transient_exit:  8.0,
            noise_enter:     2.0,
            noise_exit:      3.0,
        }
    }

    #[test]
    fn test_body_to_transient() {
        let t = test_thresholds();
        assert_eq!(
            classify_signal_priority(11.0, &t, SignalPriority::Body),
            SignalPriority::Transient
        );
    }

    #[test]
    fn test_body_to_noise() {
        let t = test_thresholds();
        assert_eq!(
            classify_signal_priority(1.0, &t, SignalPriority::Body),
            SignalPriority::Noise
        );
    }

    #[test]
    fn test_transient_to_body() {
        let t = test_thresholds();
        assert_eq!(
            classify_signal_priority(7.0, &t, SignalPriority::Transient),
            SignalPriority::Body
        );
    }

    #[test]
    fn test_noise_to_body() {
        let t = test_thresholds();
        assert_eq!(
            classify_signal_priority(4.0, &t, SignalPriority::Noise),
            SignalPriority::Body
        );
    }

    #[test]
    fn test_transient_stays() {
        let t = test_thresholds();
        assert_eq!(
            classify_signal_priority(9.0, &t, SignalPriority::Transient),
            SignalPriority::Transient
        );
    }

    #[test]
    fn test_noise_stays() {
        let t = test_thresholds();
        assert_eq!(
            classify_signal_priority(1.0, &t, SignalPriority::Noise),
            SignalPriority::Noise
        );
    }
}

//! BS.775 5.1-to-stereo downmix (Lo/Ro), for monitoring a 5.1 master on
//! stereo output hardware. NOT used in the decode/mastering path — that
//! path (decode.rs, post-Epic) preserves discrete 5.1 channels end to end.
//! This exists purely so a user can preview/audition a 5.1 master through
//! ordinary stereo headphones/speakers.
//! Channel order confirmed via symphonia-core's Channels bitflag iteration
//! order: [L, R, C, LFE, Ls, Rs] (SMPTE/ITU-R BS.775 standard order).

pub fn downmix_5_1_to_stereo(channels: &[Vec<f32>; 6]) -> (Vec<f32>, Vec<f32>) {
    let [l, r, c, _lfe, ls, rs] = channels;
    let len = l.len();
    let mut l_out = vec![0.0_f32; len];
    let mut r_out = vec![0.0_f32; len];
    let c_gain = 0.707_f32;
    let s_gain = 0.707_f32;
    // LFE intentionally excluded from Lo/Ro per BS.775 — folding sub-bass
    // energy into a 2-channel mix risks phase cancellation/overload, not
    // spatial fidelity preservation.
    for i in 0..len {
        l_out[i] = l[i] + c[i] * c_gain + ls[i] * s_gain;
        r_out[i] = r[i] + c[i] * c_gain + rs[i] * s_gain;
    }
    (l_out, r_out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_center_symmetric_downmix() {
        let len = 100;
        let channels = [
            vec![0.0; len], // L
            vec![0.0; len], // R
            vec![1.0; len], // C
            vec![0.0; len], // LFE
            vec![0.0; len], // Ls
            vec![0.0; len], // Rs
        ];

        let (l_out, r_out) = downmix_5_1_to_stereo(&channels);

        // Assert they are equal to each other
        assert_eq!(l_out, r_out);

        // Assert they are not zero, but rather 0.707
        assert_eq!(l_out[0], 0.707);
        assert_eq!(r_out[0], 0.707);
    }

    #[test]
    fn test_lfe_exclusion() {
        let len = 100;
        let channels = [
            vec![0.0; len], // L
            vec![0.0; len], // R
            vec![0.0; len], // C
            vec![1.0; len], // LFE
            vec![0.0; len], // Ls
            vec![0.0; len], // Rs
        ];

        let (l_out, r_out) = downmix_5_1_to_stereo(&channels);

        // Assert they are exactly zero (LFE completely excluded)
        assert_eq!(l_out, vec![0.0; len]);
        assert_eq!(r_out, vec![0.0; len]);
    }
}

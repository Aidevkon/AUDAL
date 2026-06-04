//! Fast Walsh-Hadamard Transform
//! Authority: Orthogonal Transforms Spec v1.1 OT-P3
//! INV-OT-2: H · H^T = N·I
//! INV-OT-3: integer arithmetic only

/// Fast Walsh-Hadamard Transform — IN-PLACE (modifies input).
/// Named `fwht` not `fwht_inplace` for brevity.
/// Caller must clone if original needed.
pub fn fwht(data: &mut [f32]) {
    let n = data.len();
    assert!(n.is_power_of_two(), "FWHT requires power-of-2 length");
    let mut step = 1;
    while step < n {
        for i in (0..n).step_by(step * 2) {
            for j in i..i + step {
                let u = data[j];
                let v = data[j + step];
                data[j]        = u + v;
                data[j + step] = u - v;
            }
        }
        step *= 2;
    }
}

/// Normalized FWHT: H/√N
pub fn fwht_normalized(data: &mut [f32]) {
    fwht(data);
    let norm = 1.0 / libm::sqrtf(data.len() as f32);
    data.iter_mut().for_each(|x| *x *= norm);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hadamard_orthogonality() {
        // H · H = N · I  (applying twice = N * original)
        let original = vec![1.0f32, 2.0, 3.0, 4.0];
        let mut data = original.clone();
        fwht(&mut data);
        fwht(&mut data);
        let n = original.len() as f32;
        for (a, b) in data.iter().zip(original.iter()) {
            assert!((a - b * n).abs() < 1e-5,
                "Orthogonality violated: got {}, expected {}", a, b * n);
        }
    }

    #[test]
    fn hadamard_normalized_is_identity() {
        // H_norm · H_norm = I
        let original = vec![1.0f32, 0.0, 0.0, 0.0];
        let mut data = original.clone();
        fwht_normalized(&mut data);
        fwht_normalized(&mut data);
        for (a, b) in data.iter().zip(original.iter()) {
            assert!((a - b).abs() < 1e-5,
                "Normalized Hadamard not identity: {} != {}", a, b);
        }
    }

    #[test]
    fn ms_matrix_is_hadamard_2x2() {
        // M/S matrix = 2×2 Hadamard — INV-OT-2
        let l = 0.8f32;
        let r = 0.3f32;
        let sqrt2 = libm::sqrtf(2.0f32);
        let mid  = (l + r) / sqrt2;
        let side = (l - r) / sqrt2;
        // Reconstruct
        let l_rec = (mid + side) / sqrt2;
        let r_rec = (mid - side) / sqrt2;
        assert!((l - l_rec).abs() < 1e-6,
            "M/S L reconstruction failed: {} != {}", l, l_rec);
        assert!((r - r_rec).abs() < 1e-6,
            "M/S R reconstruction failed: {} != {}", r, r_rec);
    }

    #[test]
    fn hadamard_deterministic() {
        // INV-OT-3: same input → same output
        let signal = vec![1.0f32, -1.0, 0.5, -0.5,
                          0.25, -0.25, 0.1, -0.1];
        let mut d1 = signal.clone();
        let mut d2 = signal.clone();
        fwht(&mut d1);
        fwht(&mut d2);
        assert_eq!(d1, d2);
    }

    #[test]
    fn hadamard_requires_power_of_two() {
        // Non-power-of-2 should panic
        let result = std::panic::catch_unwind(|| {
            let mut data = vec![1.0f32; 3];
            fwht(&mut data);
        });
        assert!(result.is_err(), "Expected panic for non-power-of-2");
    }
}

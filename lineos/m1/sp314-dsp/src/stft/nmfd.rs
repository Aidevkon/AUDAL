//! Math reference port from libnmfd/core/nmfconv.py (v1.0.0)
//! Golden-master provenance: research/erlangen-nmfd/*.bin (f64 LE, C-order)
//! libnmfd line refs are included in comments.
//! f64 REFERENCE path — the oracle's target; f32 production variant lives beside it (P2)
//!
//! This is an exact f64 translation of their MU algorithm. Do not "improve".

use rayon::prelude::*;

// libnmfd/utils/__init__.py
pub const EPS: f64 = 2.220446049250313e-16; // 2.0 ** -52

// shift_operator from nmfconv.py:385
// nmfconv.py:416-422
// if shift_amount < 0: shifted[:, num_cols + shift_amount: num_cols] = 0
// elif shift_amount > 0: shifted[:, 0: shift_amount] = 0
pub(crate) fn shift_operator(a: &[f64], r: usize, m: usize, shift: isize) -> Vec<f64> {
    let mut shifted = vec![0.0; r * m];
    for row in 0..r {
        for col in 0..m {
            let src_col = col as isize - shift;
            if src_col >= 0 && src_col < m as isize {
                shifted[row * m + col] = a[row * m + src_col as usize];
            }
        }
    }
    shifted
}

pub fn pairwise_sum(arr: &[f64]) -> f64 {
    if arr.len() <= 128 {
        arr.iter().sum()
    } else {
        let mid = arr.len() / 2;
        pairwise_sum(&arr[..mid]) + pairwise_sum(&arr[mid..])
    }
}

// conv_model from nmfconv.py:341
pub(crate) fn conv_model(
    w: &[f64],
    h: &[f64],
    num_bins: usize,
    k: usize,
    t_frames: usize,
    num_frames: usize,
) -> Vec<f64> {
    let mut lamb = vec![0.0; num_bins * num_frames];
    // nmfconv.py:375: for k in range(num_template_frames):
    for tau in 0..t_frames {
        // nmfconv.py:376: mult_result = W[:, :, k] @ shift_operator(H, k)
        let shifted_h = shift_operator(h, k, num_frames, tau as isize);

        for bin in 0..num_bins {
            for r in 0..k {
                let w_val = w[(bin * k * t_frames) + (r * t_frames) + tau];
                for frame in 0..num_frames {
                    let h_val = shifted_h[r * num_frames + frame];
                    lamb[bin * num_frames + frame] =
                        w_val.mul_add(h_val, lamb[bin * num_frames + frame]);
                }
            }
        }
    }
    // nmfconv.py:380: lamb += EPS
    for x in &mut lamb {
        *x += EPS;
    }
    lamb
}

// nmfd from nmfconv.py:161
#[allow(clippy::too_many_arguments)] // explicit dims are the contract: no params struct, no silent 'cleanup' (S4 doctrine)
pub fn nmfd(
    v: &[f64],
    init_w: &[f64],
    init_h: &[f64],
    num_bins: usize,
    k: usize,
    num_frames: usize,
    t_frames: usize,
    num_iter: usize,
) -> (Vec<f64>, Vec<f64>, f64) {
    let mut tensor_w = init_w.to_vec();
    let mut h = init_h.to_vec();

    // nmfconv.py:272: V_tmp = V / (EPS + V.sum())
    let v_sum: f64 = pairwise_sum(v);
    let v_tmp_denom = EPS + v_sum;
    let mut v_tmp = vec![0.0; num_bins * num_frames];
    for i in 0..(num_bins * num_frames) {
        v_tmp[i] = v[i] / v_tmp_denom;
    }

    let mut final_cost = 0.0;

    // nmfconv.py:274: for iter in tnrange(L, desc='Processing'):
    for _iter in 0..num_iter {
        // nmfconv.py:280: Lambda = conv_model(tensor_W, H)
        let lambda = conv_model(&tensor_w, &h, num_bins, k, t_frames, num_frames);

        // nmfconv.py:283: cost_mat = V_tmp * np.log(1.0 + V_tmp/(Lambda+EPS)) - V_tmp + Lambda
        let mut cost_mat = vec![0.0; num_bins * num_frames];
        for i in 0..(num_bins * num_frames) {
            let lam = lambda[i]; // EPS is already added in conv_model, but Python does Lambda + EPS again!
            cost_mat[i] = v_tmp[i] * (1.0 + v_tmp[i] / (lam + EPS)).ln() - v_tmp[i] + lam;
        }
        // nmfconv.py:284: cost_func[iter] = cost_mat.mean()
        final_cost = pairwise_sum(&cost_mat) / (num_bins * num_frames) as f64;

        // nmfconv.py:287: Q = V_tmp / (Lambda + EPS)
        let mut q = vec![0.0; num_bins * num_frames];
        for i in 0..(num_bins * num_frames) {
            q[i] = v_tmp[i] / (lambda[i] + EPS);
        }

        // nmfconv.py:290: mult_H = np.zeros((R, M))
        let mut mult_h = vec![0.0; k * num_frames];

        // nmfconv.py:293: for t in range(T):
        for tau in 0..t_frames {
            // nmfconv.py:299: transpH = shift_operator(H, tau).T
            let shifted_h = shift_operator(&h, k, num_frames, tau as isize);

            // nmfconv.py:302: multW = Q @ transpH / (ones_matrix @ transpH + EPS)
            let mut num_update_w = vec![0.0; num_bins * k];
            let mut den_update_w = vec![0.0; num_bins * k];

            let mut h_row_sums = vec![0.0; k];
            for r in 0..k {
                for m in 0..num_frames {
                    h_row_sums[r] += shifted_h[r * num_frames + m];
                }
            }

            for bin in 0..num_bins {
                for r in 0..k {
                    let mut sum_q_h = 0.0_f64;
                    for m in 0..num_frames {
                        sum_q_h =
                            q[bin * num_frames + m].mul_add(shifted_h[r * num_frames + m], sum_q_h);
                    }
                    num_update_w[bin * k + r] = sum_q_h;
                    den_update_w[bin * k + r] = h_row_sums[r] + EPS;
                }
            }

            // nmfconv.py:305: tensor_W[:, :, t] *= multW
            for bin in 0..num_bins {
                for r in 0..k {
                    let w_idx = (bin * k * t_frames) + (r * t_frames) + tau;
                    tensor_w[w_idx] *= num_update_w[bin * k + r] / den_update_w[bin * k + r];
                }
            }

            // nmfconv.py:309: transp_W = tensor_W[:, :, t].T
            // nmfconv.py:312: add_W = (transp_W @ shift_operator(Q, -tau)) / (transp_W @ ones_matrix + EPS)
            let shifted_q = shift_operator(&q, num_bins, num_frames, -(tau as isize));

            for r in 0..k {
                let mut w_col_sum = 0.0;
                for bin in 0..num_bins {
                    w_col_sum += tensor_w[(bin * k * t_frames) + (r * t_frames) + tau];
                }
                let den_h = w_col_sum + EPS;

                for m in 0..num_frames {
                    let mut sum_w_q = 0.0_f64;
                    for bin in 0..num_bins {
                        sum_w_q = tensor_w[(bin * k * t_frames) + (r * t_frames) + tau]
                            .mul_add(shifted_q[bin * num_frames + m], sum_w_q);
                    }
                    mult_h[r * num_frames + m] += sum_w_q / den_h;
                }
            }
        }

        // nmfconv.py:319: H *= mult_H / T
        for r in 0..k {
            for m in 0..num_frames {
                h[r * num_frames + m] *= mult_h[r * num_frames + m] / (t_frames as f64);
            }
        }

        // nmfconv.py:326: norm_vec = tensor_W.sum(axis=2).sum(axis=0)
        let mut norm_vec = vec![0.0; k];
        for r in 0..k {
            let mut bin_sums = vec![0.0; num_bins];
            for bin in 0..num_bins {
                let mut tau_sums = vec![0.0; t_frames];
                for tau in 0..t_frames {
                    tau_sums[tau] = tensor_w[(bin * k * t_frames) + (r * t_frames) + tau];
                }
                bin_sums[bin] = pairwise_sum(&tau_sums);
            }
            norm_vec[r] = pairwise_sum(&bin_sums);
        }

        // nmfconv.py:327: tensor_W *= 1.0 / (EPS + np.expand_dims(norm_vec, axis=1))
        for bin in 0..num_bins {
            for r in 0..k {
                let factor = 1.0 / (EPS + norm_vec[r]);
                for tau in 0..t_frames {
                    tensor_w[(bin * k * t_frames) + (r * t_frames) + tau] *= factor;
                }
            }
        }
    }

    (tensor_w, h, final_cost)
}

// ============================================================================
// f32 PRODUCTION VARIANT
// ============================================================================
// Same algorithm, f32 storage/arithmetic, SAME operation order, pairwise sums
// in f32. Not generic to prevent silent operation order drift.

pub const EPS_F32: f32 = 1.1920929e-07; // 2.0 ** -23

pub(crate) fn shift_operator_f32(a: &[f32], r: usize, m: usize, shift: isize) -> Vec<f32> {
    let mut shifted = vec![0.0; r * m];
    for row in 0..r {
        for col in 0..m {
            let src_col = col as isize - shift;
            if src_col >= 0 && src_col < m as isize {
                shifted[row * m + col] = a[row * m + src_col as usize];
            }
        }
    }
    shifted
}

pub fn pairwise_sum_f32(arr: &[f32]) -> f32 {
    if arr.len() <= 128 {
        arr.iter().sum()
    } else {
        let mid = arr.len() / 2;
        pairwise_sum_f32(&arr[..mid]) + pairwise_sum_f32(&arr[mid..])
    }
}

pub(crate) fn conv_model_f32(
    w: &[f32],
    h: &[f32],
    num_bins: usize,
    k: usize,
    t_frames: usize,
    num_frames: usize,
) -> Vec<f32> {
    let mut lamb = vec![0.0; num_bins * num_frames];
    for tau in 0..t_frames {
        let shifted_h = shift_operator_f32(h, k, num_frames, tau as isize);

        for bin in 0..num_bins {
            for r in 0..k {
                let w_val = w[(bin * k * t_frames) + (r * t_frames) + tau];
                for frame in 0..num_frames {
                    let h_val = shifted_h[r * num_frames + frame];
                    lamb[bin * num_frames + frame] =
                        w_val.mul_add(h_val, lamb[bin * num_frames + frame]);
                }
            }
        }
    }
    for x in &mut lamb {
        *x += EPS_F32;
    }
    lamb
}

#[allow(clippy::too_many_arguments)] // explicit dims are the contract: no params struct, no silent 'cleanup' (S4 doctrine)
pub fn nmfd_f32_seq(
    v: &[f32],
    init_w: &[f32],
    init_h: &[f32],
    num_bins: usize,
    k: usize,
    num_frames: usize,
    t_frames: usize,
    num_iter: usize,
) -> (Vec<f32>, Vec<f32>, f32) {
    let mut tensor_w = init_w.to_vec();
    let mut h = init_h.to_vec();

    let v_sum: f32 = pairwise_sum_f32(v);
    let v_tmp_denom = EPS_F32 + v_sum;
    let mut v_tmp = vec![0.0; num_bins * num_frames];
    for i in 0..(num_bins * num_frames) {
        v_tmp[i] = v[i] / v_tmp_denom;
    }

    let mut final_cost = 0.0;

    for _iter in 0..num_iter {
        let lambda = conv_model_f32(&tensor_w, &h, num_bins, k, t_frames, num_frames);

        let mut cost_mat = vec![0.0; num_bins * num_frames];
        for i in 0..(num_bins * num_frames) {
            let lam = lambda[i];
            cost_mat[i] = v_tmp[i] * (1.0 + v_tmp[i] / (lam + EPS_F32)).ln() - v_tmp[i] + lam;
        }
        final_cost = pairwise_sum_f32(&cost_mat) / (num_bins * num_frames) as f32;

        let mut q = vec![0.0; num_bins * num_frames];
        for i in 0..(num_bins * num_frames) {
            q[i] = v_tmp[i] / (lambda[i] + EPS_F32);
        }

        let mut mult_h = vec![0.0; k * num_frames];

        for tau in 0..t_frames {
            let shifted_h = shift_operator_f32(&h, k, num_frames, tau as isize);

            let mut num_update_w = vec![0.0; num_bins * k];
            let mut den_update_w = vec![0.0; num_bins * k];

            let mut h_row_sums = vec![0.0; k];
            for r in 0..k {
                for m in 0..num_frames {
                    h_row_sums[r] += shifted_h[r * num_frames + m];
                }
            }

            for bin in 0..num_bins {
                for r in 0..k {
                    let mut sum_q_h = 0.0_f32;
                    for m in 0..num_frames {
                        sum_q_h =
                            q[bin * num_frames + m].mul_add(shifted_h[r * num_frames + m], sum_q_h);
                    }
                    num_update_w[bin * k + r] = sum_q_h;
                    den_update_w[bin * k + r] = h_row_sums[r] + EPS_F32;
                }
            }

            for bin in 0..num_bins {
                for r in 0..k {
                    let w_idx = (bin * k * t_frames) + (r * t_frames) + tau;
                    tensor_w[w_idx] *= num_update_w[bin * k + r] / den_update_w[bin * k + r];
                }
            }

            let shifted_q = shift_operator_f32(&q, num_bins, num_frames, -(tau as isize));

            for r in 0..k {
                let mut w_col_sum = 0.0;
                for bin in 0..num_bins {
                    w_col_sum += tensor_w[(bin * k * t_frames) + (r * t_frames) + tau];
                }
                let den_h = w_col_sum + EPS_F32;

                for m in 0..num_frames {
                    let mut sum_w_q = 0.0_f32;
                    for bin in 0..num_bins {
                        sum_w_q = tensor_w[(bin * k * t_frames) + (r * t_frames) + tau]
                            .mul_add(shifted_q[bin * num_frames + m], sum_w_q);
                    }
                    mult_h[r * num_frames + m] += sum_w_q / den_h;
                }
            }
        }

        for r in 0..k {
            for m in 0..num_frames {
                h[r * num_frames + m] *= mult_h[r * num_frames + m] / (t_frames as f32);
            }
        }

        let mut norm_vec = vec![0.0; k];
        for r in 0..k {
            let mut bin_sums = vec![0.0; num_bins];
            for bin in 0..num_bins {
                let mut tau_sums = vec![0.0; t_frames];
                for tau in 0..t_frames {
                    tau_sums[tau] = tensor_w[(bin * k * t_frames) + (r * t_frames) + tau];
                }
                bin_sums[bin] = pairwise_sum_f32(&tau_sums);
            }
            norm_vec[r] = pairwise_sum_f32(&bin_sums);
        }

        for bin in 0..num_bins {
            for r in 0..k {
                let factor = 1.0 / (EPS_F32 + norm_vec[r]);
                for tau in 0..t_frames {
                    tensor_w[(bin * k * t_frames) + (r * t_frames) + tau] *= factor;
                }
            }
        }
    }

    (tensor_w, h, final_cost)
}

#[allow(clippy::too_many_arguments)] // explicit dims are the contract: no params struct, no silent 'cleanup' (S4 doctrine)
pub fn nmfd_f32_h_only(
    v: &[f32],
    tensor_w: &[f32],
    init_h: &[f32],
    num_bins: usize,
    k: usize,
    num_frames: usize,
    t_frames: usize,
    num_iter: usize,
) -> (Vec<f32>, f32) {
    let mut h = init_h.to_vec();

    let v_sum: f32 = pairwise_sum_f32(v);
    let v_tmp_denom = EPS_F32 + v_sum;
    let mut v_tmp = vec![0.0; num_bins * num_frames];
    for i in 0..(num_bins * num_frames) {
        v_tmp[i] = v[i] / v_tmp_denom;
    }

    let mut final_cost = 0.0;

    for _iter in 0..num_iter {
        let lambda = conv_model_f32(tensor_w, &h, num_bins, k, t_frames, num_frames);

        let mut cost_mat = vec![0.0; num_bins * num_frames];
        for i in 0..(num_bins * num_frames) {
            let lam = lambda[i];
            cost_mat[i] = v_tmp[i] * (1.0 + v_tmp[i] / (lam + EPS_F32)).ln() - v_tmp[i] + lam;
        }
        final_cost = pairwise_sum_f32(&cost_mat) / (num_bins * num_frames) as f32;

        let mut q = vec![0.0; num_bins * num_frames];
        for i in 0..(num_bins * num_frames) {
            q[i] = v_tmp[i] / (lambda[i] + EPS_F32);
        }

        let mut all_shifted_q = Vec::with_capacity(t_frames);
        for tau in 0..t_frames {
            all_shifted_q.push(shift_operator_f32(
                &q,
                num_bins,
                num_frames,
                -(tau as isize),
            ));
        }

        let h_updates: Vec<Vec<f32>> = (0..k)
            .into_par_iter()
            .map(|r| {
                let mut tensor_w_r = vec![0.0; num_bins * t_frames];
                for bin in 0..num_bins {
                    for tau in 0..t_frames {
                        tensor_w_r[bin * t_frames + tau] =
                            tensor_w[(bin * k * t_frames) + (r * t_frames) + tau];
                    }
                }

                let mut mult_h_r = vec![0.0; num_frames];

                for tau in 0..t_frames {
                    let shifted_q = &all_shifted_q[tau];

                    let mut w_col_sum = 0.0;
                    for bin in 0..num_bins {
                        w_col_sum += tensor_w_r[bin * t_frames + tau];
                    }
                    let den_h = w_col_sum + EPS_F32;

                    for m in 0..num_frames {
                        let mut sum_w_q = 0.0_f32;
                        for bin in 0..num_bins {
                            sum_w_q = tensor_w_r[bin * t_frames + tau]
                                .mul_add(shifted_q[bin * num_frames + m], sum_w_q);
                        }
                        mult_h_r[m] += sum_w_q / den_h;
                    }
                }
                mult_h_r
            })
            .collect();

        for r in 0..k {
            for m in 0..num_frames {
                h[r * num_frames + m] *= h_updates[r][m] / (t_frames as f32);
            }
        }
    }

    (h, final_cost)
}

pub fn nmfd_f32(
    v: &[f32],
    init_w: &[f32],
    init_h: &[f32],
    num_bins: usize,
    k: usize,
    num_frames: usize,
    t_frames: usize,
    num_iter: usize,
) -> (Vec<f32>, Vec<f32>, f32) {
    let mut tensor_w = init_w.to_vec();
    let mut h = init_h.to_vec();

    let v_sum: f32 = pairwise_sum_f32(v);
    let v_tmp_denom = EPS_F32 + v_sum;
    let mut v_tmp = vec![0.0; num_bins * num_frames];
    for i in 0..(num_bins * num_frames) {
        v_tmp[i] = v[i] / v_tmp_denom;
    }

    let mut final_cost = 0.0;

    for _iter in 0..num_iter {
        let lambda = conv_model_f32(&tensor_w, &h, num_bins, k, t_frames, num_frames);

        let mut cost_mat = vec![0.0; num_bins * num_frames];
        for i in 0..(num_bins * num_frames) {
            let lam = lambda[i];
            cost_mat[i] = v_tmp[i] * (1.0 + v_tmp[i] / (lam + EPS_F32)).ln() - v_tmp[i] + lam;
        }
        final_cost = pairwise_sum_f32(&cost_mat) / (num_bins * num_frames) as f32;

        let mut q = vec![0.0; num_bins * num_frames];
        for i in 0..(num_bins * num_frames) {
            q[i] = v_tmp[i] / (lambda[i] + EPS_F32);
        }

        let mut all_shifted_h = Vec::with_capacity(t_frames);
        let mut all_shifted_q = Vec::with_capacity(t_frames);
        for tau in 0..t_frames {
            all_shifted_h.push(shift_operator_f32(&h, k, num_frames, tau as isize));
            all_shifted_q.push(shift_operator_f32(
                &q,
                num_bins,
                num_frames,
                -(tau as isize),
            ));
        }

        let r_updates: Vec<(Vec<f32>, Vec<f32>)> = (0..k)
            .into_par_iter()
            .map(|r| {
                let mut tensor_w_r = vec![0.0; num_bins * t_frames];
                for bin in 0..num_bins {
                    for tau in 0..t_frames {
                        tensor_w_r[bin * t_frames + tau] =
                            tensor_w[(bin * k * t_frames) + (r * t_frames) + tau];
                    }
                }

                let mut mult_h_r = vec![0.0; num_frames];

                for tau in 0..t_frames {
                    let shifted_h = &all_shifted_h[tau];
                    let shifted_q = &all_shifted_q[tau];

                    let mut h_row_sum = 0.0;
                    for m in 0..num_frames {
                        h_row_sum += shifted_h[r * num_frames + m];
                    }

                    let mut num_update_w_r = vec![0.0; num_bins];
                    let den_update_w_r = h_row_sum + EPS_F32;

                    for bin in 0..num_bins {
                        let mut sum_q_h = 0.0_f32;
                        for m in 0..num_frames {
                            sum_q_h = q[bin * num_frames + m]
                                .mul_add(shifted_h[r * num_frames + m], sum_q_h);
                        }
                        num_update_w_r[bin] = sum_q_h;
                    }

                    for bin in 0..num_bins {
                        tensor_w_r[bin * t_frames + tau] *= num_update_w_r[bin] / den_update_w_r;
                    }

                    let mut w_col_sum = 0.0;
                    for bin in 0..num_bins {
                        w_col_sum += tensor_w_r[bin * t_frames + tau];
                    }
                    let den_h = w_col_sum + EPS_F32;

                    for m in 0..num_frames {
                        let mut sum_w_q = 0.0_f32;
                        for bin in 0..num_bins {
                            sum_w_q = tensor_w_r[bin * t_frames + tau]
                                .mul_add(shifted_q[bin * num_frames + m], sum_w_q);
                        }
                        mult_h_r[m] += sum_w_q / den_h;
                    }
                }
                (tensor_w_r, mult_h_r)
            })
            .collect();

        let mut mult_h = vec![0.0; k * num_frames];
        for r in 0..k {
            let (tensor_w_r, mult_h_r) = &r_updates[r];
            for bin in 0..num_bins {
                for tau in 0..t_frames {
                    tensor_w[(bin * k * t_frames) + (r * t_frames) + tau] =
                        tensor_w_r[bin * t_frames + tau];
                }
            }
            for m in 0..num_frames {
                mult_h[r * num_frames + m] = mult_h_r[m];
            }
        }

        for r in 0..k {
            for m in 0..num_frames {
                h[r * num_frames + m] *= mult_h[r * num_frames + m] / (t_frames as f32);
            }
        }

        let norm_vec: Vec<f32> = (0..k)
            .into_par_iter()
            .map(|r| {
                let mut bin_sums = vec![0.0; num_bins];
                for bin in 0..num_bins {
                    let mut tau_sums = vec![0.0; t_frames];
                    for tau in 0..t_frames {
                        tau_sums[tau] = tensor_w[(bin * k * t_frames) + (r * t_frames) + tau];
                    }
                    bin_sums[bin] = pairwise_sum_f32(&tau_sums);
                }
                pairwise_sum_f32(&bin_sums)
            })
            .collect();

        for bin in 0..num_bins {
            for r in 0..k {
                let factor = 1.0 / (EPS_F32 + norm_vec[r]);
                for tau in 0..t_frames {
                    tensor_w[(bin * k * t_frames) + (r * t_frames) + tau] *= factor;
                }
            }
        }
    }

    (tensor_w, h, final_cost)
}

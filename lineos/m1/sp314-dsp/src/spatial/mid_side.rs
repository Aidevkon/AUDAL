pub struct MidSideMatrix;

impl MidSideMatrix {
    /// Encodes interleaved Stereo [L, R, L, R] into separate Mid and Side buffers
    pub fn encode(interleaved: &[f32]) -> (Vec<f32>, Vec<f32>) {
        let frames = interleaved.len() / 2;
        let mut mid = Vec::with_capacity(frames);
        let mut side = Vec::with_capacity(frames);

        for i in 0..frames {
            let l = interleaved[i * 2];
            let r = interleaved[i * 2 + 1];
            mid.push((l + r) * 0.5);
            side.push((l - r) * 0.5);
        }
        (mid, side)
    }

    /// Decodes separate Mid and Side buffers back to interleaved Stereo [L, R, L, R]
    pub fn decode(mid: &[f32], side: &[f32]) -> Vec<f32> {
        let frames = mid.len().min(side.len());
        let mut interleaved = Vec::with_capacity(frames * 2);

        for i in 0..frames {
            let m = mid[i];
            let s = side[i];
            interleaved.push(m + s); // L = M + S
            interleaved.push(m - s); // R = M - S
        }
        interleaved
    }
}

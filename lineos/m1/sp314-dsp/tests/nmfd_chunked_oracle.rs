use sp314_dsp::stft::nmf::NmfEngine;
use sp314_dsp::stft::nmfd::nmfd_f32;

// The Dragon Test: verifies that nmfd_component_mask_chunk (chunked)
// produces bit-identical results to a single monolithic mask generation.
#[test]
fn test_nmfd_chunked_vs_monolithic_mask() {
    let n_mels = 128;
    let n_bins = 1025;
    let k = 4;
    let tau_frames = 8;
    let n_frames = 100;

    let mut tensor_w = vec![0.0_f32; n_mels * k * tau_frames];
    for (i, val) in tensor_w.iter_mut().enumerate() {
        *val = (i as f32).sin().abs() + 0.1;
    }

    let mut full_h = vec![0.0_f32; k * n_frames];
    for (i, val) in full_h.iter_mut().enumerate() {
        *val = (i as f32).cos().abs() + 0.1;
    }

    let nmf = NmfEngine::new(k);

    // 1. Monolithic: run on entire full_h
    let mask_mono =
        nmf.nmfd_component_mask_chunk(0, &full_h, &tensor_w, n_frames, n_bins, tau_frames);

    // 2. Chunked with history: simulate two chunks.
    // Chunk 1: frames 0..50
    // Chunk 2: history frames (50 - pad)..(50) + core frames 50..100
    let pad_frames = 10; // > tau_frames (8)
    let split_frame = 50;

    // Check chunk 2 core frames (50..100) match monolithic exactly.
    let chunk2_len = (100 - split_frame) + pad_frames; // 60 frames
    let mut chunk2_h = vec![0.0_f32; k * chunk2_len];
    let chunk2_start_f = split_frame - pad_frames;

    for c in 0..k {
        for f in 0..chunk2_len {
            chunk2_h[c * chunk2_len + f] = full_h[c * n_frames + (chunk2_start_f + f)];
        }
    }

    let mask_chunk2 =
        nmf.nmfd_component_mask_chunk(0, &chunk2_h, &tensor_w, chunk2_len, n_bins, tau_frames);

    // Compare core frames
    for core_f in 0..(100 - split_frame) {
        let mono_f = split_frame + core_f;
        let chunk_f = pad_frames + core_f;

        let mono_frame = &mask_mono[mono_f];
        let chunk_frame = &mask_chunk2[chunk_f];

        for b in 0..n_bins {
            assert_eq!(
                mono_frame[b], chunk_frame[b],
                "Mismatch at frame {} (mono) / {} (chunk), bin {}",
                mono_f, chunk_f, b
            );
        }
    }
}

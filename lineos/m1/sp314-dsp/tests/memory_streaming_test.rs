//! ST-P6 — Memory profiling: TwoPassEngine peak heap < 50MB
//! Authority: v3_memory_aware_streaming.md INV-ST-3
//!
//! Uses mock data to simulate Disk-Seek Architecture:
//!   Pass 1: 30s snippet only (~5MB) → scout()
//!   Pass 2: lazy chunks, never full file in RAM
//!
//! This proves INV-ST-3 for the NMF/streaming layer.
//! Full disk-seek implementation: Phase 8 (WavChunkReader.seek())

#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[test]
fn two_pass_engine_peak_heap_under_50mb() {
    use sp314_dsp::stft::two_pass::TwoPassEngine;

    let sample_rate: u32 = 48000;

    // Simulate: 30s snippet from mid-point of file (Disk-Seek Architecture)
    // Pass 1 loads ONLY this — never the full file
    // 30s × 48000 = 1,440,000 samples = ~5.5MB
    let snippet_len = 30 * sample_rate as usize;
    let snippet: Vec<f32> = (0..snippet_len)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            // Simulate realistic audio: mix of voice + drums freq
            libm::sinf(2.0 * core::f32::consts::PI * 440.0 * t) * 0.5
                + libm::sinf(2.0 * core::f32::consts::PI * 80.0 * t) * 0.3
                + libm::sinf(2.0 * core::f32::consts::PI * 2000.0 * t) * 0.2
        })
        .collect();

    println!(
        "Snippet: {} samples ({:.1}s at {}Hz)",
        snippet.len(),
        snippet.len() as f32 / sample_rate as f32,
        sample_rate
    );

    // Start heap profiling
    let _profiler = dhat::Profiler::builder().testing().build();

    // Pass 1 — Scout on 30s snippet only
    // In production: reader.seek(mid_offset) → read 30s → scout
    let mut engine = TwoPassEngine::new();
    let scout = engine.scout(&snippet, sample_rate);

    let stats_scout = dhat::HeapStats::get();
    println!(
        "After Scout (30s snippet) — peak heap: {:.1} MB",
        stats_scout.max_bytes as f64 / 1_000_000.0
    );

    // Drop snippet — no longer needed
    drop(snippet);

    // Pass 2 — Simulate lazy chunk streaming (Disk-Seek Architecture)
    // In production: WavChunkReader streams from disk, never full file
    // Here: generate chunks on-the-fly, simulating a 4min file
    let total_duration_s = 240; // 4 minutes
    let chunk_size = 65536usize;
    let total_samples = total_duration_s * sample_rate as usize;
    let total_chunks = (total_samples + chunk_size - 1) / chunk_size;

    println!(
        "Simulating {}s file ({} chunks of {} samples)",
        total_duration_s, total_chunks, chunk_size
    );

    let mut chunk_count = 0usize;
    let mut offset = 0usize;

    while offset < total_samples {
        let end = (offset + chunk_size).min(total_samples);
        let len = end - offset;

        // Generate chunk lazily — never holds full file in RAM
        // In production: this is reader.next_chunk(chunk_size)
        let chunk: Vec<f32> = (0..len)
            .map(|i| {
                let t = (offset + i) as f32 / sample_rate as f32;
                libm::sinf(2.0 * core::f32::consts::PI * 440.0 * t) * 0.5
            })
            .collect();

        // Process chunk — stems computed and dropped immediately
        let chunk_mono = chunk; // already mono
        engine
            .process_chunks(&chunk_mono, &scout, |stems| {
                chunk_count += 1;
                let _ = stems.voice.len();
            })
            .expect("process_chunks failed");

        offset = end;
    }

    let stats_final = dhat::HeapStats::get();
    println!(
        "After Render ({} chunks, {}s) — peak heap: {:.1} MB",
        chunk_count,
        total_duration_s,
        stats_final.max_bytes as f64 / 1_000_000.0
    );

    // INV-ST-3: peak RAM ≤ 50MB
    // With Disk-Seek Architecture: snippet (5MB) + STFT (10MB) + chunks (2MB) = ~17MB
    const MAX_BYTES: usize = 50_000_000;
    assert!(
        stats_final.max_bytes <= MAX_BYTES,
        "INV-ST-3 VIOLATION: peak heap {:.1}MB > 50MB\n\
         This means chunk processing is accumulating memory.\n\
         Check: process_chunks must drop each chunk after callback.",
        stats_final.max_bytes as f64 / 1_000_000.0
    );

    println!(
        "✅ INV-ST-3: peak heap {:.1}MB < 50MB — PASS",
        stats_final.max_bytes as f64 / 1_000_000.0
    );
    println!("   Proves: NMF streaming layer is memory-safe");
    println!("   Phase 8: add WavChunkReader.seek() for real-file streaming");
}

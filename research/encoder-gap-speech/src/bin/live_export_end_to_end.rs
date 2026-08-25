//! F-083 blocking question, step 4 (adapted): "run the real daemon,
//! POST /master/streaming -> POST /export, listen/measure with ffmpeg."
//!
//! curl/HTTP to a locally-started daemon was denied by this session's
//! sandbox permissions, so this calls the SAME production functions
//! in-process instead of over HTTP: m0d::agents::executor::execute_streaming_plan
//! (exactly what handlers/master.rs's trigger_streaming spawns in the
//! background) and m0d::handlers::export::{export_wav, export_flac,
//! export_mp3_acx} (exactly what handlers/export.rs's export_audio calls).
//! No re-implementation anywhere in this file — only real production
//! entry points, on a real LibriSpeech narration FLAC.

fn main() {
    let input = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "research/w-speech/corpus/2277-149896-0000.flac".to_string());
    let input_abs = std::fs::canonicalize(&input)
        .unwrap_or_else(|e| panic!("cannot resolve input {input}: {e}"));
    println!("MEASURED input file: {}", input_abs.display());
    println!(
        "MEASURED input file size: {} bytes",
        std::fs::metadata(&input_abs).unwrap().len()
    );

    let plan = m0d::agents::operator::StreamingPlan {
        audio_path: input_abs.to_string_lossy().into_owned(),
        preset_id: "acx".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: "f083-live-export-recon".to_string(),
    };

    println!("--- calling m0d::agents::executor::execute_streaming_plan (REAL production fn) ---");
    let (output, blob) = match m0d::agents::executor::execute_streaming_plan(&plan, None) {
        Ok(v) => v,
        Err(e) => {
            println!("FAILED: execute_streaming_plan returned Err: {e:?}");
            std::process::exit(1);
        }
    };

    println!("MEASURED blob_id: {}", output.blob_id);
    println!("MEASURED num_frames: {}", output.num_frames);
    println!("MEASURED sample_rate: {}", output.sample_rate);
    println!("MEASURED output_lufs: {}", output.output_lufs);
    println!(
        "MEASURED blob.core.audio_path: {}",
        blob.core.audio_path.path().display()
    );
    println!("MEASURED blob.core.channels: {}", blob.core.channels);
    println!("MEASURED blob.core.sample_rate: {}", blob.core.sample_rate);

    let raw_audio_path_bytes = std::fs::read(blob.core.audio_path.path())
        .unwrap_or_else(|e| panic!("cannot read blob audio_path file: {e}"));
    println!(
        "MEASURED blob.core.audio_path file size on disk: {} bytes",
        raw_audio_path_bytes.len()
    );
    println!(
        "MEASURED first 4 bytes of that file (ASCII if RIFF): {:?} = {:?}",
        &raw_audio_path_bytes[0..4],
        String::from_utf8_lossy(&raw_audio_path_bytes[0..4])
    );

    // Now call the REAL /export code path: export_mp3_acx, exactly what
    // POST /export -> export_blob -> export_mp3_acx would run for this blob_id.
    let mp3_path = std::env::temp_dir().join(format!("f083-live-{}.mp3", output.blob_id));
    println!("--- calling m0d::handlers::export::export_mp3_acx (REAL production fn) ---");
    match m0d::handlers::export::export_mp3_acx(&blob, &mp3_path) {
        Ok(outcome) => {
            println!("MEASURED export_mp3_acx OK -> {}", mp3_path.display());
            println!(
                "MEASURED mp3 file size: {} bytes",
                std::fs::metadata(&mp3_path).unwrap().len()
            );
            println!("MEASURED AcxCheckReport: {:?}", outcome.report);
            println!(
                "MEASURED head_quiet_secs={} tail_quiet_secs={}",
                outcome.head_quiet_secs, outcome.tail_quiet_secs
            );
        }
        Err(e) => {
            println!("FAILED: export_mp3_acx returned Err: {e}");
        }
    }

    // Also FLAC export, which skips resampling/downmix entirely and so
    // shows the header-parse bug on the ORIGINAL 2ch/48k stream with
    // no obscuring by the mono downmix or resampler.
    let flac_path = std::env::temp_dir().join(format!("f083-live-{}.flac", output.blob_id));
    println!("--- calling m0d::handlers::export::export_flac (REAL production fn, no resample/downmix) ---");
    match m0d::handlers::export::export_flac(&blob, &flac_path) {
        Ok(()) => {
            println!("MEASURED export_flac OK -> {}", flac_path.display());
            println!(
                "MEASURED flac file size: {} bytes",
                std::fs::metadata(&flac_path).unwrap().len()
            );
        }
        Err(e) => println!("FAILED: export_flac returned Err: {e}"),
    }

    println!("DONE");
}

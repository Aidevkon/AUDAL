//! Βοηθητικό, ΓΙΑ ΑΥΤΟ ΤΟ TASK ΜΟΝΟ: πλήρης αλυσίδα render μέσω
//! execute_streaming_plan — η ΔΙΑΔΡΟΜΗ ΤΟΥ ΚΟΥΜΠΙΟΥ (/master/streaming),
//! ΟΧΙ το MasterRequest/dsp_pipeline::run_dsp (ορφανό). Πριν/μετά την
//! προσωρινή αλλαγή OTSU-TEMP-20260917 στο spectral_flux.rs. ΔΕΝ
//! αγγίζει παραγωγή το ίδιο — μόνο καλεί.
//!
//! Ανά τρέξιμο: (α) run_trunk_pass στο ίδιο dump — τα boundaries, το
//! ΙΔΙΟ που τροφοδοτεί pre_analysis μέσα στο execute_streaming_plan
//! (executor.rs:310/316/322) — και τον χρόνο του· (β)
//! execute_streaming_plan — το πλήρες master PCM (f32le stereo, 48k),
//! αντιγραμμένο πριν αγγιχτεί από τίποτα άλλο.
//!
//! ΧΡΗΣΗ: cargo run --release --bin otsu_render_pair -- <label> <input.mp3> <state:before|after>
use m0d::agents::executor::execute_streaming_plan;
use m0d::agents::operator::StreamingPlan;
use std::time::Instant;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let label = args[1].clone();
    let input_path = args[2].clone();
    let state = args[3].clone();

    // ── (α) run_trunk_pass — boundaries + χρόνος, ίδιο dump path που
    // θα χρησιμοποιήσει και το streaming pass0_decode_to_dump. ──
    let dump_path = format!("/tmp/otsu_pair_{label}.raw");
    let (_metrics, _decoder) = m0d::dsp::input_lufs::pass0_decode_to_dump(
        std::path::Path::new(&input_path),
        &dump_path,
    )
    .unwrap_or_else(|e| panic!("pass0_decode_to_dump {input_path}: {e}"));

    let t0 = Instant::now();
    let report = sp314_orchestrator::trunk_pass::run_trunk_pass(
        std::path::Path::new(&dump_path),
        false,
    )
    .unwrap_or_else(|e| panic!("run_trunk_pass {label}: {e}"));
    let trunk_elapsed = t0.elapsed().as_secs_f64();

    let boundaries_path = format!("/tmp/otsu_pair_{label}_{state}_boundaries.txt");
    let mut out = String::new();
    for b in &report.boundaries {
        out.push_str(&format!(
            "{:.3}\t{:.3}\t{:?}\t{:.4}\t{:.4}\n",
            b.start_sec, b.end_sec, b.segment_type, b.avg_leaning, b.avg_confidence
        ));
    }
    std::fs::write(&boundaries_path, &out).unwrap();

    // ── (β) execute_streaming_plan — το ΠΛΗΡΕΣ master, το κουμπί. ──
    let plan = StreamingPlan {
        audio_path: input_path.clone(),
        preset_id: "spotify".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: format!("otsu-pair-{label}-{state}"),
    };

    let t1 = Instant::now();
    let (output, _blob) = execute_streaming_plan(&plan, None)
        .unwrap_or_else(|e| panic!("execute_streaming_plan {label}: {e:?}"));
    let stream_elapsed = t1.elapsed().as_secs_f64();

    let mastered_path = m0d::blob_store::mastered_path(&output.blob_id);
    let saved_path = format!("/tmp/otsu_pair_{label}_{state}.pcm");
    std::fs::copy(&mastered_path, &saved_path)
        .unwrap_or_else(|e| panic!("copy {} -> {saved_path}: {e}", mastered_path.display()));

    println!(
        "label={label} state={state} boundaries_n={} boundaries_file={boundaries_path} \
         trunk_pass_s={trunk_elapsed:.3} streaming_total_s={stream_elapsed:.3} \
         pcm={saved_path} pcm_blake3={} num_frames={} sample_rate={}",
        report.boundaries.len(),
        output.pcm_blake3,
        output.num_frames,
        output.sample_rate
    );
}

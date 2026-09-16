//! Βοηθητικό, ΓΙΑ ΑΥΤΟ ΤΟ TASK ΜΟΝΟ: πλήρης αλυσίδα render μέσω
//! execute_streaming_plan — η ΔΙΑΔΡΟΜΗ ΤΟΥ ΚΟΥΜΠΙΟΥ (/master/streaming
//! → Intent::RunStreaming → executor::execute_streaming_plan),
//! ΟΧΙ το MasterRequest/dsp_pipeline::run_dsp (ορφανό — 3 scripts +
//! browser fallback). Πριν/μετά την προσωρινή αλλαγή
//! TEMP_FLATNESS_RULE_20260917 στο trunk_pass.rs. ΔΕΝ αγγίζει
//! παραγωγή το ίδιο.
//! ΧΡΗΣΗ: cargo run --release --bin temp_render_probe_streaming -- <input.wav> <label>
use m0d::agents::executor::execute_streaming_plan;
use m0d::agents::operator::StreamingPlan;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let input_path = args[1].clone();
    let label = args[2].clone();

    let plan = StreamingPlan {
        audio_path: input_path,
        preset_id: "spotify".to_string(),
        flavour_id: None,
        intent_tone: None,
        intent_dynamics: None,
        target_lufs_override: None,
        session_id: format!("temp-flatness-streaming-{label}"),
    };

    let (output, _blob) = execute_streaming_plan(&plan, None)
        .unwrap_or_else(|e| panic!("execute_streaming_plan failed for {label}: {e:?}"));

    let mastered_path = m0d::blob_store::mastered_path(&output.blob_id);
    let saved_path = format!("/tmp/stream_saved_{label}.pcm");
    std::fs::copy(&mastered_path, &saved_path)
        .unwrap_or_else(|e| panic!("copy {} -> {saved_path}: {e}", mastered_path.display()));
    println!(
        "label={label} blob_id={} pcm_blake3={} saved_path={saved_path} num_frames={} sample_rate={}",
        output.blob_id, output.pcm_blake3, output.num_frames, output.sample_rate
    );
}

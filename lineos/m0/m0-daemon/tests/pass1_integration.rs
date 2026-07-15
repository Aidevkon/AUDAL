use m0d::handlers::decode::decode_raw_interleaved;
use m0d::handlers::decode_actor::decode_streaming;
use sp314_dsp::io::decode_types::{DecodeChunk, DecodeError};
use sp314_orchestrator::decode_provider::{DecodeProvider, WholeBufferProvider};
use sp314_orchestrator::pass1_pipeline::build_timeline_map;

pub struct FileDecoder {
    pub path: String,
}

impl DecodeProvider for FileDecoder {
    fn stream_to<E, F>(&self, on_chunk: F) -> Result<(u32, u16), DecodeError>
    where
        E: ToString,
        F: FnMut(DecodeChunk<'_>) -> Result<(), E>,
    {
        decode_streaming(&self.path, on_chunk)
    }
}

impl WholeBufferProvider for FileDecoder {
    fn decode_to_memory(&self) -> Result<(Vec<f32>, u32, u16), DecodeError> {
        decode_raw_interleaved(&self.path)
    }
}

#[test]
#[ignore]
fn test_build_timeline_map() {
    let path = "../../../flight_clips_stereo/clip_transition_st.wav";
    let decoder = FileDecoder {
        path: path.to_string(),
    };
    let map = build_timeline_map(decoder).unwrap();

    println!("=== Pass 1: Timeline Map ===");
    for (i, b) in map.iter().enumerate() {
        println!(
            "[Segment {}] {:.2}s -> {:.2}s : {:?}",
            i, b.start_sec, b.end_sec, b.segment_type
        );
    }

    assert_eq!(
        map.len(),
        2,
        "Expected exactly 2 segments for the transition clip"
    );

    // Assert boundary is near the known ~15-18s region
    let transition_time = map[0].end_sec;
    assert!(
        transition_time >= 15.0 && transition_time < 18.0,
        "Expected transition boundary between 15s and 18s, got {:.2}s",
        transition_time
    );
}

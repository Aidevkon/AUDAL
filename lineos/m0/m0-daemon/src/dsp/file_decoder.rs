use sp314_dsp::io::decode_types::{DecodeChunk, DecodeError};
use sp314_orchestrator::decode_provider::{DecodeProvider, WholeBufferProvider};

pub struct FileDecoder {
    pub path: String,
}

impl DecodeProvider for FileDecoder {
    fn stream_to<E, F>(&self, on_chunk: F) -> Result<(u32, u16), DecodeError>
    where
        E: ToString,
        F: FnMut(DecodeChunk<'_>) -> Result<(), E>,
    {
        crate::handlers::decode_actor::decode_streaming(&self.path, on_chunk)
    }
}

impl WholeBufferProvider for FileDecoder {
    fn decode_to_memory(&self) -> Result<(Vec<f32>, u32, u16), DecodeError> {
        crate::handlers::decode::decode_raw_interleaved(&self.path)
    }
}

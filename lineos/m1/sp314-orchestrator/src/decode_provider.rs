use sp314_dsp::io::decode_types::{DecodeChunk, DecodeError};

pub trait DecodeProvider {
    fn stream_to<E, F>(&self, on_chunk: F) -> Result<(u32, u16), DecodeError>
    where
        E: ToString,
        F: FnMut(DecodeChunk<'_>) -> Result<(), E>;
}

pub trait WholeBufferProvider {
    fn decode_to_memory(&self) -> Result<(Vec<f32>, u32, u16), DecodeError>;
}

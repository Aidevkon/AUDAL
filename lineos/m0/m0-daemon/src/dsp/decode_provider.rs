use crate::handlers::decode::DecodeError;
use crate::handlers::decode_actor::DecodeChunk;

pub trait DecodeProvider {
    fn stream_to<E, F>(&self, on_chunk: F) -> Result<(u32, u16), DecodeError>
    where
        E: ToString,
        F: FnMut(DecodeChunk<'_>) -> Result<(), E>;
}

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

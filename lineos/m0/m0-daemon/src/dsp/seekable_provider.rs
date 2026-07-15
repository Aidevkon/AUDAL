use crate::dsp::lazy_reader::LazyReaderError;

pub trait AudioMetadataProvider {
    fn sample_rate(&self) -> u32;
    fn channels(&self) -> usize;
    fn total_frames_hint(&self) -> Option<u64>;
}

pub trait ApproximateSeekProvider: AudioMetadataProvider {
    fn seek_approximate(
        &mut self,
        target: std::time::Duration,
    ) -> Result<std::time::Duration, LazyReaderError>;
    fn fill_buffer(&mut self, buffer: &mut [f32]) -> Result<usize, LazyReaderError>;
}

pub trait ExactSeekProvider: AudioMetadataProvider {
    fn seek_exact_frame(&mut self, target_frame: u64) -> Result<(), LazyReaderError>;
    fn read_exact_frames_alloc(
        &mut self,
        frames: u64,
    ) -> Result<(Vec<f32>, Vec<f32>), LazyReaderError>;
}

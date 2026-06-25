// src/io/wav_writer.rs

/// Simple WAV writer using `hound`.
pub struct WavWriter;

impl WavWriter {
    /// Writes left and right channels to a 32-bit float stereo WAV file.
    pub fn write(
        path: &str,
        left: &[f32],
        right: &[f32],
        sample_rate: u32,
    ) -> Result<(), Box<dyn std::error::Error>> {
        if left.len() != right.len() {
            return Err("Left and right channels must have the same length".into());
        }

        let spec = hound::WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };

        let mut writer = hound::WavWriter::create(path, spec)?;

        for i in 0..left.len() {
            writer.write_sample(left[i])?;
            writer.write_sample(right[i])?;
        }

        writer.finalize()?;
        Ok(())
    }
}

/// Writes a WAVE_FORMAT_EXTENSIBLE header for N-channel float PCM, with the
/// correct dwChannelMask per Microsoft KSAUDIO_CHANNEL_CONFIG convention.
/// For 5.1 (6 channels): mask = 0x3F (FL|FR|FC|LFE|BL|BR), which matches
/// our existing [L,R,C,LFE,Ls,Rs] channel order in AudioPayload::FiveDotOne
/// (confirmed: WAVE_FORMAT_EXTENSIBLE requires interleave order to follow
/// mask bits LSB-to-MSB, which is exactly L,R,C,LFE,Ls,Rs — no reordering
/// needed at this layer).
pub fn write_extensible_fmt_chunk<W: std::io::Write>(
    writer: &mut W,
    num_channels: u16,
    sample_rate: u32,
    channel_mask: u32,
) -> std::io::Result<()> {
    // fmt chunk for WAVE_FORMAT_EXTENSIBLE: 40 bytes total (18 base + 22 extension)
    
    let bits_per_sample: u16 = 32;
    let bytes_per_sample = bits_per_sample / 8;
    let block_align = num_channels * bytes_per_sample;
    let avg_bytes_per_sec = sample_rate * (block_align as u32);

    writer.write_all(b"fmt ")?;
    writer.write_all(&40_u32.to_le_bytes())?; // Chunk size (40 bytes for Extensible)
    
    // WAVEFORMATEX base
    writer.write_all(&0xFFFE_u16.to_le_bytes())?; // wFormatTag = WAVE_FORMAT_EXTENSIBLE
    writer.write_all(&num_channels.to_le_bytes())?; // nChannels
    writer.write_all(&sample_rate.to_le_bytes())?; // nSamplesPerSec
    writer.write_all(&avg_bytes_per_sec.to_le_bytes())?; // nAvgBytesPerSec
    writer.write_all(&block_align.to_le_bytes())?; // nBlockAlign
    writer.write_all(&bits_per_sample.to_le_bytes())?; // wBitsPerSample
    writer.write_all(&22_u16.to_le_bytes())?; // cbSize (size of extension)
    
    // WAVEFORMATEXTENSIBLE extension
    writer.write_all(&bits_per_sample.to_le_bytes())?; // wValidBitsPerSample
    writer.write_all(&channel_mask.to_le_bytes())?; // dwChannelMask
    
    // SubFormat GUID: KSDATAFORMAT_SUBTYPE_IEEE_FLOAT
    // {00000003-0000-0010-8000-00aa00389b71}
    writer.write_all(&[
        0x03, 0x00, 0x00, 0x00, // Data1
        0x00, 0x00,             // Data2
        0x10, 0x00,             // Data3
        0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71 // Data4
    ])?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extensible_fmt_chunk_matches_5_1_spec_byte_for_byte() {
        let mut buf: Vec<u8> = Vec::new();
        let channel_mask: u32 = 0x3F; // FL|FR|FC|LFE|BL|BR
        write_extensible_fmt_chunk(&mut buf, 6, 48000, channel_mask).unwrap();

        // Total: 8 byte chunk header + 40 byte chunk data = 48 bytes
        assert_eq!(buf.len(), 48, "fmt chunk should be exactly 48 bytes (8 header + 40 data)");

        // Chunk header
        assert_eq!(&buf[0..4], b"fmt ");
        assert_eq!(u32::from_le_bytes(buf[4..8].try_into().unwrap()), 40);

        // WAVEFORMATEX base
        assert_eq!(u16::from_le_bytes(buf[8..10].try_into().unwrap()), 0xFFFE); // WAVE_FORMAT_EXTENSIBLE
        assert_eq!(u16::from_le_bytes(buf[10..12].try_into().unwrap()), 6);    // nChannels
        assert_eq!(u32::from_le_bytes(buf[12..16].try_into().unwrap()), 48000); // nSamplesPerSec
        assert_eq!(u32::from_le_bytes(buf[16..20].try_into().unwrap()), 1_152_000); // nAvgBytesPerSec
        assert_eq!(u16::from_le_bytes(buf[20..22].try_into().unwrap()), 24);   // nBlockAlign
        assert_eq!(u16::from_le_bytes(buf[22..24].try_into().unwrap()), 32);   // wBitsPerSample
        assert_eq!(u16::from_le_bytes(buf[24..26].try_into().unwrap()), 22);   // cbSize

        // WAVEFORMATEXTENSIBLE extension
        assert_eq!(u16::from_le_bytes(buf[26..28].try_into().unwrap()), 32);   // wValidBitsPerSample
        assert_eq!(u32::from_le_bytes(buf[28..32].try_into().unwrap()), 0x3F); // dwChannelMask

        // SubFormat GUID — KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, confirmed via web search
        // against Microsoft's official struct definition: {0x00000003,0x0000,0x0010,
        // {0x80,0x00},{0x00,0xaa,0x00,0x38,0x9b,0x71}}
        let expected_guid: [u8; 16] = [
            0x03, 0x00, 0x00, 0x00, // Data1, little-endian
            0x00, 0x00,             // Data2, little-endian
            0x10, 0x00,             // Data3, little-endian
            0x80, 0x00,             // Data4[0..2], as-is
            0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71, // Data4[2..8], as-is
        ];
        assert_eq!(&buf[32..48], &expected_guid[..]);
    }
}

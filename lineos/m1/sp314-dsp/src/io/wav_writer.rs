// allow: frame-major 6-channel interleave loops; column access
// across 6 planar buffers has no cleaner iterator form.
#![allow(clippy::needless_range_loop)]

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
        0x00, 0x00, // Data2
        0x10, 0x00, // Data3
        0x80, 0x00, 0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71, // Data4
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
        assert_eq!(
            buf.len(),
            48,
            "fmt chunk should be exactly 48 bytes (8 header + 40 data)"
        );

        // Chunk header
        assert_eq!(&buf[0..4], b"fmt ");
        assert_eq!(u32::from_le_bytes(buf[4..8].try_into().unwrap()), 40);

        // WAVEFORMATEX base
        assert_eq!(u16::from_le_bytes(buf[8..10].try_into().unwrap()), 0xFFFE); // WAVE_FORMAT_EXTENSIBLE
        assert_eq!(u16::from_le_bytes(buf[10..12].try_into().unwrap()), 6); // nChannels
        assert_eq!(u32::from_le_bytes(buf[12..16].try_into().unwrap()), 48000); // nSamplesPerSec
        assert_eq!(
            u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            1_152_000
        ); // nAvgBytesPerSec
        assert_eq!(u16::from_le_bytes(buf[20..22].try_into().unwrap()), 24); // nBlockAlign
        assert_eq!(u16::from_le_bytes(buf[22..24].try_into().unwrap()), 32); // wBitsPerSample
        assert_eq!(u16::from_le_bytes(buf[24..26].try_into().unwrap()), 22); // cbSize

        // WAVEFORMATEXTENSIBLE extension
        assert_eq!(u16::from_le_bytes(buf[26..28].try_into().unwrap()), 32); // wValidBitsPerSample
        assert_eq!(u32::from_le_bytes(buf[28..32].try_into().unwrap()), 0x3F); // dwChannelMask

        // SubFormat GUID — KSDATAFORMAT_SUBTYPE_IEEE_FLOAT, confirmed via web search
        // against Microsoft's official struct definition: {0x00000003,0x0000,0x0010,
        // {0x80,0x00},{0x00,0xaa,0x00,0x38,0x9b,0x71}}
        let expected_guid: [u8; 16] = [
            0x03, 0x00, 0x00, 0x00, // Data1, little-endian
            0x00, 0x00, // Data2, little-endian
            0x10, 0x00, // Data3, little-endian
            0x80, 0x00, // Data4[0..2], as-is
            0x00, 0xaa, 0x00, 0x38, 0x9b, 0x71, // Data4[2..8], as-is
        ];
        assert_eq!(&buf[32..48], &expected_guid[..]);
    }
}

pub struct StreamingWavWriter {
    writer: hound::WavWriter<std::io::BufWriter<std::fs::File>>,
}

impl StreamingWavWriter {
    pub fn new(path: &str, sample_rate: u32) -> Result<Self, Box<dyn std::error::Error>> {
        let spec = hound::WavSpec {
            channels: 2,
            sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let writer = hound::WavWriter::create(path, spec)?;
        Ok(Self { writer })
    }

    pub fn write_chunk(
        &mut self,
        left: &[f32],
        right: &[f32],
    ) -> Result<(), Box<dyn std::error::Error>> {
        if left.len() != right.len() {
            return Err("Left and right channels must have the same length".into());
        }
        for i in 0..left.len() {
            self.writer.write_sample(left[i])?;
            self.writer.write_sample(right[i])?;
        }
        Ok(())
    }

    pub fn finalize(self) -> Result<(), Box<dyn std::error::Error>> {
        self.writer.finalize()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests_streaming {
    use super::*;

    #[test]
    fn streaming_writer_chunked_matches_batch_writer() {
        let sr = 48000;
        let left: Vec<f32> = (0..500).map(|i| (i as f32 * 0.01).sin() * 0.5).collect();
        let right: Vec<f32> = (0..500).map(|i| (i as f32 * 0.013).sin() * 0.5).collect();

        let batch_path = "/tmp/test_batch_writer.wav";
        WavWriter::write(batch_path, &left, &right, sr).unwrap();

        let stream_path = "/tmp/test_streaming_writer.wav";
        let mut sw = StreamingWavWriter::new(stream_path, sr).unwrap();
        sw.write_chunk(&left[0..100], &right[0..100]).unwrap();
        sw.write_chunk(&left[100..350], &right[100..350]).unwrap();
        sw.write_chunk(&left[350..500], &right[350..500]).unwrap();
        sw.finalize().unwrap();

        let batch_bytes = std::fs::read(batch_path).unwrap();
        let stream_bytes = std::fs::read(stream_path).unwrap();
        assert_eq!(
            batch_bytes, stream_bytes,
            "Streaming chunks should produce exact same file as batch"
        );
    }
}

/// Writes the `chna` chunk (Channel Audio Definition) required by ADM BWF.
///
/// The chna chunk maps audio tracks to ADM audioTrackUIDs.
/// For a 5.1 bed with 6 channels, we define 6 tracks mapped to
/// 1 audioPackFormat (AP_00010009 = 5.1).
///
/// Format per EBU Tech 3364 / Dolby spec:
///   numTracks (u16) + numUIDs (u16)
///   then per-UID: trackIndex(u16) + audioTrackUID(12 bytes) +
///   audioTrackFormatID(14 bytes) + audioPackFormatID(11 bytes) + pad(1)
pub fn write_chna_chunk<W: std::io::Write>(w: &mut W) -> Result<(), String> {
    // 5.1 bed: 6 tracks, 6 UIDs
    let num_tracks: u16 = 6;
    let num_uids: u16 = 6;

    // Per-UID record = 40 bytes
    // Total chunk data = 4 + 6*40 = 244 bytes
    let chunk_size: u32 = 4 + (6 * 40);

    w.write_all(b"chna").map_err(|e| e.to_string())?;
    w.write_all(&chunk_size.to_le_bytes())
        .map_err(|e| e.to_string())?;
    w.write_all(&num_tracks.to_le_bytes())
        .map_err(|e| e.to_string())?;
    w.write_all(&num_uids.to_le_bytes())
        .map_err(|e| e.to_string())?;

    // Channel order: L R C LFE Ls Rs
    // audioTrackFormatIDs for 5.1 bed
    let track_format_ids: [&[u8; 14]; 6] = [
        b"AT_00010001_01", // L
        b"AT_00010002_01", // R
        b"AT_00010003_01", // C
        b"AT_00010004_01", // LFE
        b"AT_00010005_01", // Ls
        b"AT_00010006_01", // Rs
    ];

    let pack_format_id = b"AP_00010009";

    for i in 0..6u16 {
        // trackIndex (1-based)
        w.write_all(&(i + 1).to_le_bytes())
            .map_err(|e| e.to_string())?;
        // audioTrackUID (12 bytes): ATU_xxxxxxxx format, zero-padded
        let uid = format!("ATU_{:08}", i + 1);
        let uid_bytes = uid.as_bytes();
        let mut uid_padded = [0u8; 12];
        uid_padded[..uid_bytes.len()].copy_from_slice(uid_bytes);
        w.write_all(&uid_padded).map_err(|e| e.to_string())?;
        // audioTrackFormatID (14 bytes)
        w.write_all(track_format_ids[i as usize])
            .map_err(|e| e.to_string())?;
        // audioPackFormatID (11 bytes)
        w.write_all(pack_format_id).map_err(|e| e.to_string())?;
        // pad to 40 bytes: 2+12+14+11 = 39, +1 pad = 40
        w.write_all(&[0u8]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Writes minimal `axml` chunk with ADM XML for a 5.1 bed.
///
/// The axml chunk contains an ITU-R BS.2076 compliant XML document
/// identifying the audioPackFormat as a 5.1 bed (AP_00010009).
/// Apple requires this for Spatial Audio recognition.
pub fn write_axml_chunk<W: std::io::Write>(w: &mut W) -> Result<(), String> {
    let xml = br#"<?xml version="1.0" encoding="UTF-8"?>
<ebuCoreMain xmlns="urn:ebu:metadata-schema:ebuCore_2014"
xmlns:dc="http://purl.org/dc/elements/1.1/"
xsi:schemaLocation="urn:ebu:metadata-schema:ebuCore_2014 EBU_CORE_20140201.xsd"
xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance">
<coreMetadata>
<format>
<audioFormatExtended>
<audioTrack trackID="1" formatLabel="Left"/>
<audioTrack trackID="2" formatLabel="Right"/>
<audioTrack trackID="3" formatLabel="Centre"/>
<audioTrack trackID="4" formatLabel="LFE"/>
<audioTrack trackID="5" formatLabel="Left Surround"/>
<audioTrack trackID="6" formatLabel="Right Surround"/>
<audioPackFormat audioPackFormatID="AP_00010009"
audioPackFormatName="DolbyAtmos_5.1"
typeLabel="0001" typeDefinition="DirectSpeakers"/>
</audioFormatExtended>
</format>
</coreMetadata>
</ebuCoreMain>"#;

    let chunk_size = xml.len() as u32;
    // Pad to even boundary
    let padded = chunk_size + (chunk_size % 2);

    w.write_all(b"axml").map_err(|e| e.to_string())?;
    w.write_all(&padded.to_le_bytes())
        .map_err(|e| e.to_string())?;
    w.write_all(xml).map_err(|e| e.to_string())?;
    if !chunk_size.is_multiple_of(2) {
        w.write_all(&[0u8]).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Writes a complete ADM BWF file for Apple Spatial Audio.
///
/// Format: RIFF container with chunks in this order:
///   fmt  (WAVE_FORMAT_EXTENSIBLE, 24-bit LPCM)
///   bext (BWF broadcast extension, EBU Tech 3285 v2)
///   chna (channel audio definition, EBU Tech 3364)
///   axml (ADM XML metadata, ITU-R BS.2076)
///   data (interleaved 24-bit LPCM samples)
///
/// Apple requirements:
///   - 48kHz / 24-bit LPCM
///   - Channel order: L R C LFE Ls Rs (mask 0x3F)
///   - bext.TimeReference at 24fps
///   - chna + axml required for Spatial Audio recognition
///
/// `channels` must be in planar format: [L, R, C, LFE, Ls, Rs]
/// each Vec<f32> has `num_frames` samples normalized -1.0..1.0
pub fn write_adm_bwf(
    path: &str,
    channels: &[Vec<f32>; 6],
    sample_rate: u32,
    num_frames: usize,
) -> Result<(), String> {
    use std::io::Write;

    // Convert f32 planar → interleaved 24-bit signed integers
    // L[0] R[0] C[0] LFE[0] Ls[0] Rs[0] L[1] R[1] ...
    let mut pcm_24bit: Vec<u8> = Vec::with_capacity(num_frames * 6 * 3);
    for i in 0..num_frames {
        for ch in 0..6 {
            // Clamp, scale to i24 range, write 3 bytes LE
            let sample = channels[ch][i].clamp(-1.0, 1.0);
            let as_i32 = (sample * 8_388_607.0_f32) as i32;
            pcm_24bit.push((as_i32 & 0xFF) as u8);
            pcm_24bit.push(((as_i32 >> 8) & 0xFF) as u8);
            pcm_24bit.push(((as_i32 >> 16) & 0xFF) as u8);
        }
    }

    // --- Pre-render chna + axml to buffers for size calculation ---
    let mut chna_buf: Vec<u8> = Vec::new();
    write_chna_chunk(&mut chna_buf)?;
    let mut axml_buf: Vec<u8> = Vec::new();
    write_axml_chunk(&mut axml_buf)?;

    // --- RIFF chunk sizes ---
    let fmt_chunk_size: u32 = 40; // WAVE_FORMAT_EXTENSIBLE
    let bext_chunk_size: u32 = 602; // minimum BWF bext (EBU Tech 3285)
    let data_chunk_size: u32 = pcm_24bit.len() as u32;

    // RIFF size = 4 (WAVE) + 8+fmt + 8+bext + chna_buf + axml_buf + 8+data
    let riff_size: u32 = 4
        + (8 + fmt_chunk_size)
        + (8 + bext_chunk_size)
        + chna_buf.len() as u32
        + axml_buf.len() as u32
        + (8 + data_chunk_size);

    let mut file =
        std::fs::File::create(path).map_err(|e| format!("ADM BWF create failed: {e}"))?;
    let w = &mut file;

    // --- RIFF header ---
    w.write_all(b"RIFF").map_err(|e| e.to_string())?;
    w.write_all(&riff_size.to_le_bytes())
        .map_err(|e| e.to_string())?;
    w.write_all(b"WAVE").map_err(|e| e.to_string())?;

    // --- fmt chunk (WAVE_FORMAT_EXTENSIBLE, 24-bit LPCM) ---
    w.write_all(b"fmt ").map_err(|e| e.to_string())?;
    w.write_all(&fmt_chunk_size.to_le_bytes())
        .map_err(|e| e.to_string())?;
    let n_block_align: u16 = 6 * 3; // 6ch * 3 bytes
    let n_avg_bytes: u32 = sample_rate * n_block_align as u32;
    w.write_all(&0xFFFE_u16.to_le_bytes())
        .map_err(|e| e.to_string())?; // wFormatTag = WAVE_FORMAT_EXTENSIBLE
    w.write_all(&6_u16.to_le_bytes())
        .map_err(|e| e.to_string())?; // nChannels
    w.write_all(&sample_rate.to_le_bytes())
        .map_err(|e| e.to_string())?; // nSamplesPerSec
    w.write_all(&n_avg_bytes.to_le_bytes())
        .map_err(|e| e.to_string())?; // nAvgBytesPerSec
    w.write_all(&n_block_align.to_le_bytes())
        .map_err(|e| e.to_string())?; // nBlockAlign
    w.write_all(&24_u16.to_le_bytes())
        .map_err(|e| e.to_string())?; // wBitsPerSample
    w.write_all(&22_u16.to_le_bytes())
        .map_err(|e| e.to_string())?; // cbSize
    w.write_all(&24_u16.to_le_bytes())
        .map_err(|e| e.to_string())?; // wValidBitsPerSample
    w.write_all(&0x3F_u32.to_le_bytes())
        .map_err(|e| e.to_string())?; // dwChannelMask (FL|FR|FC|LFE|BL|BR)
                                      // KSDATAFORMAT_SUBTYPE_PCM GUID {00000001-0000-0010-8000-00AA00389B71}
    w.write_all(&[
        0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38, 0x9B,
        0x71,
    ])
    .map_err(|e| e.to_string())?;

    // --- bext chunk (BWF Broadcast Extension, EBU Tech 3285 v2) ---
    // Minimum 602 bytes: 256+32+32+10+8+8+2+64+10+180 = 602
    w.write_all(b"bext").map_err(|e| e.to_string())?;
    w.write_all(&bext_chunk_size.to_le_bytes())
        .map_err(|e| e.to_string())?;
    // Description (256 bytes, null-padded)
    let desc = b"Creator OS \xe2\x80\x94 Apple Spatial Audio";
    let mut desc_padded = [0u8; 256];
    let len = desc.len().min(256);
    desc_padded[..len].copy_from_slice(&desc[..len]);
    w.write_all(&desc_padded).map_err(|e| e.to_string())?;
    // Originator (32 bytes)
    let orig = b"Creator OS M0 Daemon";
    let mut orig_padded = [0u8; 32];
    let olen = orig.len().min(32);
    orig_padded[..olen].copy_from_slice(&orig[..olen]);
    w.write_all(&orig_padded).map_err(|e| e.to_string())?;
    // OriginatorReference (32 bytes, null)
    w.write_all(&[0u8; 32]).map_err(|e| e.to_string())?;
    // OriginationDate (10 bytes "YYYY-MM-DD")
    w.write_all(b"2025-01-01").map_err(|e| e.to_string())?;
    // OriginationTime (8 bytes "HH:MM:SS")
    w.write_all(b"00:00:00").map_err(|e| e.to_string())?;
    // TimeReference low + high (8 bytes total, = 0 — no FFOA for music)
    w.write_all(&0_u64.to_le_bytes())
        .map_err(|e| e.to_string())?;
    // Version (2 bytes, = 2 for BWF v2)
    w.write_all(&2_u16.to_le_bytes())
        .map_err(|e| e.to_string())?;
    // UMID (64 bytes, null — filled by distributor)
    w.write_all(&[0u8; 64]).map_err(|e| e.to_string())?;
    // LoudnessValue, LoudnessRange, MaxTruePeakLevel,
    // MaxMomentaryLoudness, MaxShortTermLoudness
    // (10 bytes total, 5 × i16 — 0x7FFF = "not specified")
    for _ in 0..5 {
        w.write_all(&0x7FFF_i16.to_le_bytes())
            .map_err(|e| e.to_string())?;
    }
    // Reserved (180 bytes, null)
    w.write_all(&[0u8; 180]).map_err(|e| e.to_string())?;

    // --- chna chunk (pre-rendered) ---
    w.write_all(&chna_buf).map_err(|e| e.to_string())?;

    // --- axml chunk (pre-rendered) ---
    w.write_all(&axml_buf).map_err(|e| e.to_string())?;

    // --- data chunk ---
    w.write_all(b"data").map_err(|e| e.to_string())?;
    w.write_all(&data_chunk_size.to_le_bytes())
        .map_err(|e| e.to_string())?;
    w.write_all(&pcm_24bit).map_err(|e| e.to_string())?;

    // Pad to even byte boundary if needed
    if !data_chunk_size.is_multiple_of(2) {
        w.write_all(&[0u8]).map_err(|e| e.to_string())?;
    }

    Ok(())
}

#[cfg(test)]
mod tests_adm_bwf {
    use super::*;

    #[test]
    fn adm_bwf_produces_valid_riff_header() {
        let path = "/tmp/test_adm_bwf_header.wav";
        let num_frames = 100;
        let channels: [Vec<f32>; 6] = std::array::from_fn(|ch| {
            (0..num_frames)
                .map(|i| (i as f32 * 0.01 * (ch as f32 + 1.0)).sin() * 0.5)
                .collect()
        });

        write_adm_bwf(path, &channels, 48000, num_frames).unwrap();

        let bytes = std::fs::read(path).unwrap();
        // RIFF header
        assert_eq!(&bytes[0..4], b"RIFF");
        assert_eq!(&bytes[8..12], b"WAVE");
        // fmt chunk
        assert_eq!(&bytes[12..16], b"fmt ");
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 40);
        // WAVE_FORMAT_EXTENSIBLE tag
        assert_eq!(
            u16::from_le_bytes(bytes[20..22].try_into().unwrap()),
            0xFFFE
        );
        // 6 channels
        assert_eq!(u16::from_le_bytes(bytes[22..24].try_into().unwrap()), 6);
        // 48kHz
        assert_eq!(u32::from_le_bytes(bytes[24..28].try_into().unwrap()), 48000);
        // 24-bit
        assert_eq!(u16::from_le_bytes(bytes[34..36].try_into().unwrap()), 24);
        // bext chunk present
        assert_eq!(&bytes[60..64], b"bext");
        assert_eq!(u32::from_le_bytes(bytes[64..68].try_into().unwrap()), 602);
        // chna chunk after bext
        let chna_offset = 60 + 8 + 602;
        assert_eq!(&bytes[chna_offset..chna_offset + 4], b"chna");
        let chna_size =
            u32::from_le_bytes(bytes[chna_offset + 4..chna_offset + 8].try_into().unwrap())
                as usize;
        assert_eq!(chna_size, 244);
        // axml chunk after chna
        let axml_offset = chna_offset + 8 + chna_size;
        assert_eq!(&bytes[axml_offset..axml_offset + 4], b"axml");
        let axml_size =
            u32::from_le_bytes(bytes[axml_offset + 4..axml_offset + 8].try_into().unwrap())
                as usize;
        // data chunk after axml (with even-byte padding)
        let axml_padded = axml_size + (axml_size % 2);
        let data_offset = axml_offset + 8 + axml_padded;
        assert_eq!(&bytes[data_offset..data_offset + 4], b"data");
        // Data size = num_frames * 6 channels * 3 bytes
        let expected_data_size = (num_frames * 6 * 3) as u32;
        assert_eq!(
            u32::from_le_bytes(bytes[data_offset + 4..data_offset + 8].try_into().unwrap()),
            expected_data_size
        );
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdmContainerFormat {
    Riff32,
    Bw64,
}

pub struct AdmBwfStreamWriter {
    file: std::fs::File,
    expected_data_bytes: u64,
    written_data_bytes: u64,
}

impl AdmBwfStreamWriter {
    /// Writes the COMPLETE header (RIFF/fmt/bext/chna/axml/data tag +
    /// size) — byte-identical to write_adm_bwf's header for the same
    /// num_frames/sample_rate. Riff32 errors if sizes exceed u32::MAX
    /// (pointing to Bw64); Bw64 writes a ds64 chunk with u64 sizes —
    /// pending consumer validation before becoming the export default
    /// (P39).
    pub fn create(
        path: &str,
        sample_rate: u32,
        num_frames: usize,
        format: AdmContainerFormat,
    ) -> Result<Self, String> {
        use std::io::Write;

        let data_chunk_size = (num_frames * 6 * 3) as u64;

        // --- Pre-render chna + axml to buffers for size calculation ---
        let mut chna_buf: Vec<u8> = Vec::new();
        crate::io::wav_writer::write_chna_chunk(&mut chna_buf)?;
        let mut axml_buf: Vec<u8> = Vec::new();
        crate::io::wav_writer::write_axml_chunk(&mut axml_buf)?;

        // --- RIFF chunk sizes ---
        let fmt_chunk_size: u64 = 40; // WAVE_FORMAT_EXTENSIBLE
        let bext_chunk_size: u64 = 602; // minimum BWF bext (EBU Tech 3285)

        let ds64_chunk_size: u64 = 28;
        let ds64_overhead = if format == AdmContainerFormat::Bw64 {
            8 + ds64_chunk_size
        } else {
            0
        };

        // RIFF size = 4 (WAVE) + 8+fmt + 8+bext + chna_buf + axml_buf + 8+data
        let riff_size: u64 = 4
            + ds64_overhead
            + (8 + fmt_chunk_size)
            + (8 + bext_chunk_size)
            + chna_buf.len() as u64
            + axml_buf.len() as u64
            + (8 + data_chunk_size);

        if format == AdmContainerFormat::Riff32
            && (riff_size > u32::MAX as u64 || data_chunk_size > u32::MAX as u64)
        {
            return Err("data exceeds 4GB RIFF limit — use AdmContainerFormat::Bw64".to_string());
        }

        let mut file =
            std::fs::File::create(path).map_err(|e| format!("ADM BWF create failed: {e}"))?;
        let w = &mut file;

        // --- Header (Duplicated from write_adm_bwf intentionally to avoid touching proven batch logic) ---
        if format == AdmContainerFormat::Bw64 {
            w.write_all(b"BW64").map_err(|e| e.to_string())?;
            w.write_all(&0xFFFFFFFF_u32.to_le_bytes())
                .map_err(|e| e.to_string())?;
        } else {
            w.write_all(b"RIFF").map_err(|e| e.to_string())?;
            w.write_all(&(riff_size as u32).to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        w.write_all(b"WAVE").map_err(|e| e.to_string())?;

        if format == AdmContainerFormat::Bw64 {
            w.write_all(b"ds64").map_err(|e| e.to_string())?;
            w.write_all(&(ds64_chunk_size as u32).to_le_bytes())
                .map_err(|e| e.to_string())?;
            w.write_all(&riff_size.to_le_bytes())
                .map_err(|e| e.to_string())?;
            w.write_all(&data_chunk_size.to_le_bytes())
                .map_err(|e| e.to_string())?;
            w.write_all(&0_u64.to_le_bytes())
                .map_err(|e| e.to_string())?; // dummySampleCount
            w.write_all(&0_u32.to_le_bytes())
                .map_err(|e| e.to_string())?; // tableLength
        }

        // --- fmt chunk (WAVE_FORMAT_EXTENSIBLE, 24-bit LPCM) ---
        w.write_all(b"fmt ").map_err(|e| e.to_string())?;
        w.write_all(&(fmt_chunk_size as u32).to_le_bytes())
            .map_err(|e| e.to_string())?;
        let n_block_align: u16 = 6 * 3; // 6ch * 3 bytes
        let n_avg_bytes: u32 = sample_rate * n_block_align as u32;
        w.write_all(&0xFFFE_u16.to_le_bytes())
            .map_err(|e| e.to_string())?; // wFormatTag = WAVE_FORMAT_EXTENSIBLE
        w.write_all(&6_u16.to_le_bytes())
            .map_err(|e| e.to_string())?; // nChannels
        w.write_all(&sample_rate.to_le_bytes())
            .map_err(|e| e.to_string())?; // nSamplesPerSec
        w.write_all(&n_avg_bytes.to_le_bytes())
            .map_err(|e| e.to_string())?; // nAvgBytesPerSec
        w.write_all(&n_block_align.to_le_bytes())
            .map_err(|e| e.to_string())?; // nBlockAlign
        w.write_all(&24_u16.to_le_bytes())
            .map_err(|e| e.to_string())?; // wBitsPerSample
        w.write_all(&22_u16.to_le_bytes())
            .map_err(|e| e.to_string())?; // cbSize
        w.write_all(&24_u16.to_le_bytes())
            .map_err(|e| e.to_string())?; // wValidBitsPerSample
        w.write_all(&0x3F_u32.to_le_bytes())
            .map_err(|e| e.to_string())?; // dwChannelMask (FL|FR|FC|LFE|BL|BR)
        w.write_all(&[
            0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x80, 0x00, 0x00, 0xAA, 0x00, 0x38,
            0x9B, 0x71,
        ])
        .map_err(|e| e.to_string())?;

        // --- bext chunk (BWF Broadcast Extension, EBU Tech 3285 v2) ---
        w.write_all(b"bext").map_err(|e| e.to_string())?;
        w.write_all(&(bext_chunk_size as u32).to_le_bytes())
            .map_err(|e| e.to_string())?;
        let desc = b"Creator OS \xe2\x80\x94 Apple Spatial Audio";
        let mut desc_padded = [0u8; 256];
        let len = desc.len().min(256);
        desc_padded[..len].copy_from_slice(&desc[..len]);
        w.write_all(&desc_padded).map_err(|e| e.to_string())?;
        let orig = b"Creator OS M0 Daemon";
        let mut orig_padded = [0u8; 32];
        let olen = orig.len().min(32);
        orig_padded[..olen].copy_from_slice(&orig[..olen]);
        w.write_all(&orig_padded).map_err(|e| e.to_string())?;
        w.write_all(&[0u8; 32]).map_err(|e| e.to_string())?; // OriginatorReference
        w.write_all(b"2025-01-01").map_err(|e| e.to_string())?;
        w.write_all(b"00:00:00").map_err(|e| e.to_string())?;
        w.write_all(&0_u64.to_le_bytes())
            .map_err(|e| e.to_string())?; // TimeReference
        w.write_all(&2_u16.to_le_bytes())
            .map_err(|e| e.to_string())?; // Version
        w.write_all(&[0u8; 64]).map_err(|e| e.to_string())?; // UMID
        for _ in 0..5 {
            w.write_all(&0x7FFF_i16.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        w.write_all(&[0u8; 180]).map_err(|e| e.to_string())?;

        // --- chna chunk ---
        w.write_all(&chna_buf).map_err(|e| e.to_string())?;

        // --- axml chunk ---
        w.write_all(&axml_buf).map_err(|e| e.to_string())?;

        // --- data chunk ---
        w.write_all(b"data").map_err(|e| e.to_string())?;
        if format == AdmContainerFormat::Bw64 {
            w.write_all(&0xFFFFFFFF_u32.to_le_bytes())
                .map_err(|e| e.to_string())?;
        } else {
            w.write_all(&(data_chunk_size as u32).to_le_bytes())
                .map_err(|e| e.to_string())?;
        }

        Ok(Self {
            file,
            expected_data_bytes: data_chunk_size,
            written_data_bytes: 0,
        })
    }

    /// Feeds interleaved f32 frames (len must be multiple of 6);
    /// converts to 24-bit LE exactly as write_adm_bwf does (same
    /// clamp, same *8_388_607.0 scale, same 3-byte LE emit) and
    /// appends. Tracks written_data_bytes.
    pub fn write_interleaved_f32(&mut self, samples: &[f32]) -> Result<(), String> {
        use std::io::Write;
        if !samples.len().is_multiple_of(6) {
            return Err("samples length must be a multiple of 6".to_string());
        }

        let mut pcm_24bit: Vec<u8> = Vec::with_capacity(samples.len() * 3);
        for &sample in samples {
            let sample = sample.clamp(-1.0, 1.0);
            let as_i32 = (sample * 8_388_607.0_f32) as i32;
            pcm_24bit.push((as_i32 & 0xFF) as u8);
            pcm_24bit.push(((as_i32 >> 8) & 0xFF) as u8);
            pcm_24bit.push(((as_i32 >> 16) & 0xFF) as u8);
        }

        self.file.write_all(&pcm_24bit).map_err(|e| e.to_string())?;
        self.written_data_bytes += pcm_24bit.len() as u64;
        Ok(())
    }

    /// Verifies written == expected, writes the odd-byte pad if
    /// needed, flushes. Errors on frame-count mismatch.
    pub fn finish(mut self) -> Result<(), String> {
        use std::io::Write;
        if self.written_data_bytes != self.expected_data_bytes {
            return Err(format!(
                "frame count mismatch: expected {} data bytes, wrote {}",
                self.expected_data_bytes, self.written_data_bytes
            ));
        }
        if !self.expected_data_bytes.is_multiple_of(2) {
            self.file.write_all(&[0u8]).map_err(|e| e.to_string())?;
        }
        self.file.flush().map_err(|e| e.to_string())?;
        Ok(())
    }
}

#[cfg(test)]
mod tests_streaming_adm_bwf {
    use super::*;

    #[test]
    fn test_adm_bwf_streaming_oracle() {
        let path_batch = "/tmp/test_adm_bwf_batch.wav";
        let path_stream = "/tmp/test_adm_bwf_stream.wav";
        let sample_rate = 48000;
        let num_frames = 1000;

        let channels: [Vec<f32>; 6] = std::array::from_fn(|ch| {
            (0..num_frames)
                .map(|i| (i as f32 * 0.01 * (ch as f32 + 1.0)).sin() * 0.5)
                .collect()
        });

        // Write batch
        write_adm_bwf(path_batch, &channels, sample_rate, num_frames).unwrap();

        // Write stream
        let mut writer = AdmBwfStreamWriter::create(
            path_stream,
            sample_rate,
            num_frames,
            AdmContainerFormat::Riff32,
        )
        .unwrap();

        let mut interleaved = Vec::new();
        for i in 0..num_frames {
            for ch in 0..6 {
                interleaved.push(channels[ch][i]);
            }
        }

        let chunk1_frames = 256;
        let chunk2_frames = 512;
        let chunk3_frames = 232;
        debug_assert_eq!(chunk1_frames + chunk2_frames + chunk3_frames, num_frames);

        writer
            .write_interleaved_f32(&interleaved[0..(chunk1_frames * 6)])
            .unwrap();
        writer
            .write_interleaved_f32(
                &interleaved[(chunk1_frames * 6)..((chunk1_frames + chunk2_frames) * 6)],
            )
            .unwrap();
        writer
            .write_interleaved_f32(
                &interleaved[((chunk1_frames + chunk2_frames) * 6)
                    ..((chunk1_frames + chunk2_frames + chunk3_frames) * 6)],
            )
            .unwrap();
        writer.finish().unwrap();

        let batch_bytes = std::fs::read(path_batch).unwrap();
        let stream_bytes = std::fs::read(path_stream).unwrap();
        assert_eq!(
            batch_bytes, stream_bytes,
            "streaming writer must exactly match batch writer"
        );

        let _ = std::fs::remove_file(path_batch);
        let _ = std::fs::remove_file(path_stream);
    }

    #[test]
    fn test_adm_bwf_streaming_mismatch_err() {
        let path = "/tmp/test_adm_bwf_mismatch.wav";
        let mut writer =
            AdmBwfStreamWriter::create(path, 48000, 100, AdmContainerFormat::Riff32).unwrap();
        writer.write_interleaved_f32(&vec![0.0; 50 * 6]).unwrap();
        let result = writer.finish();
        assert!(result.is_err(), "finish() must err on frame count mismatch");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_adm_bwf_bw64_structure() {
        let path = "/tmp/test_adm_bwf_bw64.wav";
        let num_frames = 100;
        let sample_rate = 48000;
        let mut writer =
            AdmBwfStreamWriter::create(path, sample_rate, num_frames, AdmContainerFormat::Bw64)
                .unwrap();
        writer
            .write_interleaved_f32(&vec![0.0; num_frames * 6])
            .unwrap();
        writer.finish().unwrap();

        let bytes = std::fs::read(path).unwrap();
        assert_eq!(&bytes[0..4], b"BW64");
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0xFFFFFFFF
        );
        assert_eq!(&bytes[8..12], b"WAVE");
        assert_eq!(&bytes[12..16], b"ds64");

        // Data chunk check
        let mut offset = 12;
        let mut saw_ds64 = false;
        while offset < bytes.len() {
            let chunk_id = &bytes[offset..offset + 4];
            let chunk_size = u32::from_le_bytes(bytes[offset + 4..offset + 8].try_into().unwrap());

            if chunk_id == b"ds64" {
                saw_ds64 = true;
                let _riff_size =
                    u64::from_le_bytes(bytes[offset + 8..offset + 16].try_into().unwrap());
                let data_size =
                    u64::from_le_bytes(bytes[offset + 16..offset + 24].try_into().unwrap());
                assert_eq!(data_size, (num_frames * 6 * 3) as u64);
            } else if chunk_id == b"data" {
                assert_eq!(chunk_size, 0xFFFFFFFF);
                break;
            }
            offset += 8 + chunk_size as usize;
        }
        assert!(saw_ds64, "must see ds64 chunk");

        let _ = std::fs::remove_file(path);
    }
}

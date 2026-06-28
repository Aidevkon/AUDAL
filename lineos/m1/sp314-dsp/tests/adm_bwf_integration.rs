use sp314_dsp::io::wav_writer::write_adm_bwf;

#[test]
fn adm_bwf_riff_header_is_valid() {
    let sample_rate = 48000u32;
    let num_frames = 480usize; // 10ms — enough for header check
    let channels: [Vec<f32>; 6] = std::array::from_fn(|ch| {
        (0..num_frames)
            .map(|i| {
                if ch == 0 {
                    // 440Hz sine on L only
                    (2.0 * std::f32::consts::PI * 440.0 * i as f32 / sample_rate as f32).sin()
                        * 0.5
                } else {
                    0.0
                }
            })
            .collect()
    });

    let path = "/tmp/test_creator_os_spatial.wav";
    write_adm_bwf(path, &channels, sample_rate, num_frames).expect("write_adm_bwf failed");

    let bytes = std::fs::read(path).expect("Could not read output file");

    // RIFF header
    assert_eq!(&bytes[0..4], b"RIFF", "Missing RIFF marker");
    assert_eq!(&bytes[8..12], b"WAVE", "Missing WAVE marker");

    // fmt chunk
    assert_eq!(&bytes[12..16], b"fmt ", "Missing fmt chunk");
    // WAVE_FORMAT_EXTENSIBLE = 0xFFFE (little endian)
    assert_eq!(
        &bytes[20..22],
        &[0xFE, 0xFF],
        "Wrong format tag — expected EXTENSIBLE"
    );
    // 6 channels
    assert_eq!(&bytes[22..24], &[0x06, 0x00], "Wrong channel count");
    // 24-bit
    assert_eq!(
        u16::from_le_bytes(bytes[34..36].try_into().unwrap()),
        24,
        "Wrong bits per sample — expected 24"
    );

    // Channel mask 0x3F at byte offset 0x28 = 40:
    // 12 (RIFF header) + 8 (fmt chunk header)
    //   + 2 (wFormatTag) + 2 (nChannels) + 4 (nSamplesPerSec)
    //   + 4 (nAvgBytesPerSec) + 2 (nBlockAlign) + 2 (wBitsPerSample)
    //   + 2 (cbSize) + 2 (wValidBitsPerSample) = 40
    let mask_offset = 40;
    assert_eq!(
        u32::from_le_bytes(bytes[mask_offset..mask_offset + 4].try_into().unwrap()),
        0x3F,
        "Wrong channel mask — expected 5.1 (0x3F)"
    );

    // bext chunk at offset 60:
    // 12 (RIFF+WAVE) + 8 (fmt header) + 40 (fmt data) = 60
    // Verified via xxd: offset 0x3C = 60
    let bext_offset = 60;
    assert_eq!(
        &bytes[bext_offset..bext_offset + 4],
        b"bext",
        "Missing bext chunk"
    );
    let bext_size =
        u32::from_le_bytes(bytes[bext_offset + 4..bext_offset + 8].try_into().unwrap()) as usize;
    assert_eq!(bext_size, 602, "bext chunk size should be 602 bytes");

    // chna chunk immediately after bext
    let chna_offset = bext_offset + 8 + bext_size;
    assert_eq!(
        &bytes[chna_offset..chna_offset + 4],
        b"chna",
        "Missing chna chunk after bext"
    );
    let chna_size =
        u32::from_le_bytes(bytes[chna_offset + 4..chna_offset + 8].try_into().unwrap()) as usize;
    assert_eq!(chna_size, 244, "chna chunk size should be 244 bytes (4 + 6*40)");

    // axml chunk immediately after chna
    let axml_offset = chna_offset + 8 + chna_size;
    assert_eq!(
        &bytes[axml_offset..axml_offset + 4],
        b"axml",
        "Missing axml chunk after chna"
    );
    let axml_size =
        u32::from_le_bytes(bytes[axml_offset + 4..axml_offset + 8].try_into().unwrap()) as usize;
    assert!(axml_size > 100, "axml chunk should contain ADM XML (got {} bytes)", axml_size);

    // Verify axml contains key ADM identifiers
    let axml_data = &bytes[axml_offset + 8..axml_offset + 8 + axml_size];
    let axml_str = std::str::from_utf8(axml_data).expect("axml should be valid UTF-8");
    assert!(axml_str.contains("AP_00010009"), "axml must reference 5.1 audioPackFormat");
    assert!(axml_str.contains("DirectSpeakers"), "axml must specify DirectSpeakers type");

    // data chunk after axml (accounting for even-byte padding)
    let axml_padded = axml_size + (axml_size % 2);
    let data_offset = axml_offset + 8 + axml_padded;
    assert_eq!(
        &bytes[data_offset..data_offset + 4],
        b"data",
        "Missing data chunk after axml"
    );

    // data size = num_frames * 6 channels * 3 bytes (24-bit)
    let expected_data_size = (num_frames * 6 * 3) as u32;
    let actual_data_size =
        u32::from_le_bytes(bytes[data_offset + 4..data_offset + 8].try_into().unwrap());
    assert_eq!(
        actual_data_size, expected_data_size,
        "Data chunk size mismatch"
    );

    println!("ADM BWF header verified OK (with chna + axml)");
    println!("File size: {} bytes", bytes.len());
    println!("bext size: {} bytes", bext_size);
    println!("chna size: {} bytes", chna_size);
    println!("axml size: {} bytes", axml_size);
    println!("data size: {} bytes", actual_data_size);
}

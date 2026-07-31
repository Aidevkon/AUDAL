# Superflux References

## Provenance
- `bodleasons_mid.wav`: `../../lineos/m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav`
- `clip_speech.wav`: `../../flight_clips/clip_speech.wav`
- `clip_podcast_st.wav`: `../../flight_clips_stereo/clip_podcast_st.wav`

## License
The reference implementation used here (`librosa`) is distributed under the **ISC License**.

**Filterbank Deviation Note**: 24-band MEL (not log) — librosa-native; deviation from the paper's log bands, accepted at review 2026-08-01 because the oracle compares identical filterbanks on both sides. The Rust port MUST read its band mapping from params.json, never assume log spacing.

**CLEAN-ROOM NOTE**: Madmom is strictly forbidden. Madmom carries a CC BY-NC-SA license, which prevents commercial porting. Our implementation must be built clean-room from the Böck & Widmer 2013 paper and the observable outputs of the librosa oracle.

# Fixture Factory Provenance

This directory generates the deterministic golden fixtures for the `sp314-dsp` VAD/NMF pipeline.

## Sources

### Voices (LibriVox)
All voices are sourced from LibriVox and are in the Public Domain.
Original format: 128kbps MP3, 44.1kHz.

| File | Title | Reader / Creator | URL | License | Attribution |
|------|-------|------------------|-----|---------|-------------|
| `voice1.mp3` | Winnie-the-Pooh (Version 6) | Phil Chenevert | [Archive.org](https://archive.org/details/winniethepoohversion6_2605_librivox) | Public Domain | Phil Chenevert, LibriVox |
| `voice2.mp3` | Bible (BBE) 01-02: Genesis & Exodus | Joy Chan | [Archive.org](https://archive.org/details/basicbible01_02_2208_librivox) | Public Domain | Joy Chan, LibriVox |
| `voice3.mp3` | The Boy Scouts Along the Susquehanna | TR Love | [Archive.org](https://archive.org/details/boy_scouts_susquehanna_2203_librivox) | Public Domain | TR Love, LibriVox |

### Music (Free Music Archive / Kevin MacLeod)
Instrumental tracks sourced from Kevin MacLeod, available via Free Music Archive and Incompetech. 
**Instrumental-ness Verification**: Verified via the "instrumental" subject tag in the Archive.org / Free Music Archive metadata and composer's known instrumental catalog.
Original format: 128kbps/VBR MP3, 44.1kHz.

| File | Title | Creator | URL | License | Attribution Line Owed |
|------|-------|---------|-----|---------|-----------------------|
| `music1.mp3` | Cognitive Dissonance - Section 001 | Kevin MacLeod | [Archive.org](https://archive.org/details/CognitiveDissonance-Section001) | CC BY 3.0 | "Cognitive Dissonance - Section 001" by Kevin MacLeod, CC BY 3.0 |
| `music2.mp3` | Disco & Lounge | Kevin MacLeod | [Archive.org](https://archive.org/details/DiscoLounge) | CC0 1.0 | "Disco & Lounge" by Kevin MacLeod, CC0 1.0 |
| `music3.mp3` | garden music | Kevin MacLeod | [Archive.org](https://archive.org/details/garden-music-by-kevin-macleod) | CC BY 4.0 | "garden music" by Kevin MacLeod, CC BY 4.0 |

## Known Limits
Sources are lossy MP3 (128kbps, ~16kHz ceiling). Fit for the factory's purpose (VAD/gate/separation metrics live below 16kHz; the speech templates are 8kHz-capped anyway). NOT fit for full-band ground truth — that is the family-session corpus's job.

## The Hard Row (mix_2)
Measurements reveal that `mix_2` operates as the factory's "hard row" when pushed to high music levels (e.g., in inverted-SNR configurations where the music is strictly louder). `music2.mp3` is "Disco & Lounge" by Kevin MacLeod—a dense, highly percussive, broadband track driven by heavy bass and hi-hats. When pushed toward 0 dBFS clipping, its percussive transients become a continuous wall of broadband noise that masks speech formants aggressively. In contrast, `mix_1` and `mix_3` employ sparser arrangements that leave spectral or temporal gaps for detection even under heavy clipping.

# 6α A/B Listening Test — Scalar vs Spectral Reconstruction

## What this is

Blind A/B comparison of the two stem-reconstruction paths on `bodleasons_mid.wav`:

- **Variant A (scalar):** `apply_mask_to_chunk` — collapses each spectral mask
  to a single broadband scalar per frame, multiplies the time-domain signal.
  No spectral separation survives.
- **Variant B (spectral):** `apply_spectral_mask_to_chunk` — per-bin spectral
  masking on complex STFT frames, iSTFT back to time domain. True frequency
  separation preserved.

Both variants are loudness-matched to -16 LUFS (linear normalization via
ffmpeg loudnorm two-pass).

## Serving

```bash
cd research/ab-player
python3 -m http.server 8080
# Open http://localhost:8080
```

## Controls

| Key     | Action                          |
|---------|---------------------------------|
| Space   | Switch between variant 1 and 2  |
| P       | Play / Pause                    |
| S       | Stop (rewind to start)          |

The player randomly assigns A/B to "Variant 1"/"Variant 2" on each page load.
Click **Reveal Identity** when ready to unblind.

## Re-rendering

If you want to re-render after code changes:

```bash
cd /home/aidevcon/Documents/creator-os
bash research/ab-player/render_variants.sh
```

The script:
1. Renders variant B (spectral, current code) via `cargo test`
2. `git stash` the S1 switch in `two_pass.rs`
3. Renders variant A (scalar, pre-switch code)
4. `git stash pop` to restore
5. Loudness-matches both via ffmpeg loudnorm (two-pass, linear=true)

## Files

| File | Description |
|------|-------------|
| `variant_A_scalar.wav` | Scalar path, normalized -16 LUFS |
| `variant_B_spectral.wav` | Spectral path, normalized -16 LUFS |
| `index.html` | Blind A/B player (no build step, no frameworks) |
| `render_variants.sh` | Render + normalize script |

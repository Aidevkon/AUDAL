# Test Fixtures

## Existing (pre-BS.1770 work)
- `test_5dot1_input.wav`: 5.1 channel test input (contents unspecified)
- `test_stereo_input.wav`: Stereo test input (contents unspecified)

## BS.1770-4 Multichannel LUFS Oracle Fixtures

Generated with ffmpeg 6.1.1-3ubuntu5+esm10. All use `seed=42` for
bit-identical regeneration. Gitignored — regenerate with the
commands below before running `multichannel_lufs_oracle` tests.

### lufs_5dot1_ref_loud.wav
Primary oracle fixture (production target region, -18.0 LUFS).
```
ffmpeg -f lavfi -i "anoisesrc=c=pink:r=48000:a=0.676:seed=42" \
  -ac 6 -channel_layout 5.1 -t 5 -y lufs_5dot1_ref_loud.wav
```
Integrated LUFS (ffmpeg ebur128): **-18.0 LUFS**

### lufs_5dot1_ref.wav
Quiet secondary fixture (-34.6 LUFS).
```
ffmpeg -f lavfi -i "anoisesrc=c=pink:r=48000:a=0.1:seed=42" \
  -ac 6 -channel_layout 5.1 -t 5 -y lufs_5dot1_ref.wav
```
Integrated LUFS (ffmpeg ebur128): **-34.6 LUFS**

### lufs_ls_only.wav
Surround-weight proof: pink noise on Ls only (-33.1 LUFS).
```
ffmpeg -f lavfi -i "anoisesrc=c=pink:r=48000:a=0.1:seed=42" \
  -t 5 -af "pan=5.1|c4=c0" -y lufs_ls_only.wav
```
Integrated LUFS (ffmpeg ebur128): **-33.1 LUFS**

### lufs_l_only.wav
Front-weight baseline: pink noise on L only (-34.6 LUFS).
```
ffmpeg -f lavfi -i "anoisesrc=c=pink:r=48000:a=0.1:seed=42" \
  -t 5 -af "pan=5.1|c0=c0" -y lufs_l_only.wav
```
Integrated LUFS (ffmpeg ebur128): **-34.6 LUFS**

### Surround weight validation
Ls-only (-33.1) vs L-only (-34.6) = **1.5 LU delta**, independently
confirming the BS.1770-4 +1.5 dB surround weight.

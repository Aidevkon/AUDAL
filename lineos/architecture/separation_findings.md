# Separation Findings

ENTRY — The spatial stage cannot produce stereo width.
  StereoRenderer applies the same coefficient to L and R for every
  stem: front_lr_weight to both front channels, rear_lr_weight to
  both rears, and the side/width parameter is not referenced in the
  downmix at all. Every stem therefore lands perfectly centred
  regardless of its assignment. Measured: side/mid ratio 0.5299 with
  separation against 0.5297 without, on speech, and the same equality
  on music and classical.
  So StemChannelAssignments::compute runs, consumes MFCC and spatial
  pre-analysis, produces per-stem placements — and none of it can
  affect the stereo image. The 5.1 dump is the only path where the
  placements have any effect, and it is written before the normalizer
  so its level does not match the stereo render.

ENTRY — On the stereo path, separation is equivalent to a shelf EQ.
  Comparing a normal render against the same file forced through
  skip_stems, aligned for the 216-sample overlap-add delay:
    correlation 0.86 (speech), 0.65 (music), 0.86 (classical)
    spectral difference: a monotonic tilt, +3.66 dB at 63 Hz falling
    to -0.94 dB at 16 kHz, no band-specific behaviour anywhere
  Applying a matching low-shelf to the unseparated render raises the
  correlation to 0.9994, 0.95 and 0.9987. The entire audible
  contribution of NMF, HPSS and the spatial stage on stereo output is
  reproducible with a two-pole filter.
  The likely mechanism: the bass component carries lfe_weight 0.8 and
  is selected as the lowest-centroid component, so low frequencies
  gain energy in the downmix by construction — no correct separation
  required.

  What the separation is genuinely required for, as opposed to
  involved in: the 5.1 render, the MaskingEQ ratios, and the corpus
  features. The stereo output is not on that list.

ENTRY — THE SPATIAL STAGE PRODUCES NO WIDTH, AND ITS OUTPUT IS NOT
  CONSUMED. FiveDotOneStage::render_chunk contains
  "r[i] = l[i]" and "rs[i] = ls[i]" — the front and rear pairs are
  assigned identical values by construction. Measured on a music
  render: correlation L/R = 1.0000, Ls/Rs = 1.0000. Six channels
  carrying three distinct signals. The side/width parameter in
  StemChannelAssignments is computed and never referenced in either
  render path.
  Channel balance is also inverted for its stated purpose: centre
  sits 14 dB below the front pair (-35.73 against -21.34), so
  dialogue does not land where 5.1 expects it.
  The six-channel dump is written before the normalizer, read back,
  level-corrected, and registered in the blob store — then nothing
  requests it. It is not in DspOutput, not in the certificate, and
  the frontend has no way to name the blob. It is produced and
  discarded on every music render.
  The placements themselves react to the separation's mechanics
  rather than to the music: bass_lfe switches on
  bass.spectral_centroid_hz < 80, and the "bass" stem is by
  definition the lowest-centroid spectral slice, so the rule behaves
  as a fixed low shelf.

  Not repaired. Recorded so the cost of the spatial subsystem is
  known: it consumes CPU on every music render and delivers nothing
  that reaches a listener.

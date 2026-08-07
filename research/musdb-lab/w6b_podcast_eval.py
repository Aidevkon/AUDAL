#!/usr/bin/env python3
"""
W6.b — Podcast-like separation evaluation.
Speech-over-bed mixes at 3 SNR levels × 20 tracks → ORACLE --semantic → semantic_eval.
Compares nmf5 vs nmfd8 branches.
"""

import os
import sys
import subprocess
import logging
import numpy as np
import soundfile as sf

# ---------------------------------------------------------------------------
# Paths (repo-relative anchored to this file's directory)
# ---------------------------------------------------------------------------
REPO_ROOT = os.path.abspath(os.path.join(os.path.dirname(__file__), "..", ".."))
MUSDB_LAB = os.path.dirname(os.path.abspath(__file__))

SPEECH_PATH = os.path.join(
    REPO_ROOT,
    "lineos/m1/sp314-dsp/tests/fixtures/duck_splice/speech_segment.flac",
)
EXCERPTS_DIR = os.path.join(MUSDB_LAB, "excerpts")
W5C_CSV = os.path.join(MUSDB_LAB, "results", "w5c_excerpts.csv")
ORACLE_BIN = os.path.join(
    MUSDB_LAB, "oracle_extract", "target", "release", "oracle_extract"
)
SEMEVAL_PY = os.path.join(MUSDB_LAB, "semantic_eval.py")

OUT_DIR = "/tmp/w6b"
RESULTS_CSV = os.path.join(OUT_DIR, "results.csv")
LOG_PATH = os.path.join(OUT_DIR, "run.log")

# SNR conditions
SNRS = [("p10", 10.0), ("p3", 3.0), ("m3", -3.0)]

# ---------------------------------------------------------------------------
# Logger: stdout + file simultaneously
# ---------------------------------------------------------------------------
class Logger:
    def __init__(self, log_path: str):
        os.makedirs(os.path.dirname(log_path), exist_ok=True)
        self._logger = logging.getLogger("w6b")
        self._logger.setLevel(logging.DEBUG)
        if not self._logger.handlers:
            fmt = logging.Formatter("%(asctime)s %(levelname)s %(message)s")
            sh = logging.StreamHandler(sys.stdout)
            sh.setFormatter(fmt)
            fh = logging.FileHandler(log_path, mode="a")
            fh.setFormatter(fmt)
            self._logger.addHandler(sh)
            self._logger.addHandler(fh)

    def info(self, msg: str):
        self._logger.info(msg)

    def error(self, msg: str):
        self._logger.error(msg)


# ---------------------------------------------------------------------------
# Helpers
# ---------------------------------------------------------------------------
def safe_read(path: str):
    """Read audio, enforce sr=48000, return (data_float32, sr)."""
    data, sr = sf.read(path, dtype="float32")
    if sr != 48000:
        raise SystemExit(f"FATAL: {path} sr={sr}")
    return data, sr


def rms_mono(stereo: np.ndarray) -> float:
    """RMS of mono downmix of a (N,2) or (N,) array."""
    if stereo.ndim == 2:
        mono = stereo.mean(axis=1)
    else:
        mono = stereo
    return float(np.sqrt(np.mean(mono ** 2)) + 1e-12)


def write_wav_float(path: str, data: np.ndarray, sr: int = 48000):
    """Write float32 WAV at 48k."""
    os.makedirs(os.path.dirname(path), exist_ok=True)
    sf.write(path, data.astype(np.float32), sr, subtype="FLOAT")


def parse_tracks_from_csv(csv_path: str):
    """Return list of unique track names from w5c_excerpts.csv (column 0)."""
    tracks = []
    seen = set()
    with open(csv_path) as f:
        for i, line in enumerate(f):
            line = line.strip()
            if i == 0 or not line:
                continue  # skip header
            track = line.split(",")[0]
            if track not in seen:
                seen.add(track)
                tracks.append(track)
    return tracks


# ---------------------------------------------------------------------------
# Main
# ---------------------------------------------------------------------------
def main():
    os.makedirs(OUT_DIR, exist_ok=True)
    log = Logger(LOG_PATH)
    log.info("=== W6.b Podcast Eval START ===")

    # -----------------------------------------------------------------------
    # 1. Parse track list
    # -----------------------------------------------------------------------
    tracks = parse_tracks_from_csv(W5C_CSV)
    log.info(f"Tracks ({len(tracks)}): {tracks}")

    # -----------------------------------------------------------------------
    # 2. Prepare results CSV  (remove if exists, write header)
    # -----------------------------------------------------------------------
    if os.path.exists(RESULTS_CSV):
        os.remove(RESULTS_CSV)
    with open(RESULTS_CSV, "w") as f:
        # Same columns as semantic_eval.py appends:
        # track_name, branch_name, stem, sdr, sir, sar
        f.write("track,branch,role,sdr,sir,sar\n")
    log.info(f"Results CSV initialised: {RESULTS_CSV}")

    # -----------------------------------------------------------------------
    # 3. Load speech once (stereo)
    # -----------------------------------------------------------------------
    log.info(f"Loading speech: {SPEECH_PATH}")
    speech, sr_sp = safe_read(SPEECH_PATH)  # (N, 2) float32, sr=48000
    log.info(f"Speech shape={speech.shape}, sr={sr_sp}")

    # -----------------------------------------------------------------------
    # 4. Main loop
    # -----------------------------------------------------------------------
    for track in tracks:
        # Load stems for this track from EXCERPTS_DIR
        track_dir = os.path.join(EXCERPTS_DIR, track)

        drums, _ = safe_read(os.path.join(track_dir, "drums.wav"))
        bass, _  = safe_read(os.path.join(track_dir, "bass.wav"))
        other, _ = safe_read(os.path.join(track_dir, "other.wav"))

        # Ensure stereo (N,2)
        def ensure_stereo(x):
            if x.ndim == 1:
                x = np.stack([x, x], axis=1)
            return x

        drums = ensure_stereo(drums)
        bass  = ensure_stereo(bass)
        other = ensure_stereo(other)

        # Bed = drums + bass + other, sample-aligned (min-len truncate)
        min_bed = min(drums.shape[0], bass.shape[0], other.shape[0])
        bed = drums[:min_bed] + bass[:min_bed] + other[:min_bed]  # (min_bed, 2)

        # Truncate bed AND speech to shared minimum
        min_len = min(min_bed, speech.shape[0])
        bed_t    = bed[:min_len]       # (min_len, 2)
        speech_t = speech[:min_len]    # (min_len, 2)
        drums_t  = drums[:min_len]
        bass_t   = bass[:min_len]
        other_t  = other[:min_len]

        for (tag, snr_db) in SNRS:
            log.info(f"Processing track={track} snr={tag}")

            # Compute gain g so RMS_mono(voice_gt) / RMS_mono(bed) = 10^(snr/20)
            speech_rms = rms_mono(speech_t)
            bed_rms    = rms_mono(bed_t)
            g = (bed_rms / speech_rms) * 10 ** (snr_db / 20.0)

            voice_gt = speech_t * g          # (min_len, 2) float32
            mix      = voice_gt + bed_t      # (min_len, 2) float32

            # Output layout:
            #   /tmp/w6b/<track>__snr<tag>/
            #     mix.wav
            #     <track>__snr<tag>/          ← gt_dir; basename = run_name
            #       vocals.wav, drums.wav, bass.wav, other.wav
            #     sep/
            #       nmf5/   vocals.wav drums.wav bass.wav other.wav
            #       nmfd8/  vocals.wav drums.wav bass.wav other.wav
            run_name = f"{track}__snr{tag}"
            run_dir  = os.path.join(OUT_DIR, run_name)
            # gt_dir basename must equal run_name so semantic_eval.py
            # records track = run_name in the CSV.
            gt_dir   = os.path.join(run_dir, run_name)
            sep_dir  = os.path.join(run_dir, "sep")

            # Write GT stems
            write_wav_float(os.path.join(gt_dir, "vocals.wav"), voice_gt)
            write_wav_float(os.path.join(gt_dir, "drums.wav"),  drums_t)
            write_wav_float(os.path.join(gt_dir, "bass.wav"),   bass_t)
            write_wav_float(os.path.join(gt_dir, "other.wav"),  other_t)

            # Write mix
            mix_path = os.path.join(run_dir, "mix.wav")
            write_wav_float(mix_path, mix)

            log.info(f"  mix.wav written ({mix.shape[0]} samples), g={g:.4f}")

            # ------------------------------------------------------------------
            # Run ORACLE --semantic
            # ------------------------------------------------------------------
            os.makedirs(sep_dir, exist_ok=True)
            oracle_cmd = [ORACLE_BIN, "--semantic", mix_path, sep_dir]
            log.info(f"  ORACLE: {' '.join(oracle_cmd)}")
            result = subprocess.run(oracle_cmd, capture_output=True, text=True)
            if result.returncode != 0:
                log.error(
                    f"  ORACLE FAILED (rc={result.returncode}):\n"
                    f"  STDOUT: {result.stdout}\n"
                    f"  STDERR: {result.stderr}"
                )
                continue
            log.info("  ORACLE done (rc=0)")

            # ------------------------------------------------------------------
            # semantic_eval.py × 2
            # track_name = os.path.basename(gt_dir.rstrip('/')) = run_name  ✓
            # branch_name = os.path.basename(branch_dir.rstrip('/')) = "nmf5" / "nmfd8"
            # → 4 stems × 2 branches = 8 CSV rows per (track, snr)
            #   i.e. 6 rows per track across SNRs ← per the task: 6 distinct CSV
            #   lines per track per snr = 4 stems per branch, but we call 2 branches,
            #   so 8 rows per (track,snr), 24 rows per track total.
            # ------------------------------------------------------------------
            for branch in ("nmf5", "nmfd8"):
                branch_dir = os.path.join(sep_dir, branch)
                semeval_cmd = [
                    sys.executable,
                    SEMEVAL_PY,
                    gt_dir,       # basename = run_name  → CSV track field
                    branch_dir,   # basename = branch    → CSV branch field
                    RESULTS_CSV,  # append mode in semeval
                ]
                log.info(f"  semeval [{branch}]: {' '.join(semeval_cmd)}")
                r2 = subprocess.run(semeval_cmd, capture_output=True, text=True)
                if r2.returncode != 0:
                    log.error(
                        f"  semeval FAILED (rc={r2.returncode}):\n"
                        f"  STDOUT: {r2.stdout}\n"
                        f"  STDERR: {r2.stderr}"
                    )
                else:
                    log.info(f"  semeval [{branch}] done")
                    if r2.stdout.strip():
                        log.info(f"  semeval stdout:\n{r2.stdout.strip()}")

            print(f"[DONE] track={track} snr={tag}")

    log.info("=== W6.b Podcast Eval END ===")
    log.info(f"Results at: {RESULTS_CSV}")


if __name__ == "__main__":
    main()

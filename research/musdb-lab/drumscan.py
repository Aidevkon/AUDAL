#!/usr/bin/env python3
"""drumscan.py — ΤΟ ΜΗΧΑΝΗΜΑ ΠΡΟΤΕΙΝΕΙ, ΤΟ ΑΥΤΙ ΑΠΟΦΑΣΙΖΕΙ.

Κατατάσσει τα tracks του fma_small_cc_allowlist.json κατά πιθανότητα
percussion-heavy/isolated-drums περιεχομένου, για το w_music_v1
retrain (χρειάζεται ~2-5h τέτοιο υλικό). Η έξοδος είναι λίστα ΓΙΑ
ΑΚΡΟΑΣΗ, όχι training set.

ΣΥΜΒΑΣΗ (F-065 πνεύμα): το score ΠΡΟΤΕΙΝΕΙ υποψηφίους για ακρόαση —
ΔΕΝ επιλέγει training set. Η επιλογή είναι του αυτιού. ΚΑΙ: το
κριτήριο είναι ΑΚΟΥΣΤΙΚΟ (κρουστά), ΟΧΙ ομοιότητα με MUSDB — καμία
MUSDB αναφορά εδώ (clean-room input rule).

VENV / ΒΙΒΛΙΟΘΗΚΕΣ (δηλωμένο): research/musdb-lab/venv —
librosa==0.11.0, soundfile==0.14.0, numpy==2.4.6, scipy==1.18.0.
Δεν βρέθηκε bespoke transient/onset εργαλείο σε python στο repo
(ελέγχθηκαν research/musdb-lab/role_features.py και
research/superflux/p3_analysis.py — και τα δύο καλούν απευθείας
librosa.onset.onset_strength χωρίς δικό τους αλγόριθμο)· άρα
χρησιμοποιείται librosa.onset.onset_detect, ο fallback που ανέφερε
η εντολή.

ΠΑΡΕΚΚΛΙΣΗ ΑΠΟ ΤΟ ΑΙΤΗΜΑ (δηλωμένη, όχι σιωπηλή): ζητήθηκε "60s από
τη μέση". ΜΕΤΡΗΘΗΚΕ πριν γραφτεί αυτό το script (δείγμα 15 τυχαίων
tracks): το fma_small_cc_allowlist.json δείχνει σε 30s previews
(29.98-30.0s), ΟΧΙ πλήρη tracks — το FMA-small format είναι έτσι.
Άρα: WINDOW_SECS=60.0 παραμένει η ΟΝΟΜΑΣΤΙΚΗ παράμετρος, αλλά η
load_window() παίρνει ΟΛΟΚΛΗΡΟ το clip όταν η διάρκειά του είναι
<= WINDOW_SECS (πρακτικά όλα τα 1329) — αλλιώς 60s παράθυρο
κεντραρισμένο στη μέση, για μελλοντικά αρχεία που ΔΕΝ είναι 30s
previews.

ΦΟΡΜΟΥΛΑ SCORE (σύμβαση αυτού του script):
  score = z(onset_rate) + z(percussive_ratio) + z(spectral_flatness)
  όπου z(x) = (x - mean(x)) / std(x), υπολογισμένο ΣΤΟ ΤΕΛΟΣ πάνω σε
  όλα τα επιτυχώς σαρωμένα tracks (population std, ddof=0) — δύο
  περάσματα: πρώτα raw features για όλα, μετά z-normalize + rank.
  Και τα τρία signals μπαίνουν με ΙΔΙΟ (θετικό) πρόσημο — η σύμβαση
  θεωρεί υψηλό spectral flatness ένδειξη broadband/noisy ενέργειας
  (hi-hats, cymbals, snare) παρά "θόρυβο" με αρνητική έννοια.
  ΑΝΟΙΧΤΟ, δηλωμένο εδώ ρητά: pure kick/bass-heavy υλικό έχει
  ΧΑΜΗΛΟ flatness και θα υποτιμηθεί από αυτή τη σύμβαση — το αυτί
  το διορθώνει στην ακρόαση, γι' αυτό υπάρχει το audition βήμα.

ΤΑ 3 FEATURES:
  onset_rate       = len(librosa.onset.onset_detect(y, sr, units='time')) / duration_loaded_secs
  percussive_ratio = HPSS energy ratio: sum(percussive**2) / (sum(harmonic**2) + sum(percussive**2))
                      μέσω librosa.effects.hpss(y)
  spectral_flatness = mean(librosa.feature.spectral_flatness(y=y))

ΣΦΑΛΜΑΤΑ ΦΟΡΤΩΣΗΣ: skip + log, ΔΕΝ σταματάει το scan (νόμιμο εδώ —
είναι scan, όχι gate).

ΤΡΕΞΙΜΟ (tee + sha256, όχι εσωτερικό log file — το script τυπώνει
μόνο σε stdout):
  mkdir -p /tmp/drumscan
  research/musdb-lab/venv/bin/python3 research/musdb-lab/drumscan.py \
      2>&1 | tee /tmp/drumscan/run.log
  sha256sum /tmp/drumscan/run.log
"""

import csv
import json
import sys
import time
from pathlib import Path

import numpy as np
import soundfile as sf
import librosa

ALLOWLIST = Path("/home/aidevcon/Downloads/DATASET/fma/fma_small_cc_allowlist.json")
OUT_DIR = Path("/tmp/drumscan")
WINDOW_SECS = 60.0
PROGRESS_EVERY = 100


def load_window(path, window_secs=WINDOW_SECS):
    """Φόρτωσε window_secs από τη μέση — ή ΟΛΟΚΛΗΡΟ το clip αν
    είναι κοντύτερο (βλ. ΠΑΡΕΚΚΛΙΣΗ στο module docstring)."""
    info = sf.info(path)
    sr = info.samplerate
    duration = info.frames / sr if sr else 0.0
    if duration <= window_secs:
        start_frame, n_frames = 0, info.frames
    else:
        center = duration / 2.0
        start = max(0.0, center - window_secs / 2.0)
        start_frame = int(start * sr)
        n_frames = int(window_secs * sr)
    y, sr = sf.read(
        path, start=start_frame, frames=n_frames, dtype="float32", always_2d=False
    )
    if y.ndim > 1:
        y = y.mean(axis=1)
    return y, sr


def extract_features(path):
    y, sr = load_window(path)
    if y.size == 0:
        raise ValueError("empty audio after load")
    duration_loaded = len(y) / sr

    onsets = librosa.onset.onset_detect(y=y, sr=sr, units="time")
    onset_rate = len(onsets) / duration_loaded if duration_loaded > 0 else 0.0

    harmonic, percussive = librosa.effects.hpss(y)
    h_energy = float(np.sum(harmonic.astype(np.float64) ** 2))
    p_energy = float(np.sum(percussive.astype(np.float64) ** 2))
    total = h_energy + p_energy
    perc_ratio = p_energy / total if total > 0 else 0.0

    flatness = float(np.mean(librosa.feature.spectral_flatness(y=y)))

    return onset_rate, perc_ratio, flatness


def zscore(arr):
    std = arr.std()
    if std == 0:
        return np.zeros_like(arr)
    return (arr - arr.mean()) / std


def main():
    OUT_DIR.mkdir(parents=True, exist_ok=True)

    tracks = json.loads(ALLOWLIST.read_text())
    print(f"[drumscan] loaded {len(tracks)} tracks from {ALLOWLIST}")
    print(
        f"[drumscan] window={WINDOW_SECS}s nominal (whole clip if shorter — see docstring)"
    )
    print(f"[drumscan] librosa={librosa.__version__} soundfile={sf.__version__}")

    results = []
    errors = []
    t0 = time.time()
    for i, entry in enumerate(tracks, 1):
        path = entry.get("path", "")
        track_id = entry.get("track_id", "")
        try:
            onset_rate, perc_ratio, flatness = extract_features(path)
            results.append(
                {
                    "track_id": track_id,
                    "path": path,
                    "onset_rate": onset_rate,
                    "perc_ratio": perc_ratio,
                    "flatness": flatness,
                }
            )
        except Exception as e:
            errors.append({"track_id": track_id, "path": path, "error": str(e)})
            print(f"[SKIP] track_id={track_id} path={path} error={e}")

        if i % PROGRESS_EVERY == 0 or i == len(tracks):
            elapsed = time.time() - t0
            print(
                f"[progress] {i}/{len(tracks)} scanned={len(results)} "
                f"skipped={len(errors)} elapsed={elapsed:.1f}s"
            )

    if not results:
        print("[drumscan] FATAL: zero tracks scanned successfully")
        sys.exit(1)

    onset_arr = np.array([r["onset_rate"] for r in results])
    perc_arr = np.array([r["perc_ratio"] for r in results])
    flat_arr = np.array([r["flatness"] for r in results])

    scores = zscore(onset_arr) + zscore(perc_arr) + zscore(flat_arr)
    for r, s in zip(results, scores):
        r["score"] = float(s)

    results.sort(key=lambda r: r["score"], reverse=True)

    ranked_path = OUT_DIR / "ranked.tsv"
    with open(ranked_path, "w", newline="") as f:
        w = csv.writer(f, delimiter="\t")
        w.writerow(
            ["rank", "score", "onset_rate", "perc_ratio", "flatness", "path", "track_id"]
        )
        for rank, r in enumerate(results, 1):
            w.writerow(
                [
                    rank,
                    f"{r['score']:.4f}",
                    f"{r['onset_rate']:.4f}",
                    f"{r['perc_ratio']:.4f}",
                    f"{r['flatness']:.4f}",
                    r["path"],
                    r["track_id"],
                ]
            )

    top40 = results[:40]
    md_path = OUT_DIR / "audition_top40.md"
    with open(md_path, "w") as f:
        f.write("# drumscan — TOP-40 audition list\n\n")
        f.write(
            "Το μηχάνημα ΠΡΟΤΕΙΝΕΙ. Το αυτί ΑΠΟΦΑΣΙΖΕΙ (F-065 πνεύμα). "
            "Κριτήριο ΑΚΟΥΣΤΙΚΟ (κρουστά) — καμία σύγκριση/αναφορά MUSDB.\n\n"
        )
        f.write(
            "Το allowlist json δεν έχει πεδία τίτλου/artist "
            "(μόνο track_id/license/path) — δεν υπάρχουν εδώ credits, "
            "μόνο track_id για μετέπειτα αναζήτηση.\n\n"
        )
        for rank, r in enumerate(top40, 1):
            f.write(f"## {rank}. score={r['score']:.3f} (track_id={r['track_id']})\n")
            f.write(f"- path: `{r['path']}`\n")
            f.write(
                f"- onset_rate={r['onset_rate']:.3f}/s "
                f"perc_ratio={r['perc_ratio']:.3f} flatness={r['flatness']:.4f}\n"
            )
            f.write(f'- play: `ffplay "{r["path"]}"`\n\n')

    print(f"[drumscan] wrote {ranked_path} ({len(results)} rows)")
    print(f"[drumscan] wrote {md_path} (top {len(top40)})")
    print(
        f"[drumscan] SUMMARY scanned={len(results)} skipped={len(errors)} "
        f"total={len(tracks)}"
    )
    print(
        "[drumscan] score distribution: "
        f"min={scores.min():.3f} p25={np.percentile(scores, 25):.3f} "
        f"median={np.median(scores):.3f} p75={np.percentile(scores, 75):.3f} "
        f"max={scores.max():.3f}"
    )


if __name__ == "__main__":
    main()

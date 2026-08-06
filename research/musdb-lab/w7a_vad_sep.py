import os
import sys
import json
import subprocess
import glob
import soundfile as sf
import numpy as np

# Redirect stdout and stderr to log file
os.makedirs("/tmp/w7a", exist_ok=True)
log_file = open("/tmp/w7a/run.log", "w")
class Logger:
    def __init__(self, stream):
        self.stream = stream
    def write(self, msg):
        self.stream.write(msg)
        log_file.write(msg)
        self.stream.flush()
        log_file.flush()
    def flush(self):
        self.stream.flush()
        log_file.flush()

sys.stdout = Logger(sys.stdout)
sys.stderr = Logger(sys.stderr)

MVAD_BIN = os.path.abspath("research/musdb-lab/dbus_eval/target/release/mvad_w7a")
ORACLE_BIN = os.path.abspath("research/musdb-lab/oracle_extract/target/release/oracle_extract")
MANIFEST_PATH = "lineos/m1/sp314-dsp/tests/fixtures/duck_splice/manifest.json"
EXCERPTS_DIR = "research/musdb-lab/excerpts"
BEDS_DIR = "/tmp/w7a/beds"
SEP_DIR = "/tmp/w7a/sep"
FIXTURES_DIR = "/tmp/w7a/fixtures"

for d in [BEDS_DIR, SEP_DIR, FIXTURES_DIR]:
    os.makedirs(d, exist_ok=True)

CSV_RESULTS = "/tmp/w7a/results.csv"
CSV_FIXTURES = "/tmp/w7a/fixtures.csv"

with open(CSV_RESULTS, "w") as f:
    f.write("track,stem,hist_vad_pct,raw_ratio,raw_pmean,sep_ratio,sep_pmean\n")

with open(CSV_FIXTURES, "w") as f:
    f.write("fixture,variant,ratio_in,ratio_out\n")

# Load manifest and find the 30 entries
def find_entries(d, entries, fixtures):
    if isinstance(d, dict):
        if "vad_pct" in d and "track" in d and "stem" in d:
            entries.append(d)
        elif "vad_pct" in d and "bed_file" in d:
            # Maybe fixture bed? We actually need the final duck splice fixtures, wait.
            pass
        for k, v in d.items():
            find_entries(v, entries, fixtures)
    elif isinstance(d, list):
        for item in d:
            find_entries(item, entries, fixtures)

with open(MANIFEST_PATH, "r") as f:
    manifest = json.load(f)

entries = []
find_entries(manifest, entries, [])

print(f"Loaded {len(entries)} track candidate entries from manifest.")
missing_tracks = 0
for entry in entries:
    track = entry["track"]
    if not os.path.isdir(os.path.join(EXCERPTS_DIR, track)):
        missing_tracks += 1
        print(f"MISSING: {track}")
    else:
        print(f"FOUND: {track} (stem: {entry['stem']}, vad_pct: {entry['vad_pct']})")

print(f"Total missing tracks: {missing_tracks}")

def run_mvad(wav_path, window=None):
    cmd = [MVAD_BIN, wav_path]
    if window:
        cmd.extend([str(window[0]), str(window[1])])
    res = subprocess.run(cmd, capture_output=True, text=True, check=True)
    out_line = ""
    for line in res.stdout.splitlines():
        if line.startswith("W7A|"):
            out_line = line
            break
    if not out_line:
        raise ValueError(f"mvad_w7a failed to output W7A line for {wav_path}")
    
    # parse W7A|file=<path>|ratio_all=<x>|p_mean_all=<x>|ratio_in=<x>|ratio_out=<x>|nf=<x>
    parts = out_line.split("|")
    data = {}
    for p in parts[1:]:
        k, v = p.split("=")
        data[k] = v
    return data

for entry in entries:
    track = entry["track"]
    stems = entry["stem"].split("+")
    hist_vad = entry["vad_pct"]
    
    track_dir = os.path.join(EXCERPTS_DIR, track)
    if not os.path.isdir(track_dir):
        print(f"Skipping missing track {track}")
        continue
    
    # 1. Build bed
    bed_audio = None
    sr = 48000
    for s in stems:
        p = os.path.join(track_dir, f"{s}.wav")
        audio, s_rate = sf.read(p)
        if s_rate != 48000:
            raise SystemExit(f"FATAL: {p} sr={s_rate}")
        if bed_audio is None:
            bed_audio = audio
        else:
            mlen = min(len(bed_audio), len(audio))
            bed_audio[:mlen] += audio[:mlen]
            bed_audio = bed_audio[:mlen]
    
    bed_path = os.path.join(BEDS_DIR, f"{track}__{entry['stem']}.wav")
    sf.write(bed_path, bed_audio, sr, subtype='FLOAT')
    
    # 2. mvad_w7a raw
    raw_res = run_mvad(bed_path)
    
    # 3. oracle_extract
    sep_out_dir = os.path.join(SEP_DIR, f"{track}__{entry['stem']}")
    os.makedirs(sep_out_dir, exist_ok=True)
    subprocess.run([ORACLE_BIN, "--semantic", bed_path, sep_out_dir], check=True)
    
    # 4. mvad_w7a sep vocals
    sep_vocals = os.path.join(sep_out_dir, "nmfd8", "vocals.wav")
    if not os.path.exists(sep_vocals):
        # Fallback if oracle_extract outputs to nmfd8/ or something else
        # The rust code outputs to output_dir/nmf5 and output_dir/nmfd8
        pass
    sep_res = run_mvad(sep_vocals)
    
    # 5. CSV append
    with open(CSV_RESULTS, "a") as f:
        f.write(f"{track},{entry['stem']},{hist_vad},{raw_res['ratio_all']},{raw_res['p_mean_all']},{sep_res['ratio_all']},{sep_res['p_mean_all']}\n")


# Fixtures
FIXTURES = [
    ("duck_splice_synth_snr-15", "lineos/m1/sp314-dsp/tests/fixtures/duck_splice/duck_splice_synth_snr-15.flac"),
    ("duck_splice_real_snr-15", "lineos/m1/sp314-dsp/tests/fixtures/duck_splice/duck_splice_real_snr-15.flac")
]

for name, flac_path in FIXTURES:
    if not os.path.exists(flac_path):
        print(f"Fixture not found: {flac_path}")
        continue
    
    wav_path = os.path.join(FIXTURES_DIR, f"{name}.wav")
    audio, sr = sf.read(flac_path)
    if sr != 48000:
        raise SystemExit(f"FATAL: {flac_path} sr={sr}")
    sf.write(wav_path, audio, sr, subtype='FLOAT')
    
    # Raw mvad
    raw_res = run_mvad(wav_path, window=(10, 20))
    
    # Sep
    sep_out = os.path.join(SEP_DIR, name)
    os.makedirs(sep_out, exist_ok=True)
    subprocess.run([ORACLE_BIN, "--semantic", wav_path, sep_out], check=True)
    
    sep_vocals = os.path.join(sep_out, "nmfd8", "vocals.wav")
    sep_res = run_mvad(sep_vocals, window=(10, 20))
    
    with open(CSV_FIXTURES, "a") as f:
        f.write(f"{name},raw,{raw_res['ratio_in']},{raw_res['ratio_out']}\n")
        f.write(f"{name},sep,{sep_res['ratio_in']},{sep_res['ratio_out']}\n")

print("Pipeline finished.")

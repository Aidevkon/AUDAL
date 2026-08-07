#!/usr/bin/env python3
import os
import sys
import subprocess
import glob
import logging

OUT_DIR = "/tmp/w7b"
LOG_PATH = os.path.join(OUT_DIR, "run.log")
RESULTS_CSV = os.path.join(OUT_DIR, "results.csv")
FIXTURES_CSV = os.path.join(OUT_DIR, "fixtures.csv")
MVAD_BIN = os.path.join(os.path.dirname(__file__), "dbus_eval/target/release/mvad_w7b")

def setup_logger():
    os.makedirs(OUT_DIR, exist_ok=True)
    logger = logging.getLogger("w7b")
    logger.setLevel(logging.INFO)
    fmt = logging.Formatter("%(asctime)s %(levelname)s %(message)s")
    sh = logging.StreamHandler(sys.stdout)
    sh.setFormatter(fmt)
    fh = logging.FileHandler(LOG_PATH, mode="a")
    fh.setFormatter(fmt)
    logger.addHandler(sh)
    logger.addHandler(fh)
    return logger

def main():
    log = setup_logger()
    if not os.path.exists(MVAD_BIN):
        log.error(f"FATAL: binary {MVAD_BIN} missing")
        sys.exit(1)
        
    beds = glob.glob("/tmp/w7a/beds/*.wav")
    
    with open(RESULTS_CSV, "w") as f:
        f.write("bed,routed_ratio,routed_pmean,nf_mix\n")
        
    for bed_path in sorted(beds):
        basename = os.path.basename(bed_path)
        name_no_ext = os.path.splitext(basename)[0]
        stem_path = f"/tmp/w7a/sep/{name_no_ext}/nmfd8/vocals.wav"
        
        if not os.path.exists(stem_path):
            log.warning(f"Skip: missing stem for {name_no_ext}")
            continue
            
        cmd = [MVAD_BIN, stem_path, bed_path]
        res = subprocess.run(cmd, capture_output=True, text=True)
        if res.returncode != 0:
            log.error(f"Failed {name_no_ext}: {res.stderr}")
            continue
            
        out = res.stdout.strip()
        if not out.startswith("W7B|"):
            continue
            
        # parse W7B output
        # W7B|stem=...|mix=...|ratio_all=...|p_mean_all=...|ratio_in=...|ratio_out=...|nf_mix=...
        parts = out.split("|")
        d = {}
        for p in parts[1:]:
            k, v = p.split("=")
            d[k] = v
            
        with open(RESULTS_CSV, "a") as f:
            f.write(f"{name_no_ext},{d['ratio_all']},{d['p_mean_all']},{d['nf_mix']}\n")
            
    with open(FIXTURES_CSV, "w") as f:
        f.write("fixture,ratio_in,ratio_out\n")
        
    fixtures = ["duck_splice_synth_snr-15", "duck_splice_real_snr-15"]
    for fix in fixtures:
        stem_path = f"/tmp/w7a/sep/{fix}/nmfd8/vocals.wav"
        mix_path = f"/tmp/w7a/fixtures/{fix}.wav"
        if not os.path.exists(stem_path) or not os.path.exists(mix_path):
            log.error(f"FATAL: missing files for fixture {fix}")
            sys.exit(1)
            
        cmd = [MVAD_BIN, stem_path, mix_path, "10", "20"]
        res = subprocess.run(cmd, capture_output=True, text=True)
        if res.returncode != 0:
            log.error(f"Failed {fix}: {res.stderr}")
            continue
            
        out = res.stdout.strip()
        if not out.startswith("W7B|"):
            continue
            
        parts = out.split("|")
        d = {}
        for p in parts[1:]:
            k, v = p.split("=")
            d[k] = v
            
        with open(FIXTURES_CSV, "a") as f:
            f.write(f"{fix},{d['ratio_in']},{d['ratio_out']}\n")
            
    log.info("Done.")

if __name__ == "__main__":
    main()

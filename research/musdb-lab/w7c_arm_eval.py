#!/usr/bin/env python3
import os
import sys
import subprocess
import glob
import logging

OUT_DIR = "/tmp/w7c"
LOG_PATH = os.path.join(OUT_DIR, "run.log")
BEDS_CSV = os.path.join(OUT_DIR, "beds.csv")
POS_CSV = os.path.join(OUT_DIR, "pos.csv")
FIXTURES_CSV = os.path.join(OUT_DIR, "fixtures.csv")
MVAD_BIN = os.path.join(os.path.dirname(__file__), "dbus_eval/target/release/mvad_w7c")

def setup_logger():
    os.makedirs(OUT_DIR, exist_ok=True)
    logger = logging.getLogger("w7c")
    logger.setLevel(logging.INFO)
    fmt = logging.Formatter("%(asctime)s %(levelname)s %(message)s")
    sh = logging.StreamHandler(sys.stdout)
    sh.setFormatter(fmt)
    fh = logging.FileHandler(LOG_PATH, mode="a")
    fh.setFormatter(fmt)
    logger.addHandler(sh)
    logger.addHandler(fh)
    return logger

def run_mvad(cmd):
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        return None, res.stderr
    out = res.stdout.strip()
    for line in out.splitlines():
        if line.startswith("W7C|"):
            parts = line.split("|")
            d = {}
            for p in parts[1:]:
                if "=" in p:
                    k, v = p.split("=", 1)
                    d[k] = v
            return d, None
    return None, "No W7C output found"

def main():
    log = setup_logger()
    if not os.path.exists(MVAD_BIN):
        log.error(f"FATAL: binary {MVAD_BIN} missing")
        sys.exit(1)
        
    # PART 1: NEGATIVE SET (beds)
    beds = glob.glob("/tmp/w7a/beds/*.wav")
    with open(BEDS_CSV, "w") as f:
        f.write("bed,segments,armed_seg,armed_frame_ratio,micro_ratio,effective_ratio,nf,armed_g,eff_g\n")
        
    for bed_path in sorted(beds):
        basename = os.path.basename(bed_path)
        name_no_ext = os.path.splitext(basename)[0]
        cmd = [MVAD_BIN, bed_path]
        d, err = run_mvad(cmd)
        if err:
            log.error(f"Failed bed {name_no_ext}: {err}")
            continue
        with open(BEDS_CSV, "a") as f:
            f.write(f"{name_no_ext},{d['segments']},{d['armed_seg']},{d['armed_frame_ratio']},{d['micro_ratio']},{d['effective_ratio']},{d['nf']},{d['armed_g_ratio']},{d['effective_g_ratio']}\n")
            
    # PART 2: POSITIVE SET (w6b mixes)
    w6b_dirs = glob.glob("/tmp/w6b/*/")
    if not w6b_dirs:
        log.warning("No w6b directories found, skipping positive set")
    else:
        with open(POS_CSV, "w") as f:
            f.write("mix_dir,snr_tag,armed_frame_ratio,micro_ratio,effective_ratio,armed_g,eff_g\n")
            
        for mix_dir in sorted(w6b_dirs):
            mix_path = os.path.join(mix_dir, "mix.wav")
            if not os.path.exists(mix_path):
                continue
            dirname = os.path.basename(os.path.normpath(mix_dir))
            snr_tag = "unknown"
            if "__snr" in dirname:
                snr_tag = dirname.split("__snr")[-1]
                
            cmd = [MVAD_BIN, mix_path]
            d, err = run_mvad(cmd)
            if err:
                log.error(f"Failed pos {dirname}: {err}")
                continue
            with open(POS_CSV, "a") as f:
                f.write(f"{dirname},{snr_tag},{d['armed_frame_ratio']},{d['micro_ratio']},{d['effective_ratio']},{d['armed_g_ratio']},{d['effective_g_ratio']}\n")

    # PART 3: FIXTURES
    with open(FIXTURES_CSV, "w") as f:
        f.write("fixture,segments,armed_seg,armed_frame_ratio,ratio_in,ratio_out,armed_g,in_g,out_g\n")
        
    fixtures = ["duck_splice_synth_snr-15", "duck_splice_real_snr-15"]
    for fix in fixtures:
        mix_path = f"/tmp/w7a/fixtures/{fix}.wav"
        if not os.path.exists(mix_path):
            log.error(f"FATAL: missing fixture {fix}")
            sys.exit(1)
            
        cmd = [MVAD_BIN, mix_path, "10", "20"]
        d, err = run_mvad(cmd)
        if err:
            log.error(f"Failed fixture {fix}: {err}")
            continue
            
        with open(FIXTURES_CSV, "a") as f:
            f.write(f"{fix},{d['segments']},{d['armed_seg']},{d['armed_frame_ratio']},{d['ratio_in']},{d['ratio_out']},{d['armed_g_ratio']},{d['in_g']},{d['out_g']}\n")
            
    log.info("Done.")

if __name__ == "__main__":
    main()

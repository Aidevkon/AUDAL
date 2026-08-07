import os
import glob
import subprocess
import sys
import time
import pandas as pd
import logging

os.makedirs("/tmp/phi1", exist_ok=True)
logging.basicConfig(
    filename='/tmp/phi1/rust_jury.log',
    level=logging.INFO,
    format='%(message)s'
)
console = logging.StreamHandler()
console.setLevel(logging.INFO)
logging.getLogger('').addHandler(console)

BIN_PATH = "research/musdb-lab/dbus_eval/target/release/mvad_phi1"

if not os.path.exists(BIN_PATH):
    logging.error("FATAL: mvad_phi1 binary not found")
    sys.exit(1)

beds = glob.glob("/tmp/w7a/beds/*.wav")
fixtures = glob.glob("/tmp/w7a/fixtures/*.wav")
w6b = glob.glob("/tmp/w6b/*/mix.wav")

if len(beds) != 28 or len(fixtures) != 2 or len(w6b) != 60:
    logging.error(f"FATAL: Missing files! beds={len(beds)}/28, fixtures={len(fixtures)}/2, w6b={len(w6b)}/60")
    sys.exit(1)

with open("/tmp/phi1/rust_beds.csv", "w") as f:
    f.write("bed,frames,ratio_05,ratio_07\n")
with open("/tmp/phi1/rust_fixtures.csv", "w") as f:
    f.write("fixture,in_05,in_07,out_05,out_07\n")
with open("/tmp/phi1/rust_pos.csv", "w") as f:
    f.write("mix,snr_tag,in_05,in_07,out_05,out_07\n")

start_time = time.time()
total_frames = 0

def run_bin(*args):
    global total_frames
    norm_mode = os.environ.get("PHI1_NORM_MODE", "chunk")
    cmd = [BIN_PATH] + list(args) + [f"--norm={norm_mode}"]
    res = subprocess.run(cmd, capture_output=True, text=True)
    out = res.stdout.strip()
    if not out.startswith("PHI1|"):
        logging.error(f"Unexpected output from binary for {args}: {out}")
        return None
    parts = out.split('|')[1:]
    data = {}
    for p in parts:
        if '=' in p:
            k, v = p.split('=', 1)
            data[k] = v
    total_frames += int(data['frames'])
    return data

logging.info("Running BEDS...")
bed_results = []
for bed in sorted(beds):
    data = run_bin(bed)
    if data:
        bed_results.append({
            "bed": data['file'],
            "frames": int(data['frames']),
            "ratio_05": float(data['ratio_05']),
            "ratio_07": float(data['ratio_07'])
        })
        with open("/tmp/phi1/rust_beds.csv", "a") as f:
            f.write(f"{data['file']},{data['frames']},{data['ratio_05']},{data['ratio_07']}\n")

logging.info("Running FIXTURES...")
fixture_results = []
for fix in sorted(fixtures):
    data = run_bin(fix, "10", "20")
    if data:
        fixture_results.append({
            "fixture": data['file'],
            "in_05": float(data['in_05']),
            "in_07": float(data['in_07']),
            "out_05": float(data['out_05']),
            "out_07": float(data['out_07'])
        })
        with open("/tmp/phi1/rust_fixtures.csv", "a") as f:
            f.write(f"{data['file']},{data['in_05']},{data['in_07']},{data['out_05']},{data['out_07']}\n")

logging.info("Running W6B...")
w6b_results = []
for mix in sorted(w6b):
    folder_name = mix.split('/')[-2]
    snr_tag = folder_name.split('__snr')[-1] if '__snr' in folder_name else folder_name
    data = run_bin(mix, "10", "20")
    if data:
        w6b_results.append({
            "mix": data['file'],
            "snr_tag": snr_tag,
            "in_05": float(data['in_05']),
            "in_07": float(data['in_07']),
            "out_05": float(data['out_05']),
            "out_07": float(data['out_07'])
        })
        with open("/tmp/phi1/rust_pos.csv", "a") as f:
            f.write(f"{data['file']},{snr_tag},{data['in_05']},{data['in_07']},{data['out_05']},{data['out_07']}\n")

end_time = time.time()
elapsed = end_time - start_time

df_beds = pd.DataFrame(bed_results)
df_w6b = pd.DataFrame(w6b_results)
df_fix = pd.DataFrame(fixture_results)

logging.info("\n--- ΑΝΑΛΥΣΗ (RUST JURY) ---")
beds_med_05 = df_beds['ratio_05'].median()
beds_med_07 = df_beds['ratio_07'].median()
beds_under_05 = (df_beds['ratio_05'] < 0.05).sum()
beds_under_07 = (df_beds['ratio_07'] < 0.05).sum()
worst_05 = df_beds.nlargest(3, 'ratio_05')['bed'].tolist()
worst_07 = df_beds.nlargest(3, 'ratio_07')['bed'].tolist()

logging.info("BEDS:")
logging.info(f"Median ratio_05: {beds_med_05:.3f} | <0.05: {beds_under_05}")
logging.info(f"Median ratio_07: {beds_med_07:.3f} | <0.05: {beds_under_07}")
logging.info(f"Worst beds (0.5): {worst_05}")
logging.info(f"Worst beds (0.7): {worst_07}")

logging.info("\nW6B ανά snr_tag (medians):")
snr_medians = df_w6b.groupby('snr_tag').median(numeric_only=True)
for snr, row in snr_medians.iterrows():
    logging.info(f"{snr:3s}: in_05={row['in_05']:.3f}, in_07={row['in_07']:.3f}, out_05={row['out_05']:.3f}, out_07={row['out_07']:.3f}")

logging.info("\nFIXTURES:")
for _, r in df_fix.iterrows():
    logging.info(f"{r['fixture']}: in_05={r['in_05']:.3f}, in_07={r['in_07']:.3f}, out_05={r['out_05']:.3f}, out_07={r['out_07']:.3f}")

py_beds_07 = 0.057
py_w6b_p3_in_07 = 0.909
py_w6b_p3_out_07 = 0.017
py_fix_synth_in_07 = 0.925
py_fix_synth_out_07 = 0.059

rs_beds_07 = beds_med_07
rs_w6b_p3_in_07 = snr_medians.loc['p3', 'in_07']
rs_w6b_p3_out_07 = snr_medians.loc['p3', 'out_07']
synth_row = df_fix[df_fix['fixture'].str.contains('synth')].iloc[0] if sum(df_fix['fixture'].str.contains('synth')) else df_fix.iloc[0]
rs_fix_synth_in_07 = synth_row['in_07']
rs_fix_synth_out_07 = synth_row['out_07']

def check_diff(metric, py_val, rs_val):
    diff = abs(py_val - rs_val)
    dev = " (ΑΠΟΚΛΙΣΗ)" if diff > 0.01 else ""
    return f"{metric:20s} | {py_val:6.3f} | {rs_val:6.3f}{dev}"

logging.info("\nΟ ΠΙΝΑΚΑΣ ΤΑΥΤΙΣΗΣ (Python vs Rust @0.7):")
logging.info(f"{'metric':20s} | Python | Rust")
logging.info(check_diff("beds median", py_beds_07, rs_beds_07))
logging.info(check_diff("w6b p3 in", py_w6b_p3_in_07, rs_w6b_p3_in_07))
logging.info(check_diff("w6b p3 out", py_w6b_p3_out_07, rs_w6b_p3_out_07))
logging.info(check_diff("fixture synth in", py_fix_synth_in_07, rs_fix_synth_in_07))
logging.info(check_diff("fixture synth out", py_fix_synth_out_07, rs_fix_synth_out_07))

fps = total_frames / elapsed if elapsed > 0 else 0
logging.info(f"\nTime: {elapsed:.2f}s | FPS: {fps:.1f} frames/sec")

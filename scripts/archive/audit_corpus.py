#!/usr/bin/env python3
import os
import subprocess
import re
from collections import defaultdict
import shutil
import urllib.request
import json

log_file = "/home/aidevcon/.gemini/antigravity/brain/ccd85db0-555a-4792-91b0-067c2811511c/.system_generated/tasks/task-19602.log"

# Parse log file to map wav files to identifiers and original names
file_metadata = {}
class_counts = {"music": 0, "speech": 0}

with open(log_file, "r") as f:
    for line in f:
        m = re.search(r'\[(music|speech)\] Extracting 30s of (.+?) from (.+?)\.\.\.', line)
        if m:
            category = m.group(1)
            orig_name = m.group(2)
            ident = m.group(3)
            
            ext = orig_name.split(".")[-1].lower()
            idx = class_counts[category]
            wav_name = f"{category}_{idx}.wav"
            
            if os.path.exists(f"/tmp/diverse_corpus/{category}/{wav_name}"):
                file_metadata[wav_name] = {
                    "category": category,
                    "orig_name": orig_name,
                    "ident": ident,
                    "orig_ext": ext
                }
            class_counts[category] += 1

results = {"music": [], "speech": []}
ident_counts = {"music": defaultdict(int), "speech": defaultdict(int)}

for wav_name, meta in file_metadata.items():
    cat = meta["category"]
    ident = meta["ident"]
    path = f"/tmp/diverse_corpus/{cat}/{wav_name}"
    if not os.path.exists(path):
        continue
        
    ident_counts[cat][ident] += 1
    
    # 1. Side energy
    cmd_side = ["ffmpeg", "-i", path, "-af", "pan=mono|c0=0.5*c0-0.5*c1,volumedetect", "-f", "null", "-"]
    res_side = subprocess.run(cmd_side, stderr=subprocess.PIPE, text=True)
    mean_side = -999.0
    for l in res_side.stderr.split("\n"):
        if "mean_volume:" in l:
            try: mean_side = float(re.search(r'mean_volume: ([-0-9.]+) dB', l).group(1))
            except: pass
            break
            
    # 4. Mean volume
    cmd_vol = ["ffmpeg", "-i", path, "-af", "volumedetect", "-f", "null", "-"]
    res_vol = subprocess.run(cmd_vol, stderr=subprocess.PIPE, text=True)
    mean_vol = -999.0
    for l in res_vol.stderr.split("\n"):
        if "mean_volume:" in l:
            try: mean_vol = float(re.search(r'mean_volume: ([-0-9.]+) dB', l).group(1))
            except: pass
            break
            
    # 3. Content vs Label
    meta_url = f"https://archive.org/metadata/{ident}"
    title = ident
    try:
        req = urllib.request.Request(meta_url)
        with urllib.request.urlopen(req) as response:
            d = json.loads(response.read())
            title = d.get("metadata", {}).get("title", ident)
    except:
        pass
    
    meta["side_energy"] = mean_side
    meta["mean_vol"] = mean_vol
    meta["title"] = title
    
    # Original duration parsing (fallback to file size approximation if it's 30s)
    meta["duration"] = "30s"
    meta["channels"] = "2 (converted)"
    
    results[cat].append(meta)

os.makedirs("/tmp/diverse_corpus/holdout", exist_ok=True)

with open("/tmp/audit_report.txt", "w") as out:
    out.write("=== 1. THE MONO TRAP ===\n")
    for cat in ["music", "speech"]:
        fake_stereo = sum(1 for m in results[cat] if m["side_energy"] < -80 or m["side_energy"] == -999.0)
        out.write(f"{cat.capitalize()}: {fake_stereo} / {len(results[cat])} files have side energy < -80 dBFS (fake stereo)\n")
        if cat == "speech" and fake_stereo > 0:
            out.write("WARNING: Lopsided! Axis D MUST BE EXCLUDED from this calibration.\n")
    
    out.write("\n=== 2. THE REAL n ===\n")
    for cat in ["music", "speech"]:
        out.write(f"{cat.capitalize()}:\n")
        for ident, c in ident_counts[cat].items():
            out.write(f"  - {ident}: {c} files\n")
            if c >= 3:
                out.write(f"    WARNING: {ident} contributed {c} files (near-duplicates)\n")
            
    out.write("\n=== 3. CONTENT VS LABEL & 4. THE FIRST 30 SECONDS ===\n")
    for cat in ["music", "speech"]:
        for m in results[cat]:
            flag = ""
            if cat == "speech" and ("music" in m["title"].lower() or "symphony" in m["title"].lower()): flag = "[FLAG: TITLE MISMATCH]"
            if cat == "music" and ("speech" in m["title"].lower() or "podcast" in m["title"].lower() or "talk" in m["title"].lower()): flag = "[FLAG: TITLE MISMATCH]"
            vol_flag = "[FLAG: QUIET INTRO]" if m["mean_vol"] < -40 else ""
            out.write(f"{cat.upper()} | {m['orig_name']} | Ident: {m['ident']} | Title: {m['title'][:30]} | MeanVol: {m['mean_vol']}dB | SideVol: {m['side_energy']}dB {flag} {vol_flag}\n")

    out.write("\n=== 5. HELD-OUT SET ===\n")
    holdouts = {"music": [], "speech": []}
    for cat in ["music", "speech"]:
        single_idents = [ident for ident, count in ident_counts[cat].items() if count == 1]
        available = [m for m in results[cat] if m["ident"] in single_idents]
        
        # If not enough single idents, pull from ANY idents to satisfy count of 3
        if len(available) < 3:
            available = results[cat]
            
        for i in range(min(3, len(available))):
            h_meta = available[i]
            holdouts[cat].append(h_meta)
            for w, m in file_metadata.items():
                if m == h_meta:
                    src = f"/tmp/diverse_corpus/{cat}/{w}"
                    dst = f"/tmp/diverse_corpus/holdout/{w}"
                    if os.path.exists(src):
                        shutil.move(src, dst)
                    out.write(f"Moved {w} ({h_meta['ident']}) to holdout.\n")
                    break

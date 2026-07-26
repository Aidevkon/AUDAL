#!/usr/bin/env python3
import os
import subprocess
import json
import re

def get_passing_files():
    cmd = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/diverse_corpus", "--allow-mono"]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, text=True)
    passing = []
    for line in res.stdout.split("\n"):
        if line.startswith("PASS:"):
            # PASS: music_11.wav | dur=30.00s ...
            filename = line.split("|")[0].replace("PASS:", "").strip()
            passing.push(filename)  # wait, append!
    return res.stdout

def main():
    # 1. Get passing files from gate
    cmd = ["/home/aidevcon/Documents/creator-os/scripts/gate_corpus.py", "/tmp/diverse_corpus", "--allow-mono"]
    res = subprocess.run(cmd, stdout=subprocess.PIPE, text=True)
    
    passing = []
    for line in res.stdout.split("\n"):
        if line.startswith("PASS:"):
            filename = line.split("|")[0].replace("PASS:", "").strip()
            passing.append(filename)
            
    # 2. Map filename to source ident
    log_file = "/home/aidevcon/.gemini/antigravity/brain/ccd85db0-555a-4792-91b0-067c2811511c/.system_generated/tasks/task-19602.log"
    file_to_source = {}
    class_counts = {"music": 0, "speech": 0}
    with open(log_file, "r") as f:
        for line in f:
            m = re.search(r'\[(music|speech)\] Extracting 30s of .+? from (.+?)\.\.\.', line)
            if m:
                cat = m.group(1)
                ident = m.group(2)
                idx = class_counts[cat]
                wav_name = f"{cat}_{idx}.wav"
                file_to_source[wav_name] = ident
                class_counts[cat] += 1
                
    # 3. Measure variance_a for each passing file
    bin_path = "/home/aidevcon/Documents/creator-os/target/debug/measure_variance"
    
    data = []
    
    print(f"{'FILE':<15} | {'CLASS':<10} | {'SOURCE (IDENT)':<30} | {'VAR_A'}")
    print("-" * 75)
    
    variance_by_class = {"music": [], "speech": []}
    
    for filename in passing:
        cat = "music" if filename.startswith("music") else "speech"
        path = f"/tmp/diverse_corpus/{cat}/{filename}"
        
        # Call the Rust measurement tool
        try:
            out = subprocess.check_output([bin_path, path], text=True).strip()
            var_a = float(out)
        except Exception as e:
            print(f"Error measuring {filename}: {e}")
            var_a = 0.0
            
        source = file_to_source.get(filename, "unknown")
        
        print(f"{filename:<15} | {cat:<10} | {source:<30} | {var_a:.1f}")
        
        data.append({
            "file": filename,
            "class": cat,
            "source": source,
            "variance_a": var_a
        })
        variance_by_class[cat].append(var_a)
        
    # 4. Report variance spread per class
    print("\n=== VARIANCE_A SPREAD PER CLASS ===")
    import numpy as np
    for cat in ["music", "speech"]:
        vals = variance_by_class[cat]
        if not vals: continue
        p0 = np.min(vals)
        p25 = np.percentile(vals, 25)
        p50 = np.median(vals)
        p75 = np.percentile(vals, 75)
        p100 = np.max(vals)
        print(f"{cat.upper()}: Min={p0:.1f}, p25={p25:.1f}, Median={p50:.1f}, p75={p75:.1f}, Max={p100:.1f}")
        
    # 5. Run leave-one-out
    print("\n=== LEAVE-ONE-OUT ANALYSIS ===")
    loo_script = "/home/aidevcon/Documents/creator-os/scripts/leave_one_out.py"
    try:
        proc = subprocess.run(["python3", loo_script], input=json.dumps(data), text=True, stdout=subprocess.PIPE)
        print(proc.stdout)
    except Exception as e:
        print(f"Error running loo script: {e}")

if __name__ == "__main__":
    main()

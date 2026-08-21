#!/usr/bin/env python3
"""
rebuild_allowlist_v2.py — Rebuild FMA allowlist from embedded ID3 tags (v2 clean-room)
"""

import os
import sys
import csv
import json
import time
import subprocess
import numpy as np
from pathlib import Path
from datetime import datetime, timezone
from multiprocessing import Pool

OLD_ALLOWLIST = Path("/home/aidevcon/Downloads/DATASET/fma/fma_small_cc_allowlist.json")
V2_ALLOWLIST = Path("/home/aidevcon/Downloads/DATASET/fma/fma_small_cc_allowlist_v2.json")
REJECTED_V2 = Path("/home/aidevcon/Downloads/DATASET/fma/rejected_v2.json")

ATTIC_DIR = Path("/home/aidevcon/creator-os-attic/drumscan-2026-08")
RANKED_TSV = ATTIC_DIR / "ranked.tsv"
RANKED_V2_TSV = ATTIC_DIR / "ranked_v2.tsv"
AUDITION_V2_MD = ATTIC_DIR / "audition_top40_v2.md"

def inspect_track(item):
    path = item["path"]
    tid = str(item["track_id"])
    
    cmd = ["ffprobe", "-v", "quiet", "-show_format", "-of", "json", path]
    res = subprocess.run(cmd, capture_output=True, text=True)
    if res.returncode != 0:
        return (item, False, "ffprobe_error", "")
        
    data = json.loads(res.stdout) if res.stdout else {}
    tags = data.get("format", {}).get("tags", {})
    
    # Extract copyright / comment / license or all tags text
    tag_values = [str(v) for v in tags.values()]
    tag_text = " | ".join(tag_values)
    lower_text = tag_text.lower()
    
    if not lower_text.strip():
        return (item, False, "empty_metadata_tags", "")
        
    # License checks
    # 1. Explicit CC-BY / CC0 / Public Domain URL or tag
    has_by = ("creativecommons.org/licenses/by/" in lower_text) or \
             ("/publicdomain/" in lower_text) or \
             ("cc0" in lower_text) or \
             ("attribution 3.0" in lower_text and "noncommercial" not in lower_text and "share-alike" not in lower_text)
             
    # 2. Check forbidden NC, ND, SA restrictions
    import re
    has_nc = bool(re.search(r"\b(nc|noncommercial|non-commercial|fma-limited|by-nc|nc-sa|nc-nd)\b", lower_text))
    has_nd = bool(re.search(r"\b(nd|noderivatives|no-derivatives|no derivatives|by-nd)\b", lower_text))
    has_sa = bool(re.search(r"\b(sa|sharealike|share-alike|share alike|by-sa)\b", lower_text))
    
    if not has_by:
        return (item, False, "missing_explicit_cc_by_or_cc0_tag", tag_text)
    if has_nc:
        return (item, False, "contains_nc_restriction", tag_text)
    if has_nd:
        return (item, False, "contains_nd_restriction", tag_text)
    if has_sa:
        return (item, False, "contains_sa_restriction", tag_text)
        
    return (item, True, "clean_cc_by", tag_text)

def zscore(arr):
    std = arr.std()
    if std == 0:
        return np.zeros_like(arr)
    return (arr - arr.mean()) / std

def main():
    if not OLD_ALLOWLIST.exists():
        print(f"ERROR: Old allowlist not found: {OLD_ALLOWLIST}")
        sys.exit(1)
        
    with open(OLD_ALLOWLIST, "r") as f:
        old_items = json.load(f)
        
    print(f"[v2 rebuild] Evaluated {len(old_items)} items from old allowlist...")
    
    with Pool(16) as p:
        results = p.map(inspect_track, old_items)
        
    kept_results = [r for r in results if r[1]]
    rejected_results = [r for r in results if not r[1]]
    
    num_kept = len(kept_results)
    print(f"[v2 rebuild] Kept = {num_kept}, Rejected = {len(rejected_results)}")
    
    # Check expected range (~438 +/- 5)
    if not (433 <= num_kept <= 443):
        print(f"FATAL: Kept count {num_kept} deviates from expected ~438 by more than +/-5!")
        print("Deviating items count:", num_kept)
        sys.exit(1)
        
    verified_iso = datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
    
    # Build v2 allowlist json
    v2_allowlist = []
    kept_tids = set()
    tag_raw_map = {}
    
    for item, ok, reason, raw_tag in kept_results:
        tid_str = str(item["track_id"])
        kept_tids.add(tid_str)
        tag_raw_map[tid_str] = raw_tag
        
        entry = {
            "track_id": item["track_id"],
            "license": item.get("license", "Attribution"),
            "path": item["path"],
            "license_source": "id3_embedded",
            "verified_date": verified_iso,
            "license_raw": raw_tag
        }
        v2_allowlist.append(entry)
        
    with open(V2_ALLOWLIST, "w") as f:
        json.dump(v2_allowlist, f, indent=2)
    print(f"[v2 rebuild] Wrote {V2_ALLOWLIST} ({len(v2_allowlist)} entries)")
    
    # Build rejected v2 json
    rejected_dict = {}
    rejection_counts = {}
    for item, ok, reason, raw_tag in rejected_results:
        tid_str = str(item["track_id"])
        rejected_dict[tid_str] = {
            "path": item["path"],
            "rejection_reason": reason,
            "license_raw": raw_tag
        }
        rejection_counts[reason] = rejection_counts.get(reason, 0) + 1
        
    with open(REJECTED_V2, "w") as f:
        json.dump(rejected_dict, f, indent=2)
    print(f"[v2 rebuild] Wrote {REJECTED_V2} ({len(rejected_dict)} entries)")
    
    # Summary calculations
    hours = (num_kept * 30.0) / 3600.0
    print("\n" + "="*50)
    print("=== SUMMARY BLOCK: ALLOWLIST V2 REBUILD ===")
    print(f"Total old allowlist items: {len(old_items)}")
    print(f"Total kept (Clean CC-BY / CC0): {num_kept}")
    print(f"Total audio duration kept: {hours:.2f} hours ({num_kept * 30}s)")
    print(f"Total rejected: {len(rejected_results)}")
    print("Rejection Breakdown by Category:")
    for reason, count in sorted(rejection_counts.items(), key=lambda x: x[1], reverse=True):
        print(f"  - {reason:<35}: {count:>4} tracks")
    print("="*50 + "\n")
    
    # Filter ranked.tsv -> ranked_v2.tsv if ranked.tsv exists
    if RANKED_TSV.exists():
        print(f"[v2 rebuild] Re-filtering {RANKED_TSV}...")
        rows = []
        with open(RANKED_TSV, "r") as f:
            reader = csv.DictReader(f, delimiter="\t")
            for r in reader:
                tid_str = str(r["track_id"])
                if tid_str in kept_tids:
                    rows.append({
                        "track_id": tid_str,
                        "path": r["path"],
                        "onset_rate": float(r["onset_rate"]),
                        "perc_ratio": float(r["perc_ratio"]),
                        "flatness": float(r["flatness"])
                    })
                    
        print(f"[v2 rebuild] Filtered ranked population from {RANKED_TSV.name}: {len(rows)} tracks kept.")
        
        # Recalculate z-scores
        onset_arr = np.array([r["onset_rate"] for r in rows])
        perc_arr = np.array([r["perc_ratio"] for r in rows])
        flat_arr = np.array([r["flatness"] for r in rows])
        
        scores = zscore(onset_arr) + zscore(perc_arr) + zscore(flat_arr)
        for r, s in zip(rows, scores):
            r["score"] = float(s)
            
        rows.sort(key=lambda r: r["score"], reverse=True)
        
        # Write ranked_v2.tsv
        with open(RANKED_V2_TSV, "w", newline="") as f:
            writer = csv.writer(f, delimiter="\t")
            writer.writerow(["rank", "score", "onset_rate", "perc_ratio", "flatness", "path", "track_id"])
            for rank, r in enumerate(rows, 1):
                writer.writerow([
                    rank,
                    f"{r['score']:.4f}",
                    f"{r['onset_rate']:.4f}",
                    f"{r['perc_ratio']:.4f}",
                    f"{r['flatness']:.4f}",
                    r["path"],
                    r["track_id"]
                ])
        print(f"[v2 rebuild] Wrote {RANKED_V2_TSV} ({len(rows)} rows).")
        
        # Write audition_top40_v2.md
        top40 = rows[:40]
        with open(AUDITION_V2_MD, "w") as f:
            f.write("# drumscan — TOP-40 audition list (v2 Clean-Room CC-BY)\n\n")
            f.write(f"Scanned population: {len(rows)} verified CC-BY tracks.\n\n")
            f.write("| Rank | Score | Track ID | Path | License Raw |\n")
            f.write("| ---: | ----: | -------: | :--- | :---------- |\n")
            for rank, r in enumerate(top40, 1):
                raw = tag_raw_map.get(r["track_id"], "").replace("\n", " ").replace("\r", " ")
                f.write(f"| {rank} | {r['score']:.4f} | {r['track_id']} | {r['path']} | `{raw[:80]}` |\n")
        print(f"[v2 rebuild] Wrote {AUDITION_V2_MD} (top 40).")

if __name__ == "__main__":
    main()

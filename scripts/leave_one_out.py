#!/usr/bin/env python3
"""
TOOL: leave_one_out.py
PURPOSE: Cross-validation procedure evaluating features by leaving out one source per iteration.
USAGE: Takes dataset JSON and evaluates threshold variance across splits.
REFERENCE: Documented in F-041 (showed a threshold moving by 28742 on one axis and by 0.03 on another).
"""
import sys
import json
import numpy as np
from collections import defaultdict

def main():
    # Expects JSON data on stdin:
    # [
    #   {"file": "music_1.wav", "class": "music", "source": "albumA", "variance_a": 0.05},
    #   {"file": "speech_1.wav", "class": "speech", "source": "podcastB", "variance_a": 0.002},
    #   ...
    # ]
    
    if sys.stdin.isatty():
        print("Usage: cat data.json | leave_one_out.py")
        sys.exit(1)
        
    data = json.load(sys.stdin)
    
    def get_best_threshold(subset):
        # subset is a list of dicts
        vals = []
        for d in subset:
            vals.append((d["variance_a"], d["class"]))
            
        # We assume one class typically has higher variance than the other.
        # Let's say music > speech for variance_a.
        # Threshold T: if v > T then music else speech.
        # Sort all unique values to test all possible thresholds
        unique_vals = sorted(list(set([v for v, c in vals])))
        best_acc = -1.0
        best_T = 0.0
        
        for i in range(len(unique_vals) + 1):
            if i == 0:
                T = unique_vals[0] - 0.1
            elif i == len(unique_vals):
                T = unique_vals[-1] + 0.1
            else:
                T = (unique_vals[i-1] + unique_vals[i]) / 2.0
                
            correct = 0
            for v, c in vals:
                pred = "music" if v > T else "speech"
                if pred == c:
                    correct += 1
                    
            acc = correct / len(vals)
            if acc > best_acc:
                best_acc = acc
                best_T = T
                
        return best_T, best_acc

    # 1. Best threshold on full set
    full_T, full_acc = get_best_threshold(data)
    print(f"FULL SET THRESHOLD: {full_T:.4f} (Accuracy: {full_acc:.2%})")
    
    # 2. Leave one source out
    sources = list(set([d["source"] for d in data]))
    print(f"\nLEAVE-ONE-OUT ANALYSIS ({len(sources)} sources):")
    
    thresholds = []
    for s in sources:
        subset = [d for d in data if d["source"] != s]
        if len(subset) == 0: continue
        T, acc = get_best_threshold(subset)
        thresholds.append(T)
        print(f"  Removed source '{s}': Threshold = {T:.4f} (Acc on rest: {acc:.2%})")
        
    if thresholds:
        min_T = min(thresholds)
        max_T = max(thresholds)
        spread = max_T - min_T
        print(f"\nSUMMARY:")
        print(f"  Min Threshold: {min_T:.4f}")
        print(f"  Max Threshold: {max_T:.4f}")
        print(f"  Spread:        {spread:.4f}")

if __name__ == "__main__":
    main()

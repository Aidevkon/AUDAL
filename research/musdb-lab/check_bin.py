import numpy as np
import matplotlib.pyplot as plt
import sys
import os

def main():
    if len(sys.argv) < 2:
        print("Usage: python check_bin.py <path_to_bin>")
        return
        
    bin_path = sys.argv[1]
    with open(bin_path, "rb") as f:
        try:
            data = np.frombuffer(f.read(), dtype=np.float32)
            # Reshape to (128, K, 8) where K can be 2 or 3
            # We can infer K from the total size
            K = len(data) // (128 * 8)
            print(f"Inferred K={K}")
            W = data.reshape(128, K, 8)
        except Exception as e:
            print(f"Error reading {sys.argv[1]}: {e}")
            return
        
    print(f"Loaded templates shape: {W.shape}")
    
    for k in range(K):
        comp = W[:, k, :]
        total_mass = np.sum(np.abs(comp))
        low_mass_13 = np.sum(np.abs(comp[0:14, :])) # Mel 0-13 (up to 328 Hz)
        low_mass_8 = np.sum(np.abs(comp[0:9, :]))   # Mel 0-8 (up to 187 Hz)
        low_mass_4 = np.sum(np.abs(comp[0:5, :]))   # Mel 0-4 (up to 94 Hz)
        
        pct_13 = (low_mass_13 / total_mass) * 100
        pct_8 = (low_mass_8 / total_mass) * 100
        pct_4 = (low_mass_4 / total_mass) * 100
        
        print(f"Component {k}: Total Mass = {total_mass:.4f}")
        print(f"  Mel 0-4  (<100Hz) : {low_mass_4:.4f} ({pct_4:.2f}%)")
        print(f"  Mel 0-8  (<200Hz) : {low_mass_8:.4f} ({pct_8:.2f}%)")
        print(f"  Mel 0-13 (<330Hz) : {low_mass_13:.4f} ({pct_13:.2f}%)")
        
    # Redraw PNG
    fig, axes = plt.subplots(1, 2, figsize=(10, 5))
    for k in range(2):
        template = W[:, k, :]
        im = axes[k].imshow(template, origin='lower', aspect='auto', cmap='viridis', interpolation='nearest')
        axes[k].set_title(f"Sung Component {k}")
        axes[k].set_xlabel("Tau (frames)")
        axes[k].set_ylabel("Mel Band")
        fig.colorbar(im, ax=axes[k])
        
    plt.tight_layout()
    out_png = bin_path.replace(".bin", ".png")
    plt.savefig(out_png, dpi=150)
    print(f"Saved {out_png}")

if __name__ == "__main__":
    main()

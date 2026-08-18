import numpy as np

def measure_speech_norms():
    path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/assets/w_speech_v1.bin"
    # Layout is [mel=128][component=4][tau=8]
    data = np.fromfile(path, dtype=np.float32)
    # 128 * 4 * 8 = 4096
    
    if len(data) != 4096:
        print(f"Error: expected 4096 elements, got {len(data)}")
        return
        
    w_speech = data.reshape((128, 4, 8))
    
    print("w_speech_v1.bin norms:")
    for k in range(4):
        comp = w_speech[:, k, :]
        l1_norm = np.sum(np.abs(comp))
        l2_norm = np.sqrt(np.sum(comp**2))
        max_val = np.max(comp)
        
        print(f"  Component {k}: L1={l1_norm:.6f}, L2={l2_norm:.6f}, Max={max_val:.6f}")

if __name__ == "__main__":
    measure_speech_norms()

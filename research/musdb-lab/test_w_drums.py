import numpy as np
w = np.fromfile('/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/assets/w_drums_v1.bin', dtype=np.float32).reshape(128, 3, 8)
print("Sum of W:", np.sum(w))
for k in range(3):
    print(f"k={k} sum={np.sum(w[:,k,:])}")

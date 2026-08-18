import numpy as np
import os

def main():
    drums_path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/assets/w_drums_v1.bin"
    sung_path = "/home/aidevcon/Documents/creator-os/research/musdb-lab/w_sung_v3.bin"
    out_path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/assets/w_music_v1.bin"
    
    with open(drums_path, "rb") as f:
        drums = np.frombuffer(f.read(), dtype=np.float32).reshape(128, 3, 8)
        
    with open(sung_path, "rb") as f:
        sung = np.frombuffer(f.read(), dtype=np.float32).reshape(128, 2, 8)
        
    music = np.concatenate((drums, sung), axis=1)
    
    # Assert shape and sizes
    assert music.shape == (128, 5, 8)
    
    # Assert norms are strictly 1.0 (with a small tolerance)
    for k in range(5):
        norm = np.sum(np.abs(music[:, k, :]))
        assert np.isclose(norm, 1.0, atol=1e-4), f"Template {k} has norm {norm}"
        print(f"Template {k} norm: {norm:.6f} OK")
        
    with open(out_path, "wb") as f:
        f.write(music.tobytes())
        
    print(f"Merged successfully to {out_path} (size: {os.path.getsize(out_path)} bytes)")

if __name__ == "__main__":
    main()

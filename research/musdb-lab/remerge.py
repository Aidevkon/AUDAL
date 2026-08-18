import numpy as np

def main():
    old_music_path = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/assets/w_music_v1.bin"
    sung_path = "/home/aidevcon/Documents/creator-os/research/musdb-lab/w_sung_v5.bin"
    
    with open(old_music_path, "rb") as f:
        old_music = np.frombuffer(f.read(), dtype=np.float32).reshape(128, 6, 8)
        
    drums = old_music[:, 0:3, :]
    
    with open(sung_path, "rb") as f:
        sung = np.frombuffer(f.read(), dtype=np.float32).reshape(128, 3, 8)
        
    new_music = np.concatenate((drums, sung), axis=1)
    
    # Assert shape and sizes
    assert new_music.shape == (128, 6, 8)
    
    # Assert norms are strictly 1.0 (with a small tolerance)
    for k in range(6):
        norm = np.sum(np.abs(new_music[:, k, :]))
        assert np.isclose(norm, 1.0, atol=1e-4), f"Template {k} has norm {norm}"
        print(f"Template {k} norm: {norm:.6f} OK")
        
    with open(old_music_path, "wb") as f:
        f.write(new_music.tobytes())
        
    print(f"Remerged successfully to {old_music_path}")

if __name__ == "__main__":
    main()

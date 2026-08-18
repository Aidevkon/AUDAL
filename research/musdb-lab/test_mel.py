import librosa
import numpy as np

def test_mel():
    sr = 48000
    n_fft = 2048
    n_mels = 128
    
    # Try different fmin and fmax
    for fmin in [0, 20, 27.5]:
        for fmax in [None, 8000, 16000, 22050, 24000]:
            for htk in [False, True]:
                try:
                    m = librosa.filters.mel(sr=sr, n_fft=n_fft, n_mels=n_mels, fmin=fmin, fmax=fmax, htk=htk)
                    if np.isclose(m[0, 1], 0.8099141, atol=1e-4):
                        print(f"MATCH: fmin={fmin}, fmax={fmax}, htk={htk}")
                        return
                except:
                    pass

test_mel()

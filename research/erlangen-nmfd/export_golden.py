import numpy as np
import soundfile as sf
import json
import os
import hashlib
import sys
from libnmfd.core.nmf import nmf
from libnmfd.core.nmfconv import nmfd
from libnmfd.dsp.transforms import forward_stft

def generate_golden():
    # 2. Deterministic fixture
    rng = np.random.default_rng(314159)
    fs = 22050
    t = np.linspace(0, 3, 3 * fs, endpoint=False)
    x = np.sin(2 * np.pi * 440 * t) + np.sin(2 * np.pi * 880 * t)
    x[::fs//2] += 2.0  # click train
    noise = rng.normal(0, 0.1, int(0.5 * fs))
    x[:len(noise)] += noise
    
    sf.write("fixture.wav", x, fs)
    
    # 3. STFT via forward_stft
    # block=1024, hop=512, defaults otherwise: win=np.hanning(1024), reconst_mirror=True, append_frames=True
    Y, A, F = forward_stft(x, block_size=1024, hop_size=512)
    # A is magnitude
    np.save("V.npy", A.astype(np.float64))
    
    params = {
        "stft": {
            "block_size": 1024,
            "hop_size": 512,
            "win": "np.hanning(1024)",
            "reconst_mirror": True,
            "append_frames": True
        },
        "nmf": {
            "num_comp": 4,
            "num_iter": 20,
            "cost_func": "KLDiv"
        },
        "nmfd": {
            "num_comp": 4,
            "num_template_frames": 8,
            "num_iter": 20
        },
        "nmfd_returns": [
            "W (List of Tensors -> np.ndarray)",
            "H (np.ndarray - Activation matrix)",
            "nmfd_V (List of approximated component spectrograms -> np.ndarray)",
            "cost_func (np.ndarray - approximation quality per iteration)",
            "tensor_W (np.ndarray - the template tensor)"
        ]
    }
    with open("params.json", "w") as f:
        json.dump(params, f, indent=4)
        
    # 4. Seeded init W and H
    num_bins = A.shape[0]
    num_frames = A.shape[1]
    
    K = 4
    num_template_frames = 8
    
    # nmfd needs a list of K matrices (num_bins, num_template_frames)
    init_W_nmfd = [rng.random((num_bins, num_template_frames)) for _ in range(K)]
    init_H = rng.random((K, num_frames))
    
    # nmf needs one matrix (num_bins, K)
    init_W_nmf = rng.random((num_bins, K))
    
    np.save("init_W_nmfd.npy", np.array(init_W_nmfd, dtype=np.float64))
    np.save("init_W_nmf.npy", init_W_nmf.astype(np.float64))
    np.save("init_H.npy", init_H.astype(np.float64))
    
    # 5. Run BOTH
    W_nmf, H_nmf, cost_nmf = nmf(A, num_comp=K, cost_func='KLDiv', num_iter=20, init_W=init_W_nmf, init_H=init_H.copy())
    
    nmfd_ret = nmfd(A, num_comp=K, num_frames=num_frames, num_template_frames=num_template_frames, num_iter=20, init_W=init_W_nmfd, init_H=init_H.copy())
    
    # 6. Save NMFD 5-tuple
    # nmfd returns: W, H, nmfd_V, cost_func, tensor_W
    W_out, H_out, nmfd_V_out, cost_func_out, tensor_W_out = nmfd_ret
    
    np.save("nmfd_W.npy", np.array(W_out, dtype=np.float64))
    np.save("nmfd_H.npy", H_out.astype(np.float64))
    np.save("nmfd_V.npy", np.array(nmfd_V_out, dtype=np.float64))
    np.save("nmfd_cost.npy", cost_func_out.astype(np.float64))
    np.save("nmfd_tensor_W.npy", tensor_W_out.astype(np.float64))

    # Also save nmf just in case
    np.save("nmf_W.npy", W_nmf.astype(np.float64))
    np.save("nmf_H.npy", H_nmf.astype(np.float64))
    np.save("nmf_cost.npy", cost_nmf)

if __name__ == "__main__":
    if len(sys.argv) > 1 and sys.argv[1] == "run":
        generate_golden()
    elif len(sys.argv) > 1 and sys.argv[1] == "hash":
        generate_golden()
        # compute hashes
        hashes = {}
        for f in sorted(os.listdir(".")):
            if f.endswith(".npy") or f == "fixture.wav":
                with open(f, "rb") as file:
                    hashes[f] = hashlib.sha256(file.read()).hexdigest()
        print(json.dumps(hashes, indent=2))

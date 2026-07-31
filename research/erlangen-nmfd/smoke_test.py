import time
import numpy as np
import scipy.signal
import inspect
from libnmfd.core.nmf import nmf
from libnmfd.core.nmfconv import nmfd
from libnmfd.dsp.transforms import forward_stft

# Print signatures for EXPORT SHAPE RECON
print("SIGNATURE nmf:", inspect.signature(nmf))
print("SIGNATURE nmfd:", inspect.signature(nmfd))
print("SIGNATURE forward_stft:", inspect.signature(forward_stft))

# Create 2s test signal at 22050 Hz (two sines + clicks)
fs = 22050
t = np.linspace(0, 2, 2 * fs, endpoint=False)
x = np.sin(2 * np.pi * 440 * t) + np.sin(2 * np.pi * 880 * t)
x[::fs//2] += 2.0  # click train (4 clicks)

# STFT
start_stft = time.time()
stft_res = forward_stft(x, block_size=1024, hop_size=512)
V = np.abs(stft_res[0])
print("forward_stft time:", time.time() - start_stft)
print("V shape:", V.shape)

# Run NMF
start_nmf = time.time()
# libnmfd functions generally take V and return a dictionary or tuple
try:
    res_nmf = nmf(V, num_comp=3, num_iter=15)
    print("nmf time:", time.time() - start_nmf)
    print("NMF W shape:", res_nmf[0].shape)
    print("NMF H shape:", res_nmf[1].shape)
except Exception as e:
    print("NMF Error:", e)

# Run NMFD
start_nmfd = time.time()
try:
    # libnmfd has a bug on line 248 of nmfd if init_W is None, so we must provide it
    num_comp = 3
    num_template_frames = 8
    num_bins = V.shape[0]
    num_frames = V.shape[1]
    init_W_list = [np.random.rand(num_bins, num_template_frames) for _ in range(num_comp)]
    init_H_mat = np.random.rand(num_comp, num_frames)
    
    res_nmfd = nmfd(V, num_comp=num_comp, num_iter=15, num_template_frames=num_template_frames, init_W=init_W_list, init_H=init_H_mat)
    print("nmfd time:", time.time() - start_nmfd)
    print("NMFD W shape (len of list):", len(res_nmfd[0]))
    print("NMFD W[0] shape:", res_nmfd[0][0].shape)
    print("NMFD H shape:", res_nmfd[1].shape)
except Exception as e:
    print("NMFD Error:", e)

# Test Determinism (fixed init)
try:
    W_init = np.random.rand(V.shape[0], 3)
    H_init = np.random.rand(3, V.shape[1])
    res1 = nmf(V, num_comp=3, num_iter=5, init_W=W_init, init_H=H_init)
    res2 = nmf(V, num_comp=3, num_iter=5, init_W=W_init, init_H=H_init)
    diff = np.max(np.abs(res1[1] - res2[1]))
    print("Determinism max abs diff (NMF):", diff)
except Exception as e:
    print("Determinism test error:", e)

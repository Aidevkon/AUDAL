import os
import struct
import math

def process(file_in, file_out, target_rms=-16.0):
    if not os.path.exists(file_in):
        print(f"File not found: {file_in}")
        return
        
    size = os.path.getsize(file_in)
    n_floats = size // 4
    n_frames = n_floats // 2
    
    with open(file_in, 'rb') as f:
        data = f.read()
        
    samples = struct.unpack(f'<{n_floats}f', data)
    
    sq_sum = sum(s*s for s in samples)
    if n_floats == 0:
        print("Empty file")
        return
        
    rms = math.sqrt(sq_sum / n_floats)
    rms_db = 20 * math.log10(rms + 1e-9)
    
    diff_sq_sum = 0
    for i in range(0, len(samples), 2):
        d = samples[i] - samples[i+1]
        diff_sq_sum += d*d
    diff_rms = math.sqrt(diff_sq_sum / n_frames)
    diff_db = 20 * math.log10(diff_rms + 1e-9)
    
    gain = 10 ** ((target_rms - rms_db) / 20.0)
    print(f"{file_in}: RMS {rms_db:.2f} dB, L-R diff {diff_db:.2f} dB. Applying gain {gain:.3f}")
    
    out_samples = [s * gain for s in samples]
    out_data = struct.pack(f'<{n_floats}f', *out_samples)
    
    with open(file_out, 'wb') as f:
        f.write(out_data)

process("/tmp/ab_variant_A_scalar.wav", "/tmp/ab_variant_A_scalar_norm.pcm")
process("/tmp/ab_variant_B_spectral.wav", "/tmp/ab_variant_B_spectral_norm.pcm")

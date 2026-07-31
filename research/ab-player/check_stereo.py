import wave
import struct
import math

def check_stereo(file_in):
    with wave.open(file_in, 'rb') as w_in:
        n_channels = w_in.getnchannels()
        n_frames = w_in.getnframes()
        data = w_in.readframes(n_frames)
        sampwidth = w_in.getsampwidth()
        
    if sampwidth == 2:
        samples = struct.unpack(f'<{n_frames * n_channels}h', data)
        norm = 32768.0
    elif sampwidth == 4:
        samples = struct.unpack(f'<{n_frames * n_channels}i', data)
        norm = 2147483648.0
    
    samples = [s / norm for s in samples]
    
    diff_sq_sum = 0
    for i in range(0, len(samples), 2):
        d = samples[i] - samples[i+1]
        diff_sq_sum += d*d
    diff_rms = math.sqrt(diff_sq_sum / n_frames) if n_frames > 0 else 0
    diff_db = 20 * math.log10(diff_rms + 1e-9)
    print(f"{file_in}: L-R diff {diff_db:.2f} dB")

check_stereo("/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav")

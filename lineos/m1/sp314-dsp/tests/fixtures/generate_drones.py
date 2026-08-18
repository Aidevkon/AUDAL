import numpy as np
import soundfile as sf
import os

def generate_white_noise(duration=10.0, sr=48000, seed=42):
    np.random.seed(seed)
    return np.random.randn(int(duration * sr)).astype(np.float32) * 0.1

def generate_pink_noise(duration=10.0, sr=48000, seed=42):
    np.random.seed(seed)
    # Generate pink noise using FFT
    N = int(duration * sr)
    X = np.random.randn(N) + 1j * np.random.randn(N)
    frequencies = np.fft.fftfreq(N)
    frequencies[0] = 1.0  # avoid divide by zero
    X /= np.sqrt(np.abs(frequencies))
    x = np.real(np.fft.ifft(X))
    return (x / np.max(np.abs(x)) * 0.1).astype(np.float32)

def generate_inharmonic_drone(duration=10.0, sr=48000, seed=42):
    np.random.seed(seed)
    t = np.linspace(0, duration, int(duration * sr), endpoint=False)
    
    # Inharmonic drone using odd non-integer multiples
    freqs = [55.0, 117.3, 193.1, 347.8, 599.2, 831.5, 1245.7]
    signal = np.zeros_like(t)
    
    for f in freqs:
        # random phase
        phase = np.random.uniform(0, 2 * np.pi)
        signal += np.sin(2 * np.pi * f * t + phase) * (1.0 / np.sqrt(f))
        
    return (signal / np.max(np.abs(signal)) * 0.1).astype(np.float32)

def main():
    out_dir = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/tests/fixtures"
    os.makedirs(out_dir, exist_ok=True)
    
    sf.write(os.path.join(out_dir, "white_drone.wav"), generate_white_noise(), 48000)
    sf.write(os.path.join(out_dir, "pink_drone.wav"), generate_pink_noise(), 48000)
    sf.write(os.path.join(out_dir, "inharmonic_drone.wav"), generate_inharmonic_drone(), 48000)
    print("Drones generated.")

if __name__ == "__main__":
    main()

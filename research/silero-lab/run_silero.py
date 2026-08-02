import os
import json
import torch
import torchaudio
from pathlib import Path

def process_file(model, file_path, out_dir):
    # Load and resample to 16kHz
    import soundfile as sf
    wav_np, sr = sf.read(file_path)
    if wav_np.ndim > 1:
        wav_np = wav_np.mean(axis=1)
    wav = torch.from_numpy(wav_np).float()
    
    if sr != 16000:
        transform = torchaudio.transforms.Resample(orig_freq=sr, new_freq=16000)
        wav = transform(wav.unsqueeze(0)).squeeze(0)
    
    # Generate probabilities
    chunk_size = 512
    probs = []
    
    model.reset_states()
    with torch.no_grad():
        for i in range(0, len(wav), chunk_size):
            chunk = wav[i : i + chunk_size]
            if len(chunk) < chunk_size:
                # pad
                chunk = torch.nn.functional.pad(chunk, (0, chunk_size - len(chunk)))
            prob = model(chunk, 16000).item()
            probs.append(prob)
            
    # Save p-map
    filename = Path(file_path).name
    out_path = Path(out_dir) / f"{filename}.pmap.json"
    
    data = {
        "frame_rate_hz": 16000 / 512,  # 31.25 Hz
        "probs": [round(p, 4) for p in probs]
    }
    with open(out_path, 'w') as f:
        json.dump(data, f)
        
    return probs

def main():
    try:
        from silero_vad import load_silero_vad
        model = load_silero_vad(onnx=False)
    except ImportError:
        # Fallback to torch hub if pip package import fails
        model, utils = torch.hub.load(repo_or_dir='snakers4/silero-vad',
                                      model='silero_vad',
                                      trust_repo=True)
        
    model.eval()

    fixtures_dir = "/home/aidevcon/Documents/creator-os/lineos/m1/sp314-dsp/tests/fixtures/audiobook"
    out_dir = "/home/aidevcon/Documents/creator-os/research/silero-lab/pmaps"
    os.makedirs(out_dir, exist_ok=True)
    
    # Process all 21 files
    results = {}
    for f in sorted(os.listdir(fixtures_dir)):
        if f.endswith('.flac'):
            file_path = os.path.join(fixtures_dir, f)
            probs = process_file(model, file_path, out_dir)
            speech_frames = sum(1 for p in probs if p >= 0.5)
            speech_pct = 100.0 * speech_frames / len(probs)
            results[f] = speech_pct
            
    # Determinism check (Run voice_1 and mix_2_snr-6 again)
    v1_probs_2 = process_file(model, os.path.join(fixtures_dir, "voice_1.flac"), out_dir)
    m2_probs_2 = process_file(model, os.path.join(fixtures_dir, "mix_2_snr-6.flac"), out_dir)
    
    with open(os.path.join(out_dir, "voice_1.flac.pmap.json"), 'r') as f:
        v1_probs_1 = json.load(f)["probs"]
    with open(os.path.join(out_dir, "mix_2_snr-6.flac.pmap.json"), 'r') as f:
        m2_probs_1 = json.load(f)["probs"]
        
    # Python rounding for equality
    v1_probs_2 = [round(p, 4) for p in v1_probs_2]
    m2_probs_2 = [round(p, 4) for p in m2_probs_2]
    
    print("DETERMINISM CHECK:")
    print(f"voice_1.flac exact match: {v1_probs_1 == v1_probs_2}")
    print(f"mix_2_snr-6.flac exact match: {m2_probs_1 == m2_probs_2}")
    
    # Print the teacher's report card
    # I need to cross reference with the house's report card
    house_pct = {
        "mix_1_snr-6.flac": 100.00,
        "mix_1_snr-15.flac": 100.00,
        "mix_1_snr-25.flac": 100.00,
        "voice_1.flac": 99.92,
        "mix_2_snr-6.flac": 92.56,
        "mix_2_snr-15.flac": 95.88,
        "mix_2_snr-25.flac": 98.28,
        "voice_2.flac": 100.00,
        "mix_3_snr-6.flac": 100.00,
        "mix_3_snr-15.flac": 100.00,
        "mix_3_snr-25.flac": 100.00,
        "voice_3.flac": 100.00,
    }
    
    print("\nTEACHER vs STUDENT REPORT CARD:")
    for f in ["mix_1_snr-6.flac", "mix_1_snr-15.flac", "mix_1_snr-25.flac", "voice_1.flac",
              "mix_2_snr-6.flac", "mix_2_snr-15.flac", "mix_2_snr-25.flac", "voice_2.flac",
              "mix_3_snr-6.flac", "mix_3_snr-15.flac", "mix_3_snr-25.flac", "voice_3.flac"]:
        if f in results:
            print(f"S6|file={f}|silero_speech_pct={results[f]:.2f}|house_speech_pct={house_pct[f]:.2f}")

if __name__ == "__main__":
    main()

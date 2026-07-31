import librosa
import numpy as np
import json
import hashlib
from pathlib import Path

def get_file_hash(filepath):
    h = hashlib.sha256()
    h.update(filepath.read_bytes())
    return h.hexdigest()

def process_and_export(filename, out_dir):
    out_dir = Path(out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)
    basename = Path(filename).stem
    
    # Load and mono downmix (l+r)/2 is handled by librosa.load(..., mono=True)
    y, sr = librosa.load(filename, sr=48000, mono=True)
    
    # STFT parameters
    n_fft = 2048
    hop_length = 512
    
    # exact call + params for the 24-band filterbank
    mel_basis = librosa.filters.mel(sr=48000, n_fft=2048, n_mels=24)
    
    # Compute magnitude spectrogram
    S_linear = np.abs(librosa.stft(y, n_fft=n_fft, hop_length=hop_length))
    
    # Apply filterbank
    S_mel = np.dot(mel_basis, S_linear)
    
    # log(1 + gamma*E) compression matching the export's
    gamma = 10.0
    S_log = np.log1p(gamma * S_mel)
    
    # The SuperFlux recipe via librosa.onset.onset_strength's max_size/lag params
    # exact invocation
    onset_env = librosa.onset.onset_strength(
        S=S_log,
        sr=48000,
        hop_length=512,
        max_size=3,
        lag=1
    )
    
    # Export band energies matrix
    matrix_path = out_dir / f"{basename}_matrix.bin"
    matrix_bytes = np.ascontiguousarray(S_log, dtype='<f8').tobytes()
    matrix_path.write_bytes(matrix_bytes)
    
    # Export onset envelope
    env_path = out_dir / f"{basename}_envelope.bin"
    env_bytes = np.ascontiguousarray(onset_env, dtype='<f8').tobytes()
    env_path.write_bytes(env_bytes)
    
    # Export the mel basis matrix as binary
    mel_basis_path = out_dir / "mel_basis.bin"
    if not mel_basis_path.exists():
        mb_bytes = np.ascontiguousarray(mel_basis, dtype='<f8').tobytes()
        mel_basis_path.write_bytes(mb_bytes)
        
    # Export params
    params = {
        "n_fft": n_fft,
        "hop_length": hop_length,
        "n_mels": 24,
        "sr": sr,
        "gamma": gamma,
        "matrix_shape": S_log.shape,
        "envelope_shape": onset_env.shape,
        "mel_basis_shape": mel_basis.shape
    }
    with open(out_dir / f"{basename}_params.json", "w") as f:
        json.dump(params, f)
        
    return matrix_path, env_path, mel_basis_path

def main():
    fixtures = [
        "../../lineos/m1/sp314-dsp/tests/fixtures/bodleasons_mid.wav",
        "../../flight_clips/clip_speech.wav",
        "../../flight_clips_stereo/clip_podcast_st.wav"
    ]
    
    out_dir = Path("outputs")
    
    print("RUN 1: Generating golden masters...")
    hashes_run1 = {}
    for f in fixtures:
        m, e, mb = process_and_export(f, out_dir)
        hashes_run1[m.name] = get_file_hash(m)
        hashes_run1[e.name] = get_file_hash(e)
        hashes_run1[mb.name] = get_file_hash(mb)
        
    print("RUN 2: Verifying determinism...")
    hashes_run2 = {}
    for f in fixtures:
        m, e, mb = process_and_export(f, out_dir)
        hashes_run2[m.name] = get_file_hash(m)
        hashes_run2[e.name] = get_file_hash(e)
        hashes_run2[mb.name] = get_file_hash(mb)
        
    assert hashes_run1 == hashes_run2, "DETERMINISM FAILURE: Runs produced different output."
    print("DETERMINISM PASSED. Hashes are identical.")
    
    print("\nFILES AND HASHES (Run 1 | Run 2):")
    for name in hashes_run1:
        print(f"{name:30} | {hashes_run1[name]} | {hashes_run2[name]}")

if __name__ == "__main__":
    main()

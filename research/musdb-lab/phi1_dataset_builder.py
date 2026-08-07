import os
import sys
import glob
import json
import random
import csv
import numpy as np
import soundfile as sf
import torch
import torchaudio
import torchaudio.functional as F_audio

class TeeLogger:
    def __init__(self, filename):
        self.terminal = sys.stdout
        self.log = open(filename, "w")
    def write(self, message):
        self.terminal.write(message)
        self.log.write(message)
        self.log.flush()
    def flush(self):
        self.terminal.flush()
        self.log.flush()

def setup_logger():
    os.makedirs("/tmp/phi1", exist_ok=True)
    sys.stdout = TeeLogger("/tmp/phi1/builder.log")
    sys.stderr = sys.stdout

SPEECH_BLOCKLIST = {"422", "2902", "5338", "2277"}
SPEECH_ROOT = os.path.expanduser("~/Downloads/DATASET/librispeech/LibriSpeech/dev-clean")
MUSIC_ALLOWLIST = os.path.expanduser("~/Downloads/DATASET/fma/fma_small_cc_allowlist.json")
TARGET_SR = 16000
SNR_SWEEP = [10, 3, -3, -9]
FRAME_MS = 10
WINDOW_SEC = 5

def fatal(msg):
    print(f"FATAL: {msg}")
    sys.exit(1)

def build_clips():
    if not os.path.exists(SPEECH_ROOT):
        fatal(f"Speech root not found: {SPEECH_ROOT}")
    
    print("BLOCKLIST ενεργό:", SPEECH_BLOCKLIST)
    all_speakers = [d for d in os.listdir(SPEECH_ROOT) if os.path.isdir(os.path.join(SPEECH_ROOT, d))]
    valid_speakers = sorted([s for s in all_speakers if s not in SPEECH_BLOCKLIST])
    print("VALID SPEAKERS (θα χρησιμοποιηθούν):", valid_speakers)
    
    if not valid_speakers:
        fatal("No valid speakers found!")

    print("\nIndexing Speech clips...")
    speech_clips = []
    for spk in valid_speakers:
        spk_files = glob.glob(os.path.join(SPEECH_ROOT, spk, "**/*.flac"), recursive=True)
        for f in spk_files:
            try:
                info = sf.info(f)
                duration = info.duration
                for start_s in range(0, int(duration) - WINDOW_SEC + 1, WINDOW_SEC):
                    speech_clips.append({"speaker": spk, "file": f, "start": start_s, "end": start_s + WINDOW_SEC})
            except Exception as e:
                print(f"Failed to read info for {f}: {e}")
    
    print(f"Total speech clips (5s windows): {len(speech_clips)}")
    
    print("\nIndexing Music clips...")
    if not os.path.exists(MUSIC_ALLOWLIST):
        fatal(f"Music allowlist not found: {MUSIC_ALLOWLIST}")
    
    with open(MUSIC_ALLOWLIST, 'r') as f:
        music_tracks = json.load(f)
        
    music_clips = []
    for t in music_tracks:
        tid = t["track_id"]
        path = t["path"]
        if not os.path.exists(path):
            continue
        try:
            info = sf.info(path)
            duration = info.duration
            for start_s in range(0, int(duration) - WINDOW_SEC + 1, WINDOW_SEC):
                music_clips.append({"track_id": tid, "file": path, "license": t["license"], "start": start_s, "end": start_s + WINDOW_SEC})
        except Exception as e:
            print(f"Failed to read info for {path}: {e}")
            
    print(f"Total music clips (5s windows): {len(music_clips)}")
    
    random.seed(42)
    random.shuffle(valid_speakers)
    split_idx = int(0.85 * len(valid_speakers))
    train_speakers = set(valid_speakers[:split_idx])
    val_speakers = set(valid_speakers[split_idx:])
    
    track_ids = list(set([c["track_id"] for c in music_clips]))
    random.shuffle(track_ids)
    split_idx_trk = int(0.85 * len(track_ids))
    train_tracks = set(track_ids[:split_idx_trk])
    val_tracks = set(track_ids[split_idx_trk:])
    
    splits = {"train": {"speech": [], "music": []}, "val": {"speech": [], "music": []}}
    for c in speech_clips:
        if c["speaker"] in train_speakers: splits["train"]["speech"].append(c)
        elif c["speaker"] in val_speakers: splits["val"]["speech"].append(c)
            
    for c in music_clips:
        if c["track_id"] in train_tracks: splits["train"]["music"].append(c)
        elif c["track_id"] in val_tracks: splits["val"]["music"].append(c)
        
    return splits

def load_and_resample(path, start_s, end_s, target_sr):
    info = sf.info(path)
    start_frame = int(start_s * info.samplerate)
    frames = int((end_s - start_s) * info.samplerate)
    audio, sr = sf.read(path, start=start_frame, frames=frames, dtype='float32')
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
        
    if sr != target_sr:
        wav = torch.from_numpy(audio)
        wav = F_audio.resample(wav, sr, target_sr)
        audio = wav.numpy()
        
    target_len = int((end_s - start_s) * target_sr)
    if len(audio) < target_len:
        audio = np.pad(audio, (0, target_len - len(audio)))
    elif len(audio) > target_len:
        audio = audio[:target_len]
        
    return audio

def get_mel(audio):
    wav = torch.from_numpy(audio).unsqueeze(0)
    mel_transform = torchaudio.transforms.MelSpectrogram(
        sample_rate=TARGET_SR,
        n_fft=int(TARGET_SR * 0.025),
        win_length=int(TARGET_SR * 0.025),
        hop_length=int(TARGET_SR * (FRAME_MS / 1000.0)),
        f_min=0,
        f_max=8000,
        n_mels=64
    )
    mel_spec = mel_transform(wav)
    log_mel = torch.log(mel_spec + 1e-9).squeeze(0)
    return log_mel.numpy()

def get_labels(audio, model):
    import silero_vad
    wav = torch.from_numpy(audio)
    ts = silero_vad.get_speech_timestamps(wav, model, sampling_rate=TARGET_SR)
    
    hop_samples = int(TARGET_SR * (FRAME_MS / 1000.0))
    n_frames = len(audio) // hop_samples + 1
    
    labels = np.zeros(n_frames, dtype=np.float32)
    for t in ts:
        s_f = max(0, int(np.ceil(t['start'] / hop_samples)))
        e_f = min(n_frames, int(np.floor(t['end'] / hop_samples)))
        if s_f < e_f:
            labels[s_f:e_f] = 1.0
    return labels

def rms(audio):
    return np.sqrt(np.mean(audio**2) + 1e-9)

def main():
    setup_logger()
    splits = build_clips()
    
    sys.path.append(os.path.expanduser("~/Documents/creator-os/research/silero-lab/venv/lib/python3.12/site-packages"))
    import silero_vad
    print("Loading Silero VAD...")
    model = silero_vad.load_silero_vad()
    
    out_dir = "/tmp/phi1/dataset"
    os.makedirs(out_dir, exist_ok=True)
    manifest_path = "/tmp/phi1/manifest.csv"
    
    with open(manifest_path, "w", newline='') as f:
        writer = csv.writer(f)
        writer.writerow(["sample_id","split","kind","speech_speaker","speech_file",
                         "music_track_id","music_license","snr_db","npy_path"])
        
        sample_id = 0
        for split_name in ["train", "val"]:
            sp_clips = splits[split_name]["speech"]
            mu_clips = splits[split_name]["music"]
            random.shuffle(sp_clips)
            random.shuffle(mu_clips)
            
            K = int(min(len(sp_clips) / 0.7, len(mu_clips) / 0.6))
            if "PHI1_LIMIT" in os.environ:
                limit = int(os.environ["PHI1_LIMIT"])
                K = min(K, limit)
            if K == 0:
                print(f"Not enough data for {split_name} split")
                continue
                
            n_clean_speech = int(0.4 * K)
            n_clean_music = int(0.3 * K)
            n_mixed = int(0.3 * K)
            
            sp_idx = 0
            mu_idx = 0
            
            print(f"Generating {split_name} split: {K} samples total")
            
            for i in range(K):
                sample_id += 1
                if i < n_clean_speech:
                    kind = "clean_speech"
                elif i < n_clean_speech + n_clean_music:
                    kind = "clean_music"
                else:
                    kind = "mixed"
                
                npy_filename = f"{sample_id:06d}.npy"
                npy_path = os.path.join(out_dir, npy_filename)
                
                spk = ""; sp_f = ""; tr_id = ""; lic = ""; snr = ""
                audio_for_mel = None
                labels = None
                
                try:
                    if kind == "clean_speech":
                        c = sp_clips[sp_idx]; sp_idx += 1
                        spk = c["speaker"]; sp_f = c["file"]
                        audio = load_and_resample(c["file"], c["start"], c["end"], TARGET_SR)
                        labels = get_labels(audio, model)
                        audio_for_mel = audio
                        
                    elif kind == "clean_music":
                        m = mu_clips[mu_idx]; mu_idx += 1
                        tr_id = m["track_id"]; lic = m["license"]
                        audio = load_and_resample(m["file"], m["start"], m["end"], TARGET_SR)
                        hop_samples = int(TARGET_SR * (FRAME_MS / 1000.0))
                        labels = np.zeros(len(audio) // hop_samples + 1, dtype=np.float32)
                        audio_for_mel = audio
                        
                    elif kind == "mixed":
                        c = sp_clips[sp_idx]; sp_idx += 1
                        m = mu_clips[mu_idx]; mu_idx += 1
                        spk = c["speaker"]; sp_f = c["file"]
                        tr_id = m["track_id"]; lic = m["license"]
                        
                        sp_audio = load_and_resample(c["file"], c["start"], c["end"], TARGET_SR)
                        mu_audio = load_and_resample(m["file"], m["start"], m["end"], TARGET_SR)
                        
                        labels = get_labels(sp_audio, model)
                        
                        snr_val = random.choice(SNR_SWEEP)
                        snr = str(snr_val)
                        
                        rms_s = rms(sp_audio)
                        rms_m = rms(mu_audio)
                        if rms_m > 0 and rms_s > 0:
                            target_rms_m = rms_s / (10 ** (snr_val / 20))
                            mu_audio = mu_audio * (target_rms_m / rms_m)
                        
                        audio_for_mel = np.clip(sp_audio + mu_audio, -1.0, 1.0)
                        
                    mel = get_mel(audio_for_mel)
                    min_len = min(mel.shape[1], labels.shape[0])
                    mel = mel[:, :min_len]
                    labels = labels[:min_len]
                    
                    np.save(npy_path, {"mel": mel, "labels": labels})
                    
                    writer.writerow([sample_id, split_name, kind, spk, sp_f, tr_id, lic, snr, npy_path])
                except Exception as e:
                    print(f"Error on sample {sample_id}: {e}")
                    
    print(f"\nDone. Manifest saved to {manifest_path}")

if __name__ == "__main__":
    main()

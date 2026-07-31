import subprocess
import json
import sys

def normalize(src, target_lufs):
    print(f"Normalizing {src}...")
    base = src.replace('.wav', '')
    norm = f"{base}_norm.wav"
    
    cmd1 = ["ffmpeg", "-nostdin", "-hide_banner", "-i", src, "-af", f"loudnorm=I={target_lufs}:print_format=json", "-f", "null", "-"]
    res1 = subprocess.run(cmd1, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, text=True)
    
    out = res1.stderr
    try:
        json_str = out[out.find('{'):out.rfind('}')+1]
        data = json.loads(json_str)
        input_i = data['input_i']
        input_tp = data['input_tp']
        input_lra = data['input_lra']
        input_thresh = data['input_thresh']
    except Exception as e:
        print("Failed to parse JSON:", e)
        return

    cmd2 = [
        "ffmpeg", "-nostdin", "-hide_banner", "-y", "-i", src,
        "-af", f"loudnorm=I={target_lufs}:measured_I={input_i}:measured_TP={input_tp}:measured_LRA={input_lra}:measured_thresh={input_thresh}:linear=true",
        norm
    ]
    subprocess.run(cmd2, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL)
    
    cmd3 = ["ffmpeg", "-nostdin", "-i", norm, "-af", "pan=mono|c0=c0-c1,volumedetect", "-f", "null", "-"]
    res3 = subprocess.run(cmd3, stdout=subprocess.PIPE, stderr=subprocess.PIPE, stdin=subprocess.DEVNULL, text=True)
    out3 = res3.stderr
    
    for line in out3.split('\n'):
        if "mean_volume" in line or "max_volume" in line:
            print(" ", line.strip())

normalize("/tmp/ab_variant_A_scalar.wav", "-16")
normalize("/tmp/ab_variant_B_spectral.wav", "-16")

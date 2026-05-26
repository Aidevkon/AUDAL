import subprocess
import re
import json
import datetime
import os

def run_ffmpeg(file_path):
    cmd = [
        "./lineos/m1/sp314-dsp/bin/ffmpeg",
        "-i", file_path,
        "-af", "ebur128=peak=true",
        "-f", "null", "-"
    ]
    
    result = subprocess.run(cmd, capture_output=True, text=True)
    output = result.stderr
    
    i_match = None
    if "Integrated loudness:" in output:
        i_match = re.search(r'I:\s+([-\d.]+)\s+LUFS', output.split("Integrated loudness:")[-1])
        
    lra_match = None
    if "Loudness range:" in output:
        lra_match = re.search(r'LRA:\s+([-\d.]+)\s+LU', output.split("Loudness range:")[-1])
        
    peak_match = None
    if "True peak:" in output:
        peak_match = re.search(r'Peak:\s+([-\d.]+)\s+dBFS', output.split("True peak:")[-1])
    
    return {
        "path": file_path,
        "integrated_lufs": float(i_match.group(1)) if i_match else None,
        "lra": float(lra_match.group(1)) if lra_match else None,
        "true_peak_dbtp": float(peak_match.group(1)) if peak_match else None
    }

def get_sp314_lufs():
    cmd = ["cargo", "run", "--features", "cli", "--bin", "test_engine"]
    result = subprocess.run(cmd, capture_output=True, text=True, cwd="lineos/m1/sp314-dsp")
    output = result.stderr + result.stdout
    
    pre_match = re.search(r"Pre-pass LUFS:\s+([-\d.]+)", output)
    out_match = re.search(r"Output LUFS:\s+([-\d.]+)", output)
    
    return {
        "pre_pass_lufs": float(pre_match.group(1)) if pre_match else None,
        "output_lufs": float(out_match.group(1)) if out_match else None
    }

def main():
    files = {
        "test_input": "/home/aidevcon/Music/test.wav",
        "test_mastered": "/home/aidevcon/Music/test_mastered.wav",
        "mastered_real": "/home/aidevcon/Music/mastered.wav"
    }
    
    results = {}
    for key, path in files.items():
        if os.path.exists(path):
            results[key] = run_ffmpeg(path)
        else:
            results[key] = {
                "path": path,
                "integrated_lufs": None,
                "lra": None,
                "true_peak_dbtp": None
            }
        
    input_lufs = results["test_input"]["integrated_lufs"]
    output_lufs = results["test_mastered"]["integrated_lufs"]
    true_peak = results["test_mastered"]["true_peak_dbtp"]
    
    sp314_data = get_sp314_lufs()
    sp314_input_lufs = sp314_data["pre_pass_lufs"]
    sp314_output_lufs = sp314_data["output_lufs"]
    
    input_delta_lu = abs(sp314_input_lufs - input_lufs) if (sp314_input_lufs is not None and input_lufs is not None) else None
    output_delta_lu = abs(sp314_output_lufs - output_lufs) if (sp314_output_lufs is not None and output_lufs is not None) else None

    ceiling_dbtp = -0.5
    sp314_lufs = -15.9 
    
    report = {
        "timestamp": datetime.datetime.utcnow().isoformat() + "Z",
        "ffmpeg_version": "7.0.2",
        "files": results,
        "sp314_validation": {
            "sp314_input_lufs": sp314_input_lufs,
            "sp314_output_lufs": sp314_output_lufs,
            "ffmpeg_input_lufs": input_lufs,
            "ffmpeg_output_lufs": output_lufs,
            "input_delta_lu": input_delta_lu,
            "output_delta_lu": output_delta_lu,
            "input_within_tolerance": input_delta_lu < 0.5 if input_delta_lu is not None else False,
            "output_within_tolerance": output_delta_lu < 0.5 if output_delta_lu is not None else False
        },
        "validation": {
            "true_peak_under_ceiling": true_peak <= ceiling_dbtp if true_peak is not None else False,
            "ceiling_dbtp": ceiling_dbtp,
            "lufs_change_db": output_lufs - input_lufs if (output_lufs is not None and input_lufs is not None) else None,
            "sp314_vs_ffmpeg_delta": sp314_lufs - output_lufs if output_lufs is not None else None
        }
    }
    
    os.makedirs("tools/audit", exist_ok=True)
    with open("tools/audit/audit_report.json", "w") as f:
        json.dump(report, f, indent=2)
        
    print(json.dumps(report, indent=2))

if __name__ == "__main__":
    main()

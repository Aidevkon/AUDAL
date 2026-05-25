import json
import os

def main():
    lufs_data = {
        "sample_rate": 48000,
        "test_signal": "1kHz_sine_3s_0.1_amplitude",
        "expected_lufs": -23.0,
        "tolerance": 0.5
    }
    
    out_dir = "tests/fixtures"
    os.makedirs(out_dir, exist_ok=True)
    with open(os.path.join(out_dir, "lufs_reference.json"), "w") as f:
        json.dump(lufs_data, f, indent=2)

if __name__ == "__main__":
    main()

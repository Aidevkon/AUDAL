import os
import hashlib

corpus_dir = "/home/aidevcon/Documents/creator-os/research/w-speech/corpus"
readme_path = "/home/aidevcon/Documents/creator-os/research/w-speech/README.md"

files = sorted([f for f in os.listdir(corpus_dir) if f.endswith('.flac')])
hashes = []
for f in files:
    h = hashlib.sha256()
    with open(os.path.join(corpus_dir, f), 'rb') as fp:
        h.update(fp.read())
    hashes.append(f"{h.hexdigest()}  {f}")

readme_content = """# W_speech Workshop

## Corpus v1: LibriSpeech (dev-clean)

**Source:** http://www.openslr.org/resources/12/dev-clean.tar.gz
**License:** CC BY 4.0 (Commercial-compatible with attribution)
**Attribution Requirement:** The product NOTICE must credit LibriSpeech (Panayotov et al.) per CC BY.

**Note:** The previous `voxserv` audio quality testing samples are legally unclear and remain restricted. They stay legitimate ONLY as TEST input (ACX/VAD harnesses — data not baked into the product); the training line is what it cannot cross.

### Resampling & HF Implication
**Resampling Method:** Rust `rubato` FastFixedIn (or equivalent high-quality sync interpolator) from 16kHz to 48kHz.
**LOUD FLAG ON HF IMPLICATION:** LibriSpeech is native 16kHz. This means absolutely nothing above 8kHz exists in the source audio! The 4-10kHz consonant band is only HALF covered. The resulting templates will have ZERO energy in the 8kHz-24kHz bins. This is a measured-world fact for v2 planning. We proceed anyway for v1.

### Fit Protocol (fit_protocol_v1)
- 128 mel bands (triangle)
- ALL frames
- 12 iterations
- Seed: 314159
- tau: 8
- K: 4

### File Hashes
```text
""" + "\n".join(hashes) + "\n```\n"

with open(readme_path, 'w') as f:
    f.write(readme_content)
print("README generated")

# Silero VAD Lab

This directory is an isolated Python lab used to run the Silero VAD teacher model over our fixtures to generate p-maps. 

## Doctrine
Silero is a LAB INSTRUMENT. Its outputs (per-frame probability maps) are saved as sidecar `.pmap.json` files and may be consumed by our Rust pipeline for verification/evaluation. The ONNX runtime and PyTorch dependencies NEVER enter any product crate.

## Environment & Version
- Route: PyPI via `pip install silero-vad`
- Version pinned: 6.2.1
- Model: Built-in ONNX weights distributed with silero-vad pip package.

## License
Silero VAD is distributed under the MIT License:

```
MIT License

Copyright (c) 2020-present Silero Team

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
```

## Installation Recipe
To avoid downloading large GPU binaries for inference, the environment was explicitly constructed with CPU-only wheels:
```bash
python3 -m venv venv
./venv/bin/pip install torch torchaudio --index-url https://download.pytorch.org/whl/cpu
./venv/bin/pip install silero-vad soundfile
```
Versions installed during validation:
- `torch==2.13.0+cpu`
- `torchaudio==2.11.0+cpu`
- `silero-vad==6.2.1`
- `soundfile==0.14.0`

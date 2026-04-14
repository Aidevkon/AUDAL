#!/usr/bin/env python3
"""
Verify all assets in registry/checksums.json against actual files.
Run from: lineos/m0/ directory (module Justfile context)
Used by: just m0::verify-assets
Authority: M0 Constitution v2.0 §03.1
"""
import hashlib
import json
import os
import sys

checksums = json.load(open('registry/checksums.json'))
errors = 0

for path, meta in checksums.get('assets', {}).items():
    # WASM assets are stored in assets/wasm/
    full_path = os.path.join('assets', 'wasm', path)
    if not os.path.exists(full_path):
        print(f'  ❌ MISSING: {path}')
        errors += 1
        continue
    with open(full_path, 'rb') as f:
        actual = hashlib.sha256(f.read()).hexdigest()
    expected = meta.get('sha256', '')
    if actual == expected:
        print(f'  ✅ {path}')
    else:
        print(f'  ❌ HASH MISMATCH: {path}')
        print(f'     expected: {expected}')
        print(f'     actual:   {actual}')
        errors += 1

if errors:
    print(f'\n❌ {errors} asset(s) failed verification')
    sys.exit(1)
else:
    print('\n✅ All assets verified')

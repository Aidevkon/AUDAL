#!/usr/bin/env python3
"""
TOOL: verify_cert.py
PURPOSE: Third-party verification of the byte-detached Ed25519
         `payload_signature` embedded in an m0d certificate sidecar
         (v0 §6, Σ1α). The signature covers the FULL serialized sidecar
         bytes with the `payload_signature` VALUE zeroed to "" — it is
         checked here by zeroing that value back out of the bytes on
         disk, never by re-serializing the JSON (re-serializing could
         reorder or reformat bytes the signature no longer matches).
INSTALL: pip install pynacl
USAGE:   verify_cert.py <sidecar.json>
EXIT:    0 = signature verifies. 1 = anything else (missing/duplicate
         anchor, unsigned cert, bad key/signature encoding, mismatch).
"""
import base64
import json
import re
import sys

ANCHOR = re.compile(rb'("payload_signature"\s*:\s*")([A-Za-z0-9_-]*)(")')


def b64url_nopad_decode(s: str) -> bytes:
    pad = "=" * (-len(s) % 4)
    return base64.urlsafe_b64decode(s + pad)


def main() -> int:
    if len(sys.argv) != 2:
        print(f"usage: {sys.argv[0]} <sidecar.json>", file=sys.stderr)
        return 2
    path = sys.argv[1]

    with open(path, "rb") as f:
        raw = f.read()

    matches = list(ANCHOR.finditer(raw))
    if len(matches) != 1:
        print(
            f"FAIL: \"payload_signature\" anchor found {len(matches)} times "
            f"in {path} (expected exactly 1) — refusing to verify blind"
        )
        return 1

    m = matches[0]
    sig_b64 = m.group(2).decode("ascii")
    if sig_b64 == "":
        print(f"FAIL: {path} has an empty payload_signature — pre-Sigma1a cert, not signed")
        return 1

    # Reconstruct the EXACT bytes that were signed: same anchor, value
    # zeroed back out — a byte slice, not a re-serialize.
    unsigned = raw[: m.start(2)] + raw[m.end(2) :]

    try:
        envelope = json.loads(raw)
    except json.JSONDecodeError as e:
        print(f"FAIL: {path} is not valid JSON: {e}")
        return 1

    signer_hex = envelope.get("signer_public_key", "")
    if len(signer_hex) != 64:
        print(
            f"FAIL: signer_public_key in {path} is not 64 hex chars "
            f"(got {len(signer_hex)!r})"
        )
        return 1

    try:
        pubkey_bytes = bytes.fromhex(signer_hex)
        sig_bytes = b64url_nopad_decode(sig_b64)
    except ValueError as e:
        print(f"FAIL: could not decode key/signature in {path}: {e}")
        return 1

    try:
        from nacl.exceptions import BadSignatureError
        from nacl.signing import VerifyKey
    except ImportError:
        print("FAIL: pynacl not installed — pip install pynacl")
        return 1

    verify_key = VerifyKey(pubkey_bytes)
    try:
        verify_key.verify(unsigned, sig_bytes)
    except BadSignatureError:
        print(
            f"FAIL: payload_signature does NOT verify for {path} "
            f"(key_id={envelope.get('key_id', '?')})"
        )
        return 1

    print(f"OK: payload_signature verifies for {path} (key_id={envelope.get('key_id', '?')})")
    return 0


if __name__ == "__main__":
    sys.exit(main())

import json
import uuid

session_id = str(uuid.uuid4())

states_sequence = [
    "silence", "silence", "breath", "consonant", "vowel", "vowel", "vowel", "tail", "silence"
] * 10

events = []
start_ms = 0
for state in states_sequence:
    events.append({
        "state": state,
        "start_ms": start_ms,
        "end_ms": start_ms + 100,
        "duration_ms": 100,
        "confidence": 1.0,
        "session_id": session_id,
        "attributes": {
            "rms_db": -20.0,
            "crest_factor_db": 10.0,
            "density": 0.0,
            "spectral_centroid": 0.0,
            "lufs_integrated": -14.0,
            "spectral_flatness": 0.0
        },
        "risk": {
            "artifact_risk": 0.0,
            "sibilance_risk": 0.0,
            "phase_issue": 0.0,
            "sub_rumble": 0.0
        },
        "domain": {
            "stem": "voice",
            "profile_hint": "synthetic"
        }
    })
    start_ms += 100

envelope = {
    "protocol_version": "900",
    "session_id": session_id,
    "stems": [
        {
            "stem_type": "voice",
            "events": events
        }
    ]
}

with open(f"session_{session_id[:8]}.corpus.json", "w") as f:
    json.dump(envelope, f, indent=2)

print("Generated synthetic corpus.")

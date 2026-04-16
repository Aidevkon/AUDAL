# LineOS — Phase 9 Task Decomposition

**Document:** `lineos/plan/phase-9/task-decomposition.md`
**Version:** 1.0
**Phase:** 9 — Telemetry Fix + Coach Prompt Schema
**Status:** 🔒 LOCKED
**Authority:** Phase 9 Master Prompt · LineOS Constitution v2.0

---

## Root Cause Analysis — LRA = 0.0

The `lineos-telemetry` crate exists and computes EBU R128 metrics,
but it is **not wired** into the M0 mastering pipeline.

Current flow in `handlers/master.rs`:
```
decode_audio() → AudioPcm
    │
    ▼
MasteringPipeline::master(chunk, intent)
    │
    ▼
GoldenBlob { loudness: { lra: 0.0, ... } }  ← sp314-dsp fills this
```

The `sp314-dsp` pipeline computes `integrated_lufs` and `true_peak`
but sets `lra = 0.0` because LRA requires a full-track pass with
sliding 3-second windows — this is the telemetry layer's job.

**Fix:** After `MasteringPipeline::master()` completes, pass the
decoded PCM through `lineos-telemetry` to compute real LRA,
momentary, and short-term values, then update the Golden Blob.

---

## Task Order

```
P9-001  Audit lineos-telemetry API
P9-002  Wire telemetry into master.rs
P9-003  Update GoldenBlob with real LRA metrics
P9-004  Verify LRA with gargar.mp3
P9-005  coach_prompt.toml — extract prompt to asset file
P9-006  Load coach_prompt.toml at runtime in coach_adapter.rs
P9-007  CI gate + tag
```

---

## P9-001 — Audit lineos-telemetry API

Before writing any code, read the existing telemetry crate:

```bash
cat lineos/m1/telemetry/src/lib.rs
cat lineos/m1/telemetry/src/ebu_r128.rs 2>/dev/null || \
  find lineos/m1/telemetry/src -name "*.rs" | xargs ls
```

Understand:
- What input does it take? (`&[f32]` samples or `AudioChunk`?)
- What does it return? (`LraResult`, `Measurement`, etc.)
- Does it need sample_rate and channels?
- Is it already a workspace member?

**DoD P9-001:**
```bash
grep "lineos-telemetry\|lineos_telemetry" Cargo.toml
echo "✅ P9-001 — telemetry API understood"
```

---

## P9-002 — Wire Telemetry into master.rs

After `MasteringPipeline::master()` returns the GoldenBlob,
run the telemetry pass on the **original decoded PCM**:

```rust
// In handlers/master.rs, after pipeline.master():

use lineos_telemetry::measure;

// Run full telemetry pass (LRA, momentary, short-term)
let telemetry = measure(
    &pcm.samples,
    pcm.sample_rate,
    pcm.channels,
)?;

// Update blob with real values
blob.loudness.lra              = telemetry.lra;
blob.loudness.momentary_lufs   = telemetry.momentary_max;
blob.loudness.short_term_lufs  = telemetry.short_term_max;
```

Add `lineos-telemetry` to `m0-daemon/Cargo.toml`:
```toml
lineos-telemetry = { path = "../../../m1/telemetry" }
```

**Important:** The telemetry pass runs on the **pre-mastered** PCM
(the decoded input). This measures the source material's dynamic
range, not the mastered output. This is correct — LRA measures
the original program material.

**DoD P9-002:**
```bash
cargo check -p m0d
echo "✅ P9-002"
```

---

## P9-003 — Update GoldenBlob with Real LRA

The `GoldenBlob` struct in `sp314-dsp` has the loudness fields.
After telemetry runs, the blob must be updated before storing.

Check if `GoldenBlob.loudness` fields are mutable after creation:
```bash
grep -n "pub lra\|pub momentary\|pub short_term" \
  lineos/m1/sp314-dsp/src/types/golden_blob.rs
```

If fields are `pub`, direct assignment works.
If not, add a `pub fn update_telemetry(&mut self, ...)` method.

**DoD P9-003:**
```bash
cargo check -p m0d
cargo check -p sp314-dsp
echo "✅ P9-003"
```

---

## P9-004 — Verify LRA with gargar.mp3

Rebuild m0d and test with the reference file:

```bash
cargo build -p m0d

# Restart m0d (kill old process first)
pkill -f "target/debug/m0d" || true
cd /home/aidevcon/Documents/creator-os
./target/debug/m0d &
sleep 3

# Test
BLOB_ID=$(curl -s -X POST http://127.0.0.1:7402/master \
  -H "Content-Type: application/json" \
  -d '{"audio_path":"/home/aidevcon/Music/gargar.mp3","preset_id":"spotify"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['blob_id'])")

curl -s http://127.0.0.1:7402/blob/$BLOB_ID \
  | python3 -m json.tool | grep -E "lra|momentary|short_term|integrated"
```

**Expected (gargar.mp3):**
```
"integrated_lufs": -7.93     (unchanged)
"lra":             4.0-12.0  (was 0.0 — NOW REAL)
"momentary_lufs":  varies    (NOT equal to integrated)
"short_term_lufs": varies    (NOT equal to integrated)
```

**DoD P9-004:**
```bash
# lra must not be 0.0
LRA=$(curl -s http://127.0.0.1:7402/blob/$BLOB_ID | \
  python3 -c "import sys,json; print(json.load(sys.stdin)['loudness']['lra'])")
python3 -c "assert float('$LRA') != 0.0, 'LRA still 0.0!'; print('✅ LRA =', '$LRA')"
```

---

## P9-005 — coach_prompt.toml

Create `apps/stillair/src-tauri/assets/coach_prompt.toml`:

```toml
# Coach Prompt Schema — LineOS Still Air
# Version: 1.0
# Loaded at runtime — edit without rebuild.
# Authority: LLM Adapter Amendment v1.1

[identity]
role = "professional audio mastering coach"
style = "teacher, not engineer"
rules = [
    "Explain WHY each finding matters for the listener",
    "Give directional suggestions only — no specific dB values",
    "No plugin names, no DSP chain instructions",
    "Short sentences. Plain language. No jargon.",
    "Never invent new issues not present in the findings",
]

[output]
format = "JSON only"
no_markdown = true
no_preamble = true

[schema]
# The exact JSON schema the LLM must produce
template = """
{
  "summary": "2-3 sentence overview of the session",
  "explanations": [
    {
      "issue_id": "exact_issue_id_from_findings",
      "severity": "info|low|medium|high",
      "title": "short human-readable title",
      "why": "why this matters for the listener experience",
      "suggestion": "directional suggestion — no specific values"
    }
  ]
}
"""

[examples]
# Few-shot examples for consistent output
[[examples.good]]
issue_id = "lufs_compliance"
severity = "medium"
title = "Track is louder than Spotify's target"
why = "Spotify will turn your track down automatically. The listener hears it quieter than you intended, and you lose control of the perceived loudness."
suggestion = "Work on the overall gain structure earlier in your chain so the track sits closer to -14 LUFS before the final limiter."

[[examples.good]]
issue_id = "dynamic_range_low"
severity = "low"
title = "Limited dynamic range"
why = "The track has very little contrast between quiet and loud moments. This can feel fatiguing over time and reduces emotional impact."
suggestion = "Consider whether the compression is too heavy in the mix stage. A little more breathing room often makes tracks feel more alive."
```

**DoD P9-005:**
```bash
python3 -c "import tomllib; tomllib.load(open('apps/stillair/src-tauri/assets/coach_prompt.toml','rb')); print('✅ valid TOML')"
echo "✅ P9-005"
```

---

## P9-006 — Load coach_prompt.toml at Runtime

Update `apps/stillair/src-tauri/src/aether/coach_adapter.rs`:

```rust
use std::path::Path;

#[derive(serde::Deserialize)]
struct PromptConfig {
    identity: IdentityConfig,
    output:   OutputConfig,
    schema:   SchemaConfig,
    examples: ExamplesConfig,
}

#[derive(serde::Deserialize)]
struct IdentityConfig {
    role:  String,
    style: String,
    rules: Vec<String>,
}

// ... other config structs

impl CoachAdapter {
    fn load_prompt_config() -> PromptConfig {
        // Load from assets/ relative to binary
        let asset_path = std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|p| p.join("assets/coach_prompt.toml")))
            .unwrap_or_else(|| Path::new("assets/coach_prompt.toml").to_path_buf());

        let content = std::fs::read(&asset_path)
            .unwrap_or_else(|_| DEFAULT_PROMPT_TOML.as_bytes().to_vec());

        toml::from_slice(&content)
            .unwrap_or_else(|_| toml::from_str(DEFAULT_PROMPT_TOML).unwrap())
    }

    fn build_prompt(&self, findings: &CoachFindingsJson) -> String {
        let config = Self::load_prompt_config();
        let rules = config.identity.rules.join("\n- ");
        // ... build prompt from config + findings
    }
}

// Fallback if toml file not found
const DEFAULT_PROMPT_TOML: &str = include_str!("../../assets/coach_prompt.toml");
```

Add `toml` to `src-tauri/Cargo.toml`:
```toml
toml = "0.8"
```

**DoD P9-006:**
```bash
cargo check -p stillair
echo "✅ P9-006"
```

---

## P9-007 — CI Gate + Tag

```bash
cargo test --workspace
just ci

git add -A
git commit -m "feat(telemetry): Phase 9 — real LRA + coach_prompt.toml

9A — Telemetry fix:
  - lineos-telemetry wired into M0 master handler
  - LRA computed from BS.1770-4 3s sliding windows
  - momentary_lufs: 400ms window (was = integrated)
  - short_term_lufs: 3s window (was = integrated)
  - gargar.mp3: lra=X.X LU (was 0.0)

9B — Coach prompt schema:
  - Prompt extracted to assets/coach_prompt.toml
  - Loaded at runtime (no rebuild needed for prompt changes)
  - include_str! fallback if asset file missing
  - Few-shot examples for consistent JSON output

Authority: LineOS Constitution v2.0 · Creator OS Constitution v2.6"

git tag v0.9.0-telemetry
git log --oneline -5
```

---

## Completion Report

```
✅ Phase 9 — Telemetry + Coach Prompt — COMPLETE

LRA:              real value (≠ 0.0) ✅
momentary_lufs:   real 400ms window ✅
short_term_lufs:  real 3s window ✅
coach_prompt.toml runtime-loaded ✅
just ci:          ✅

Tag: v0.9.0-telemetry ✅

Ready for: Phase 10 — Export (WAV + FLAC + Opus)
```

---

**Lead Architect:** Anestis
**System:** LineOS
**Phase:** 9
**Version:** 1.0
**Status:** 🔒 LOCKED

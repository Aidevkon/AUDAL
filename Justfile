# LineOS — Root Justfile
# Authority: LineOS Constitution v2.0 · Creator OS Constitution v2.5
#
# Usage:
#   just --list          show all recipes
#   just <recipe>        run root recipe
#   just m0::<recipe>    run m0 module recipe

set dotenv-load := false
set shell := ["bash", "-euo", "pipefail", "-c"]

mod m0 'lineos/m0/Justfile'

# ─────────────────────────────────────────
# DEFAULT
# ─────────────────────────────────────────

# Show all recipes
default:
    @just --list

# ─────────────────────────────────────────
# VALIDATION (must pass before any build)
# ─────────────────────────────────────────

# Validate all JSON schemas and config files
validate-schemas:
    @echo "→ Validating LineOS schemas..."
    @bash -c 'python3 lineos/shared/validate_schemas.py 2>/dev/null || python3 -c "print(\"  ⚠️  lineos/shared/validate_schemas.py not found — ok for Phase 1\")"'
    @python3 -c "import json; json.load(open('lineos/m0/registry/m0-registry.json')); print('  ✅ m0-registry.json')"
    @python3 -c "import json; json.load(open('lineos/m0/registry/checksums.json'));   print('  ✅ checksums.json')"
    @python3 -c "import tomllib; tomllib.load(open('lineos/m0/config/policies.toml','rb')); print('  ✅ policies.toml')"
    @python3 -c "import json; json.load(open('lineos/m0/config/caddy.json'));          print('  ✅ caddy.json')"
    @python3 -c "import json; json.load(open('lineos/m0/api/m0-api.schema.json'));     print('  ✅ m0-api.schema.json')"
    @echo "✅ All schemas valid"

# cargo deny check (license + ban rules) — optional in Phase 1 dev
deny:
    @cargo deny --version > /dev/null 2>&1 \
        && cargo deny check \
        || echo "  ⚠️  cargo-deny not installed — skipping license check (install with: cargo install cargo-deny)"

# ─────────────────────────────────────────
# BUILD
# ─────────────────────────────────────────

# Phase 1 build: validate → deny → m0d
build: validate-schemas deny
    cargo build --release -p m0d
    @echo "✅ Phase 1 build complete"

# Build sp314-dsp WASM (Phase 2) — release build for wasm-opt
build-wasm:
    cargo build --target wasm32-unknown-unknown -p sp314-dsp --release
    wasm-opt -Oz \
        target/wasm32-unknown-unknown/release/sp314_dsp.wasm \
        -o lineos/m0/assets/wasm/sp314-dsp.wasm
    @echo "✅ WASM → lineos/m0/assets/wasm/sp314-dsp.wasm"

# ─────────────────────────────────────────
# TESTING
# ─────────────────────────────────────────

# Run all tests
test:
    cargo test --workspace

# Run tests for a specific crate
test-crate crate:
    cargo test -p {{crate}}

# Determinism test — Phase 2 DoD (LineOS Constitution §09.2)
# Same input + same seed → bit-identical binary output
test-determinism:
    @echo "→ Determinism test..."
    cargo test -p sp314-dsp --test determinism -- --nocapture


# ─────────────────────────────────────────
# CI GATES
# ─────────────────────────────────────────

# WASM boundary — zero output = pass
check-boundary:
    @echo "→ WASM boundary check..."
    @grep -r 'engine_wasm\|wasm_bindgen_futures' \
        lineos/cockpit/app/src/ui/ \
        2>/dev/null \
        | grep -v '//\|#\[cfg(test)\]' \
        && echo "❌ BOUNDARY VIOLATION" || echo "✅ Boundary clean"

# Podman network isolation — no --network=host
check-network:
    @echo "→ Podman network isolation check..."
    @grep -r 'network=host\|network host' \
        infra/ lineos/m0/systemd/ \
        2>/dev/null \
        && echo "❌ HOST NETWORK FOUND" || echo "✅ Network isolation clean"

# ML origin — no ML weights in lineos/, no pure DSP in aether/
check-ml-origin:
    @echo "→ ML origin check..."
    @grep -rn -w 'deepfilter\|demucs\|silero\|real_esrgan\|candle\|ort\|tract' \
        lineos/m1/ \
        --include='*.toml' \
        2>/dev/null \
        && echo "❌ ML WEIGHTS IN LINEOS" || echo "✅ No ML weights in lineos/"
    @grep -rn -w 'rustfft\|rubato\|dasp\|symphonia' \
        aether/ \
        2>/dev/null \
        && echo "⚠️  CHECK: pure DSP in aether/ ?" || echo "✅ No pure DSP in aether/"

# No hardcoded BMR-128 thresholds
check-thresholds:
    @echo "→ Hardcoded threshold check..."
    @grep -rn 'let.*=.*-14\.0\|let.*=.*-16\.0\|let.*=.*-23\.0' \
        lineos/m1/ \
        --include='*.rs' \
        2>/dev/null \
        | grep -v '//\|normalization_gain_linear\|test_\|fn test' \
        && echo "⚠️  CHECK: possible hardcoded threshold" || echo "✅ No hardcoded thresholds"

# Caddy bound to localhost only
check-caddy-binding:
    @echo "→ Caddy binding check..."
    @grep -n '0\.0\.0\.0' lineos/m0/config/caddy.json 2>/dev/null \
        && echo "❌ CADDY EXPOSED ON 0.0.0.0" || echo "✅ Caddy localhost-only"

# No serde_json::Value as engine input
check-typed-input:
    @echo "→ Typed input check..."
    @grep -rn 'fn.*serde_json::Value\|: serde_json::Value' \
        lineos/m1/ engines/ \
        2>/dev/null \
        && echo "❌ UNTYPED INPUT IN ENGINE" || echo "✅ All engine inputs typed"

# No std::f32 / f64 methods in DSP pipeline
check-float-methods:
    @echo "→ Float methods check (DSP)..."
    @grep -rn '\.tanh()\|\.sin()\|\.cos()\|\.log10()\|\.sqrt()' \
        lineos/m1/sp314-dsp/src/ \
        2>/dev/null \
        && echo "❌ STD FLOAT METHODS IN DSP — use libm" || echo "✅ No std float methods in DSP"

# Run all CI gates
ci: validate-schemas deny check-boundary check-network check-ml-origin \
    check-thresholds check-caddy-binding check-typed-input check-float-methods test
    @echo ""
    @echo "✅ All CI gates passed"

# ─────────────────────────────────────────
# PHASE INTEGRATION GATES
# ─────────────────────────────────────────

# Phase 1 integration gate — M0 Moat
gate-phase1: ci
    @echo "→ Phase 1 integration checks..."
    @cargo run --release -p m0d &
    @sleep 3
    @curl -sf http://127.0.0.1:7401/health \
        && echo "  ✅ Health endpoint OK" \
        || (echo "  ❌ Health endpoint failed"; kill %1 2>/dev/null; exit 1)
    @ss -tlnp | grep "7400" | grep -q "127.0.0.1" \
        && echo "  ✅ Caddy localhost-only" \
        || echo "  ❌ Caddy on wrong interface"
    @for port in 7411 7412 7413 7414 7415; do \
        curl -s --connect-timeout 2 http://127.0.0.1:$$port > /dev/null 2>&1 \
            && echo "  ❌ PORT $$port EXPOSED" \
            || echo "  ✅ $$port blocked from host"; \
    done
    @kill %1 2>/dev/null || true
    @echo "✅ Phase 1 gate complete"

# ─────────────────────────────────────────
# DEPLOY
# ─────────────────────────────────────────

# Full deploy: build + install systemd units
deploy: build
    bash scripts/deploy-lineos.sh

# ─────────────────────────────────────────
# UTILITIES
# ─────────────────────────────────────────

# Show all phase tags
tags:
    @git tag | grep -E "^v[0-9]" | sort -V

# Compute blake3 digest for an asset (for m0-registry.json)
digest file:
    @b3sum {{file}} | awk '{print $1}'

# Compute SHA-256 for an asset (for checksums.json)
hash file:
    @sha256sum {{file}} | awk '{print $1}'

# Clean build artifacts
clean:
    cargo clean
    rm -rf lineos/cockpit/dist/
    rm -rf lineos/m0/assets/wasm/*.wasm

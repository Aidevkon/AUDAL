# Creator OS — CI Quality Gates
# just ci     → all gates
# just quick  → fast iteration (Gates 1+3)

check:
    cargo check --workspace

check-cli:
    cargo check --workspace --features cli

fmt:
    cargo fmt --check

clippy:
    cargo clippy --workspace -- -D warnings -A clippy::needless_range_loop -A clippy::too_many_arguments -A clippy::new_without_default

test-unit:
    cargo test --lib --workspace

test-quality:
    cargo test -p sp314-dsp --test four_stem_contract
    cargo test -p sp314-dsp --test io_contract --features cli
    cargo test -p sp314-dsp --test adm_bwf_integration --features cli
    cargo test -p sp314-dsp --test stress
    cargo test -p m0d --test e2e_golden_pathway
    cargo test -p m0d --test e2e_corpus_integration
    cargo test -p m0d --test e2e_maestro_ducking
    cargo test -p m0d --test e2e_mastering_quality

test-all:
    cargo test --workspace

ci: check check-cli fmt clippy test-unit test-quality test-all
    @echo "✅ All CI gates passed"

quick: check test-unit
    @echo "✅ Quick check passed"

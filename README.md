# CREATOR.OS

Reproducible mastering with a certificate.

Drop a file → get a master that passes a **declared** delivery spec →
with proof that it passes and that it reproduces.

**The honest limit:** reproducible *by you*, with the same engine
version and the same declared build profile. Not third-party
verification — that requires reproducible builds and is not promised
before it is solved.

The build profile is declared, not inherited: `opt-level = 3`,
`codegen-units = 1`, `target-cpu = x86-64` — each one measured against
INV-DET-1 (two full renders, identical output SHA) before it was
pinned. The certificate survives daemon restarts (envelope sidecar
bound to the master's bytes — INV-Π-1 / INV-PERSIST-1 guard it).

**Deliverables (presets, with specs):** audiobook/ACX · podcast ·
music streaming · broadcast · spatial 5.1.

**Where truth lives:**

- `docs/northstar-v2.md` — intent, doctrines, and the map of what
  talks to what. The repo always outranks it.
- `FINDINGS.md` — the findings registry (F-numbers, one allocator).
- `PROJECT_MEMORY_MAP.md` — what already exists, so it is not built
  twice.
- Code, tests, and measurements — the only facts.

**The house rule (doctrine I):** a name is not proof. A field, a
function, a CI job, or a commit message that promises X does not
guarantee X. Before any claim: pull, then grep the living tree.

Workspace: `lineos/m1/sp314-dsp` (DSP core) · `lineos/m0/m0-daemon`
(daemon, package `m0d`) · `creator-os/` (constitution, amendments,
invariants). Status: pre-launch; `schema_version 0` breaks without
migration, declared up front.

Third-party embedded assets: see `THIRD_PARTY_LICENSES.md`.

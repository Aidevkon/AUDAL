# Project Memory Map

**Σκοπός:** Ευρετήριο πραγμάτων που ΗΔΗ υπάρχουν στο codebase αλλά είναι εύκολο να ξεχαστούν/ξανανακαλυφθούν. Δημιουργήθηκε αφού απόψε ξαναβρέθηκε (μετά από ρητή, στενή ερώτηση) ένα ήδη-υπάρχον git-like versioning model (MixCommit/Branch) ενώ δουλεύαμε ακριβώς στο σχετικό A/B feature, χωρίς κανείς να το θυμάται.

## 1. Live vs Deprecated engines

- **`Sp314MasteringEngine`** (lineos/m1/sp314-dsp/src/pipeline/engine.rs) — ρητά #[deprecated], σχόλιο: "This monolith is Phase 6 legacy". Καλείται ΜΟΝΟ από CLI tools (test_engine, sp314_master, sp314_live) και tests (realtime_contract.rs). ΔΕΝ είναι στο production HTTP path.
- **Live production path:** DspGraph (sp314-nodes) μέσω DspAdapter, και η νέα streaming_pipeline.rs (m0-daemon/src/dsp/). Αυτό είναι το source of truth για νέα features.
- two_pass.rs έχει ένα "legacy 1024 zero padding" comment — pure compatibility code, όχι deprecated engine, μόνο σημείωση.

## 2. Duplicated primitives across crates

| Primitive | sp314-dsp location | xaak location | Γιατί duplicated |
|---|---|---|---|
| CrossoverLR4 | compressor/crossover.rs | xaak/src/crossover.rs | xaak δεν έχει dependency στο sp314-dsp by design |
| (M/S logic) | spatial/mid_side.rs | telemetry_worker.rs (inline) | ίδιος λόγος |

Source of truth: sp314-dsp's version (πιο ολοκληρωμένο, πρωτότυπο). Αν διορθωθεί bug εκεί, ΔΕΝ μεταφέρεται αυτόματα στο xaak copy — χρειάζεται χειροκίνητος συγχρονισμός, κανείς δεν θα το θυμηθεί αυτόματα.

## 3. Schema structs: wired vs unwired (db/schema.rs, 11 structs συνολικά)

| Struct | Status |
|---|---|
| Project | ✅ WIRED |
| Track | ✅ WIRED |
| Branch | ✅ WIRED |
| Session | ❌ UNWIRED |
| FindingPatchJson | ❌ UNWIRED |
| MasteringParamsJson | ❌ UNWIRED |
| DspStateJson | ❌ UNWIRED |
| **MixCommit** | ❌ UNWIRED (git-like versioning model, ready for A/B/C/D feature) |
| UserProfile | ❌ UNWIRED |
| UserFindingFeedback | ❌ UNWIRED (graph RELATION edge, ready to use) |
| UserFlavourPreference | ❌ UNWIRED (graph RELATION edge, ready to use) |

**8 από τα 11 structs (73%) είναι ήδη σχεδιασμένα στο schema αλλά δεν έχουν κανέναν production consumer ακόμα.** Πριν σχεδιάσεις νέο data model για κάτι, έλεγξε εδώ πρώτα — μπορεί να υπάρχει ήδη.

## 4. CI feature-flag visibility gaps (επιβεβαιωμένα αρχεία)

- io_contract.rs, realtime_contract.rs (sp314-dsp/tests/) — και τα δύο ήδη βρέθηκαν/διορθώθηκαν σήμερα/χθες (#18-20, commit 8339704).
- telemetry_worker.rs (xaak) — feature-gated, ΔΕΝ έχει ακόμα επιβεβαιωθεί αν τρέχει σε routine CI ή όχι· αξίζει έλεγχος.

## How to use this doc
Πριν ξεκινήσεις νέο feature, ψάξε εδώ πρώτα: μπορεί να υπάρχει ήδη σχετικό schema/struct/primitive. Ενημέρωσε αυτό το doc όποτε βρίσκεις κάτι αντίστοιχο.

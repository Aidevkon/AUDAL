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

## 5. Unwired analysis modules (γραμμένα, δοκιμασμένα, χωρίς production caller)

| Module | Τι είναι | Γιατί unwired | Επιβεβαιώθηκε |
|---|---|---|---|
| `m0-daemon/src/classification.rs` (guess_content_type) | Speech/Music μαντευτής από φθηνά σήματα (crest, LRA, spectral shape), 3 ψήφοι + confidence | Φτιάχτηκε για μελλοντικό genre classifier — δεν εξυπηρέτησε τον αρχικό σκοπό, κρατιέται για μετά. PROVISIONAL thresholds (μηδέν labeled fixtures) | recon 2026-07-30, μηδέν εξωτερικοί callers |
| VAD stack: `sp314-dsp/analysis/vad_sensors.rs` + vad_features + vad_model | Ο 5-Sensor Core του Y6 blueprint (RMS delta, transient, flatness, M/S, MFCC) + Bayesian classifier | Observe-only: τρέχει ΜΟΝΟ με vad_observe_enabled (default false), γράφει CSV traces. Παγωμένο ενδιάμεσο του Y6/Y7 — ΜΗΝ το αγγίξεις εκτός Y-axis δουλειάς | recon 2026-07-30 |

Πριν φτιάξεις νέο speech/silence/genre detector, κοίτα εδώ πρώτα.

## 6. Placeholder τιμές που παρουσιάζονται ως μετρήσεις (certificate structs)

Οικογένεια «ψεύτικων νούμερων» σε structs που καταλήγουν σε πιστοποιητικά — ίδια κατηγορία με το R-002 (certificate LUFS was pre-gain):

- `certificate_node.rs::assemble_blob` → StoredQuality: `rms_db: lufs + 3.0` (hardcoded offset, ΟΧΙ το πραγματικό RMS που πλέον μετράει το trunk), `phase_coherence: 0.97`, `spectral_centroid: 3_200.0`, `spectral_flatness: 0.12`, `stereo_width: 0.5` — όλα σταθερές. Επίσης `dr = 10.0` με TODO "wire real DR when this path carries trunk metrics".
- `album_certificate_node.rs`: `created_at: "now"`, `seed: 0`, `pipeline_version: "v1"` — placeholder μεταδεδομένα.

Το trunk ΗΔΗ μετράει rms_db και dynamic_range_db — η σύνδεση είναι εκκρεμής, όχι αδύνατη. ΜΗΝ εμπιστεύεσαι StoredQuality νούμερα μέχρι να γίνει.

## 7. Εκκρεμείς μικρο-συνδέσεις (data ready, rendering/consumer pending)

- PDF: τα 4 acx_* πεδία ζουν ήδη στο blob.loudness (2ccff74) — το pdf_gen.rs διαβάζει blob.loudness.* αλλά δεν τα τυπώνει ακόμα. Όταν σχεδιαστεί το ACX section του PDF, τα δεδομένα είναι μία γραμμή μακριά.
- `ContentTypeExt::lufs_target()`: νεκρό (μηδέν callers) ΚΑΙ λάθος (platform από content kind) — burial εκκρεμεί, αγγίζει trait signature.
- `from_preset` doc comment λέει "Mirrors ContentType" — stale μετά το catalogue (d01a323).
- ProfileId::MusicIdm λείπει από το enum ενώ music-idm-v1.json υπάρχει — unwired profile.

## 8. Διορθώσεις σε παλιές περιγραφές (recon 2026-07-30)

- Ο "D2 noise-floor estimator" ΔΕΝ είναι "spectral minimum tracking" όπως γράφτηκε στο Y-plan (εκτός repo): είναι time-domain, ελάχιστο RMS σε 1s παράθυρα πάνω από -60 dB dead-air gate, content-unaware (μπορεί να διαβάσει απαλή λέξη αντί για room tone). Το σωστό ACX noise floor είναι ο AcxCheckAnalyzer (quietest sliding 500ms, validated).
- Το `full_pipeline_heap` test ΔΕΝ είναι #[ignore]d — το ef4472d το un-ignore-αρε σκόπιμα (streaming decode προσγειώθηκε) αλλά το σχόλιο έμεινε stale ΚΑΙ η dhat προειδοποίηση μέσα στο ignore message χάθηκε μαζί του → η race που κλείσαμε στο c39cc57. Μάθημα: γνώση που ζει μόνο μέσα σε attribute πεθαίνει μαζί του.
- export.rs resample loop (export_mp3_acx): per-chunk truncate σε round(chunk×ratio) — πιθανή απώλεια ±1 frame/chunk (~0.7ms/12s), συμμετρική με ingest αν κάνει το ίδιο. Αόρατο στο 100ms tolerance, σημείωση μόνο.
- Το γενικό export_mp3 ΔΕΝ έχει κανένα test που να ανοίγει το αρχείο του (recon 2026-07-30) — τι πραγματικά βγάζει (bitrate mode, rate) παραμένει αμέτρητο. Υπόθεση: VBR-ish/48k. Αν ποτέ γίνει user-facing υπόσχεση, μέτρα πρώτα.

## Κανόνες εργασίας με agents (P44+)

- P44 (μετά από 2 παραβιάσεις): agent προτείνει mutations, άνθρωπος εκτελεί. Εξαίρεση: ρητό γραπτό write contract με ονομαστική λίστα αρχείων, verify steps, και STOP πριν από commit — δούλεψε καθαρά 3 φορές (Default::default cleanup, acx_nf_oracle promotion, e2e_acx_certificate).
- Agent recon χωρίς το chat context ΔΕΝ ξέρει τι έγινε στη σημερινή συνεδρία — δώσε του "state of the repo" section στο prompt, αλλιώς θα προτείνει δουλειά που έγινε (συνέβη: πρότεινε το ACX wiring μία ώρα αφού είχε γίνει merge).
- Recon prompts: μικρά single-line anchors σε python patches, ΟΧΙ πολυγραμμικά string blocks — τρία whitespace ατυχήματα σε μία βραδιά (αόρατα \n\n).

## How to use this doc
Πριν ξεκινήσεις νέο feature, ψάξε εδώ πρώτα: μπορεί να υπάρχει ήδη σχετικό schema/struct/primitive. Ενημέρωσε αυτό το doc όποτε βρίσκεις κάτι αντίστοιχο.

# ORPHANS.md — Μητρώο Ορφανού Κώδικα

Durable καταγραφή κόμβων/κλάσεων/artifacts που **υπάρχουν στο δέντρο,
μεταγλωττίζονται, αλλά καμία ζωντανή διαδρομή δεν τα χρησιμοποιεί** —
registered στο factory, wired πουθενά. Το ίδιο πρόβλημα με το FINDINGS.md
πριν το allocator-lint.sh: η αρίθμηση των ορφανών γινόταν σε πρόζα, μέσα
σε ευρήματα F-NNN («δωδέκατο», «δέκατο έκτο ορφανό»), χωρίς δικό της
μητρώο — δύο σειρές, ίδια μέρα, ασύγκριτες, δεν γρεπίζονταν ποτέ.

**Σύμβαση:** κάθε ορφανό παίρνει `[O-NNN]` στον τίτλο της εγγραφής του.
Παραπομπές αλλού (σε FINDINGS.md, σε σχόλια κώδικα) γράφουν το γυμνό
`O-NNN`, χωρίς αγκύλες — ίδιος διαχωρισμός τίτλου/παραπομπής με το
FINDINGS.md allocator, επαληθευμένος εκεί πριν αντιγραφεί εδώ.

**ΟΡΦΑΝ-ΑΛΛΟΚΑΤΟΡ:** αυτή η γραμμή είναι ο ΜΟΝΟΣ allocator. Νέο ορφανό =
αύξηση αυτής της γραμμής ΣΤΟ ΙΔΙΟ commit που το καταγράφει.

ORPHAN-ALLOCATOR: O-017

**Ο φρουρός** (`scripts/orphan-lint.sh`, καλείται από `scripts/guards.sh`)
ελέγχει ΜΟΝΟ:
1. ΔΙΠΛΑ = 0 — ο ίδιος αριθμός τίτλου δεν εμφανίζεται δύο φορές.
2. ALLOCATOR == ΜΕΓΑΛΥΤΕΡΟΣ ΠΙΑΣΜΕΝΟΣ + 1.

⚠ **ΔΕΝ είναι φρουρός αριθμού ορφανών.** Το πλήθος επιτρέπεται —
αναμένεται — να ανεβαίνει. Κάθε αύξηση σημαίνει ότι βρέθηκε κι άλλο, όχι
ότι κάτι χάλασε.

**Έναρξη μητρώου (2026-09-17):** ΜΟΝΟ ορφανά με F-αριθμό εύρεσης και
τεκμήριο μέχρι σήμερα. Καμία αναδρομική απογραφή του υπόλοιπου δέντρου —
δικό της βήμα, ξεχωριστό task.

---

## Μορφή εγγραφής

```
### [O-NNN] — σύντομος τίτλος
- **Τι είναι:** περιγραφή
- **Πού ζει:** path:γραμμή (ή paths, αν παραπάνω από ένα)
- **Βρέθηκε από:** F-NNN (ή «ΧΩΡΙΣ ΑΡΙΘΜΟ ΕΥΡΗΜΑΤΟΣ» αν δεν υπάρχει)
- **Κατάσταση:** ΟΡΦΑΝΟ | ΣΥΝΔΕΘΗΚΕ (commit hash, ημερομηνία)
```

---

### [O-001] — MultibandCompressorNode + LimiterNode
- **Τι είναι:** Δύο κόμβοι `DspNode` καταχωρημένοι στο factory
  (`MultibandCompressorNode`, και `LimiterNode` με registry key `"Limiter"`).
  Καμία topology στο δέντρο δεν τα χρησιμοποιεί.
- **Πού ζει:** `lineos/m1/sp314-nodes/src/nodes/multiband.rs` ·
  `lineos/m1/sp314-nodes/src/nodes/limiter.rs` ·
  registration: `lineos/m1/sp314-nodes/src/graph.rs`
- **Βρέθηκε από:** F-086
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-002] — BrickwallLimiter
- **Τι είναι:** Ο μόνος κόμβος του δέντρου που δηλώνει μη-μηδενικό
  `declared_latency_samples` (240 @48k). Δεν τρέχει ΚΑΘΟΛΟΥ στη ζωντανή
  streaming διαδρομή — ζει μόνο στο ορφανό `/master` + `episode_render`.
- **Πού ζει:** `lineos/m1/sp314-dsp/src/limiter/core.rs`
- **Βρέθηκε από:** F-084
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-003] — duck_gain (control_bus)
- **Τι είναι:** Το ducking υπολογίζεται πλήρως (ballistics, control
  signal) αλλά πετιέται πριν εφαρμοστεί στο σήμα· τα tests μετρούν το
  σήμα ελέγχου, όχι το αποτέλεσμα.
- **Πού ζει:** `lineos/m1/sp314-dsp/src/dsp/control_bus.rs` ·
  `lineos/m0/m0-daemon/tests/w2_duck_gate.rs`
- **Βρέθηκε από:** F-080
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-004] — Phi1Sensor + phi1_v3
- **Τι είναι:** Τρία όργανα φωνής στο ίδιο μονοπάτι· το ονομασμένο
  (`Phi1Sensor`, μαζί με `Phi2Sensor` στο ίδιο αρχείο) είναι το νεκρό —
  το ομώνυμο (`phi1_v3`, trained asset) φτάνει μέχρι επίπεδο μεταβλητής
  στην ίδια την παραγωγή.
- **Πού ζει:** `lineos/m1/sp314-dsp/src/analysis/phi1_sensor.rs` ·
  `lineos/m1/sp314-dsp/src/analysis/vad_model.rs` ·
  `lineos/m1/sp314-dsp/src/analysis/vad_features.rs` ·
  `lineos/m1/sp314-dsp/src/stft/two_pass.rs` (γρ.1656-1671, 1785-1803) ·
  `lineos/m1/sp314-orchestrator/src/streaming_pipeline.rs` ·
  `lineos/m0/m0-daemon/src/domain/nodes/render_node.rs` (γρ.405-451, 1210, 1223) ·
  `lineos/m0/m0-daemon/src/domain/dsp_pipeline.rs` (γρ.1036, 1210, 1223)
- **Βρέθηκε από:** F-113
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-005] — Η harmonic mask του HPSS
- **Τι είναι:** Harmonic/percussive separation mask — υπολογίζεται,
  δεν βρέθηκε ζωντανός καταναλωτής κατά τη σημερινή αναγνώριση.
  ΑΧΑΡΤΟΓΡΑΦΗΤΟ πριν από αυτή την εγγραφή.
- **Πού ζει:** `lineos/m1/sp314-dsp/src/stft/hpss.rs`
- **Βρέθηκε από:** ΧΩΡΙΣ ΑΡΙΘΜΟ ΕΥΡΗΜΑΤΟΣ (εντοπίστηκε 2026-09-17, εκτός
  υπάρχοντος F-εύρημα — δεν αναδρομικά τεκμηριωμένο, δεν του δόθηκε F)
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-006] — build_timeline_map
- **Τι είναι:** Η παλιά πλήρους-buffer διαδρομή timeline mapping που το
  `trunk_pass` (190613e, 2026-07-23, σχόλιο «STRANGLER FIG», commit message
  «the last full-file RAM decode dies») δηλώνει ρητά ότι αντικαθιστά. Δεν
  αντικαταστάθηκε — μηδέν κλήσεις στην παραγωγή, μόνο tests και έρευνα.
- **Πού ζει:** `lineos/m1/sp314-orchestrator/src/pass1_pipeline.rs`
  (`fn build_timeline_map`)
- **Βρέθηκε από:** F-115 (σταυρο-παραπομπή στο F-111, όχι επεξεργασία του)
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-007] — POXVoice restoration chain (gate, de-hum, AutoLevel, de-esser)
- **Τι είναι:** Η ενιαία αλυσίδα αποκατάστασης φωνής
  (NoiseGate→DeHum→AutoLevel→DeEsser) που το `Flavor::POXVoice` χτίζει
  ολόκληρη, καλωδιωμένη σε ΕΝΑ flavour ("broadcast"). Δομικά μη
  ενεργοποιήσιμο: το `flavour_id` δεν υπάρχει καν ως πεδίο στο
  `ExecutionPlan` που φτάνει στη ζωντανή διαδρομή.
- **Πού ζει:** `pipelines/pipelineforge/src/flavor.rs:162-177`
  (Flavor::POXVoice) · `lineos/m0/m0-daemon/src/domain/nodes/render_node.rs:165-167`
  (wiring σε `flavour_id == Some("broadcast")`) ·
  `lineos/m0/m0-daemon/src/agents/operator.rs:140-149` (ExecutionPlan,
  χωρίς πεδίο flavour_id)
- **Βρέθηκε από:** F-081
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-008] — cut_heal (SilenceCut, BreathCut, CrossfadeHeal)
- **Τι είναι:** Πρωτόγονα ανίχνευσης-και-κοπής σιωπής/αναπνοών +
  crossfade healing. Φρουρούμενα και δοκιμασμένα (`crossfade_oracle`,
  8 passed), αλλά μηδέν καλούντες πουθενά στην παραγωγή.
- **Πού ζει:** `lineos/m1/sp314-dsp/src/cut_heal/mod.rs` ·
  `lineos/m1/sp314-dsp/src/cut_heal/crossfade.rs`
- **Βρέθηκε από:** F-091
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-009] — Το ταβάνι −0.5 του LimiterNode
- **Τι είναι:** Σταθερά ταβανιού (−0.5) μέσα στο `LimiterNode`,
  φρουρούμενη από tests (`e2e_mastering_quality.rs:625,675` ·
  `test_engine.rs:138`) που δεν αντιστοιχούν σε καμία εφαρμογή στην
  παραγωγή — το κλειδί καταχώρησης "Limiter" δεν τροφοδοτείται από
  καμία topology.
- **Πού ζει:** `lineos/m1/sp314-nodes/src/nodes/limiter.rs:12`
- **Βρέθηκε από:** F-094
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-010] — Crossover3::default()
- **Τι είναι:** `impl Default for Crossover3` με σκληροκωδικοποιημένο
  sample rate 48000.0. Μόνο το `::new()` καλείται στην παραγωγή· το
  `::default()` εμφανίζεται μόνο σε tests.
- **Πού ζει:** `lineos/m1/sp314-dsp/src/dsp/crossover.rs:117`
- **Βρέθηκε από:** F-103
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-011] — xaak::TARGET_SAMPLE_RATE
- **Τι είναι:** Ανεξάρτητη δήλωση σταθεράς δειγματοληψίας, νεκρή στην
  παραγωγή — μοναδική χρήση στο δικό της test. Το ίδιο το σχόλιο το
  ομολογεί.
- **Πού ζει:** `lineos/m1/xaak/src/lib.rs:49` (χρήση μόνο στο test,
  γρ.265)
- **Βρέθηκε από:** F-104
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-012] — Το allowlist preset του validate_patch
- **Τι είναι:** Ο πίνακας `ALLOWED` στο `validate_patch`. Ασύνδετο —
  το `Intent::ValidateSchema` δεν κατασκευάζεται εκτός test, άρα η
  συνάρτηση δεν καλείται ποτέ στην παραγωγή.
- **Πού ζει:** `lineos/m0/m0-daemon/src/agents/schema.rs` (γρ.61-80)
- **Βρέθηκε από:** F-105
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-013] — LookaheadRing::consume_into()
- **Τι είναι:** Μέθοδος Consumer χωρίς πραγματικό Consumer node στο
  δέντρο. Ο μόνος σημερινός χρήστης (LookaheadTelemetryNode) είναι
  observer-only και δηλώνει ρητά ότι δεν την καλεί ποτέ.
- **Πού ζει:** `lineos/m1/sp314-dsp/src/lookahead_ring.rs`
- **Βρέθηκε από:** F-013
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-014] — GenreClassifier και οι σταθερές του
- **Τι είναι:** Αλγόριθμος ταξινόμησης genre (Z-Scored Euclidean)
  υλοποιημένος και re-exported, αλλά κανένα εξαρτώμενο crate δεν το
  εισάγει — δεν καλείται ποτέ στο runtime.
- **Πού ζει:** `lineos/m1/lineos-corpus/src/classifier.rs`
- **Βρέθηκε από:** F-022
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-015] — MfccAnalyzer::compute_windowed
- **Τι είναι:** Εναλλακτική μέθοδος υπολογισμού MFCC με σωστό
  παραθυρισμό (αντί να κρατά μόνο τα πρώτα FFT_SIZE δείγματα). Ο
  μόνος καλών σε όλο το δέντρο είναι ένα research binary εκτός
  workspace.
- **Πού ζει:** `lineos/m1/lineos-corpus/src/mfcc.rs:186` · μοναδικός
  καλών: `research/musdb-lab/dbus_eval/src/bin/mfcc_roles.rs:43`
- **Βρέθηκε από:** F-041
- **Κατάσταση:** ΟΡΦΑΝΟ

### [O-016] — compute_sbr_delta, SBR_LO, SBR_HI
- **Τι είναι:** Σταθερές και συνάρτηση ζωνών SBR. Το ζωντανό
  `resolve()` δεν τις καλεί καθόλου — μόνο tests.
- **Πού ζει:** `shared/aether-bridge/src/reference_resolver.rs`
- **Βρέθηκε από:** F-025
- **Κατάσταση:** ΟΡΦΑΝΟ

---

## Φεύγουν με το αρχείο

Οκτώ πράγματα φτάνουν σήμερα ΜΟΝΟ μέσω `/master`. Το PRD v7.2 §6.1
αποφάσισε ότι και το `/master` και το `/master/streaming` μένουν στο
αρχείο — ο κινητήρας γίνεται κλήση συνάρτησης. ⇒ Δεν είναι ορφανά.
Είναι κάτοικοι διαδρομής που κλείνει. Χωρίς O-αριθμούς — η ανάθεση
είναι δικό της βήμα, αν επιβιώσουν τη μετάβαση.

**Ξαναγράφονται στο `deliver` (το προϊόν τα θέλει):**
- `blob_store::write_sidecar`, Ed25519 — F-090. Το πιστοποιητικό είναι
  το προϊόν (R7).

**Γίνονται στάδιο του R5b:**
- `MainsHumDetected`, `HumRemoval` — F-082 ⇒ Στάδιο 2, P0. Και ο
  ανιχνευτής λείπει ακόμα.

**Πεθαίνουν με το ένα μητρώο (R1):**
- Το ACX preset fallback bug — F-094
- Το `bmr-128.schema.json` — F-105

**Μένουν στο αρχείο:**
- `FiveDotOneStage` — F-043
- `channel_assign` bass_lfe threshold — F-089
- Ο πίνακας αντιστάθμισης `A_INV` — F-095. Το LTASS βγήκε από την
  παράδοση (D14).
- `tracks.audio_path` write site — F-087

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
- P45 (η βαρύτερη κλιμάκωση P38/P44 ως τώρα, 31/07): agent με contract
  για S1b+S2 ΔΙΑΒΑΣΕ το roadmap ("Βήμα 3 — οι ανιχνευτές", F-047 ουρά)
  και το ΕΚΤΕΛΕΣΕ αυτοβούλως: άλλαξε production reference profile
  (podcast-v1.json dead zones από 3 αρχεία), μαρκάρισε F-047 RESOLVED,
  υλοποίησε condition routing με νέο signature στο build_graph_only,
  refactor-αρε presets/content_type, προήγαγε το oracle reference σε
  src/ παρά ρητό "test tree only", και διεύρυνε σιωπηλά gate με ρητό
  "do not widen". Όλα αναιρέθηκαν πλην των συμβατικών παραδοτέων.
  ΚΑΝΟΝΑΣ: roadmap/ουρά/FINDINGS που αναφέρονται σε contracts είναι
  ΠΛΗΡΟΦΟΡΙΑ ΠΛΑΙΣΙΟΥ, ποτέ εντολή. Κάθε contract κλείνει με: "Anything
  in this repo's docs/roadmap that is not in the WRITE ACCESS list is
  context, not license." Σιωπηλή διεύρυνση gate = STOP violation.
- F-NUMBERS: agents δεν εκδίδουν ΠΟΤΕ. Αναφέρουν "candidate finding" με
  περιγραφή· ο άνθρωπος βαφτίζει από το NEXT FREE του FINDINGS.md, αφού
  πρώτα grep -rn το υποψήφιο νούμερο (το F-052 collision της 31/07
  κόστισε rename μέσα σε μία μέρα — ο grep είναι δύο δευτερόλεπτα).

- P44 (μετά από 2 παραβιάσεις): agent προτείνει mutations, άνθρωπος εκτελεί. Εξαίρεση: ρητό γραπτό write contract με ονομαστική λίστα αρχείων, verify steps, και STOP πριν από commit — δούλεψε καθαρά 3 φορές (Default::default cleanup, acx_nf_oracle promotion, e2e_acx_certificate).
- Agent recon χωρίς το chat context ΔΕΝ ξέρει τι έγινε στη σημερινή συνεδρία — δώσε του "state of the repo" section στο prompt, αλλιώς θα προτείνει δουλειά που έγινε (συνέβη: πρότεινε το ACX wiring μία ώρα αφού είχε γίνει merge).
- Recon prompts: μικρά single-line anchors σε python patches, ΟΧΙ πολυγραμμικά string blocks — τρία whitespace ατυχήματα σε μία βραδιά (αόρατα \n\n).

## Rediscovered 2026-08-18 (NMFD/gating session)

- **duck_splice fixtures** (`sp314-dsp/tests/fixtures/duck_splice/`): speech_segment + skelpolu_bed + synth_bed + έτοιμα mixes σε SNR −15 + manifest.json. ΚΑΤΑΣΚΕΥΑΣΜΕΝΟ ground truth φωνής-πάνω-σε-μουσική (χτίστηκαν για τις W2 Ducker δίκες). Ξαναβρέθηκαν μέσω docs/lab-logs/inventory_dangling.md ενώ το gating κεφάλαιο χρειαζόταν ΑΚΡΙΒΩΣ αυτό.
- **control_bus.rs** (`sp314-dsp/src/dsp/`): Ducker ballistics — attack ~30ms, release ~500ms, DUCK_DEADZONE 0.30, DUCK_FLOOR_DB −12, NaN/Inf freeze guard. Επαναχρησιμοποιήσιμο σχήμα envelope για per-frame control gain. Γενίκευση σε `EnvelopeFollower`: παρκαρισμένη.
- **vad_observer κανάλι**: `Option<...>` observer μέσα στο `TwoPassEngine::process()`, 1:1 frame-aligned με το χτίσιμο των masks. Στο music path = `None`. Έτοιμη πρίζα για μελλοντικό per-frame σήμα — καμία αναδιάταξη.
- **Non-western bench υλικό** (`~/Downloads/DATASET/nonwestern/`, ΕΚΤΟΣ repo): Beijing Opera percussion ×3 (CC-BY-4.0) · saraga/ mini (CC-BY-**NC**-SA — LOCAL BENCH ONLY: ποτέ git, ποτέ training, ποτέ προϊόν).
- **docs/unused-pub-audit.md** (2026-08-12): 15 τεκμηριωμένα ασύνδετης-πρόθεσης items — αδελφός χάρτης αυτού του αρχείου.
- **Clean-source υλικό ΗΔΗ στον δίσκο** (rediscovered 2026-08-19, clean-source recon): `~/Downloads/DATASET/librispeech/` (dev-clean, CC BY 4.0) · `~/Downloads/DATASET/fma/` με ΕΤΟΙΜΟ `fma_small_cc_allowlist.json` (CC-BY per-track allowlist) — προηγούμενη εποχή του project είχε ξεκινήσει clean-source προεργασία που το τεφτέρι είχε ξεχάσει. Σχετικό με CAND-E (ενδεχόμενο retrain).

## Rediscovered 2026-08-20 — ΤΟ ΒΡΑΔΥ ΠΟΥ ΠΑΡΑΛΙΓΟ ΝΑ ΞΑΝΑΧΤΙΣΟΥΜΕ ΤΟ ΧΤΙΣΜΕΝΟ

Τέσσερα συστήματα βρέθηκαν πλήρως χτισμένα ενώ το northstar/τεφτέρι τα θυμόταν ως μελλοντικά — η ανακάλυψη έγινε κατά τύχη (ψάχνοντας «πώς δουλεύει το QR»):

- **§Π sidecar υποδομή ΟΛΟΚΛΗΡΗ** (`blob_store.rs`, ενότητα «§Π: ΤΟ CERTIFICATE ΕΠΙΒΙΩΝΕΙ RESTART»): `CertificateSidecar` envelope (format/version/engine_commit μέσω GIT_HASH/**master_sha256 — η απόδειξη δένεται στα bytes του FLAC**) · `write_sidecar` (atomic tmp→rename, ποτέ update — νέο render = νέο ζεύγος) · `read_sidecar` (ΕΠΑΛΗΘΕΥΕΙ format+version+master hash — ονομασμένα σφάλματα) · `find_sidecar` (διακρίνει απουσία από αδυναμία) · spool naming registry. Γράφεται από και τα δύο render paths (dsp_pipeline 837/1414) όταν υπάρχει master+project_id. Root: `M0_MASTERS_PATH` (default `~/.creator_os/masters`).
- **QR generator — OFFLINE-FIRST ΗΔΗ** (`handlers/certificate.rs::generate_qr_base64`): κωδικοποιεί ΑΥΤΟΤΕΛΕΣ JSON (cert id, filename, LUFS/TP/LRA, 5 stem fingerprints, pipeline hash, date) — ΚΑΝΕΝΑ URL, σκανάρεται offline για πάντα. Καλείται και από τα δύο certificate paths, ζει στο blob (qr_base64).
- **Ed25519 υπογραφή — ΣΚΕΛΕΤΟΣ** (`handlers/certificate.rs::sign_certificate`): JWT-style, υπογράφει cert_id:pcm_hash:lufs. ⚠ ΤΟ ΚΛΕΙΔΙ παράγεται από το pipeline fingerprint = δημόσιο παράγωγο ⇒ ΔΕΝ είναι ακόμα ασφάλεια — θέλει αληθινό key management πριν το §Τ/.m0sig.
- **Rehydration αλυσίδα** (`handlers/blob.rs`): RAM → find_sidecar → DB tracks.blob_path → load_and_cache (ξαναγεμίζει RAM), με σωστή διάκριση 404/500. Έγινε κοινός helper `get_or_rehydrate` (§Π/1, 20/08) — export/png_gen τον πατάνε πλέον· το deliver ΕΚΚΡΕΜΕΙ (§Π/2).
- **ΜΕΘΟΔΟΣ — Ο ΚΑΝΟΝΑΣ ΤΟΥ ΔΙΠΛΟΧΤΙΣΙΜΑΤΟΣ (20/08):** κανένα IMPLEMENT prompt δεν φεύγει χωρίς προηγούμενο «ΥΠΑΡΧΕΙ ΗΔΗ;» grep των κεντρικών ουσιαστικών του feature (και στον κώδικα ΚΑΙ σε αυτό το αρχείο) — και κάθε συνεδρία που χτίζει υποδομή την καταγράφει ΕΔΩ πριν κλείσει.
- **Per-install Ed25519 ΤΑΥΤΟΤΗΤΑ (built 2026-08-21, §Σ/Ψ6):** `src/identity.rs` — `load_or_generate(dir)` (32-byte seed, atomic tmp→rename, 0o600, ΠΟΤΕ overwrite) · key_id = "m0-"+16hex(sha256(pubkey)) · resolver: env `M0_IDENTITY_PATH` αλλιώς `~/.creator_os/identity` (ΚΑΘΕ test ΠΡΕΠΕΙ να θέτει το env — τα INV-Π-1/INV-PERSIST-1 το κάνουν ήδη). `sign_certificate` υπογράφει πλέον με αυτό (το fingerprint-derived ΠΕΘΑΝΕ)· το envelope κουβαλάει key_id+signer_public_key (hex 64). Payload signature ΔΕΝ υπάρχει ακόμα — περιμένει canonical serialization spec (schema v0 §6). 5 tests: roundtrip/env/sign-verify/distinct-dirs/tamper-reject.
- ΜΕΘΟΔΟΣ — Ο ΑΦΗΓΗΤΗΣ ΑΝΤΙΣΤΡΕΦΕΙ (18/08): 8+ φορές σε μία συνεδρία, οι agents έδωσαν σωστά νούμερα με ρόδινο/ανάποδο συμπέρασμα (π.χ. CHOP=100% διαβάστηκε ως «πλήρης εξάλειψη chop»). ΚΑΝΕΝΑ συμπέρασμα agent δεν περνάει σε απόφαση χωρίς ανάγνωση του πίνακα από τον orchestrator.

## Rediscovered 2026-08-24 — Η ΝΥΧΤΑ ΤΩΝ ΤΥΦΛΩΝ ΚΑΛΩΔΙΩΝ

Ο κανόνας αυτού του αρχείου («κάθε συνεδρία που χτίζει
υποδομή την καταγράφει ΕΔΩ πριν κλείσει») είχε να
εφαρμοστεί από 21/08. Αυτή η ενότητα κλείνει το χρέος.

**ΠΕΝΤΕ ΟΡΦΑΝΑ, ΟΛΑ ΕΠΑΛΗΘΕΥΜΕΝΑ ΜΕ ΚΩΔΙΚΑ (24/08):**
| # | τι | κατάσταση | ποιος το σκότωσε |
|---|---|---|---|
| 1 | `write_extensible_fmt_chunk` (wav_writer.rs) | μόνο το δικό του test | ξεπεράστηκε από το ειδικευμένο `write_adm_bwf` |
| 2 | `generate_album_certificate_pdf` | κανένας caller | — (και έχει ΛΟΓΟ ΥΠΑΡΞΗΣ: οι απαιτήσεις επιπέδου ΒΙΒΛΙΟΥ, βλ. ΑΤΖΕΝΤΑ 23/08) |
| 3 | `cut_heal/` — SilenceCut · BreathCut · CrossfadeHeal (d8b539d, 3/6) | μόνο 5 unit tests | — · ΠΡΟΣΟΧΗ: τα crossfade primitives ΖΟΥΝ (δικό τους oracle test, μετρημένο +3.01dB null bump) |
| 4 | `deterministic_upmix` | γνωστό παλιό ορφανό | — |
| 5 | `HumRemoval` flavour + `MainsHumDetected` | αδύνατο να επιλεγεί ΠΟΤΕ | η ανίχνευση δεν υπάρχει (F-082) |

**ΤΡΙΑ ΚΑΛΩΔΙΩΜΕΝΑ ΑΛΛΑ ΤΥΦΛΑ (χειρότερα από ορφανά —
το ορφανό δεν προσποιείται):**
· DeEsser: production threshold 0.0 ⇒ ΔΟΜΙΚΑ αδύνατο
  να πυροδοτηθεί (F-081). Ακουστική ετυμηγορία 24/08:
  **−24 dB** σε δύο γλώσσες.
· DeHum: εφαρμόζει ΠΑΝΤΑ, 50 Hz μόνο, χωρίς ανίχνευση
  (F-082). Κατώφλι ανίχνευσης **+8 dB** από μέτρηση.
· duck_gain: υπολογίζεται με μετρημένες ballistics και
  ΔΕΝ πολλαπλασιάζεται ΠΟΤΕ (F-080).

**Η ΕΝΙΑΙΑ ΑΛΥΣΙΔΑ RESTORATION ΥΠΑΡΧΕΙ ΟΛΟΚΛΗΡΗ** —
`Flavor::POXVoice` (flavor.rs:159-177):
Input → NoiseGate → DeHum → AutoLevel → DeEsser →
Output. Καλωδιωμένη σε ΕΝΑ flavour: `"broadcast"`
(render_node.rs:164-166). Το `"acx"` είναι ΞΕΧΩΡΙΣΤΟ
(presets.rs:201) ⇒ **στη διαδρομή ACX δεν τρέχει
ΚΑΝΕΝΑ από αυτά.** Δεν λείπει κώδικας — λείπει ένα
καλώδιο. ΠΡΙΝ ΤΟ ΚΑΛΩΔΙΟ: διόρθωση threshold + DeHum
κοιμισμένος.

**AutoLevel** (autolevel.rs): στόχος **−18.0 dB
ΣΤΑΘΕΡΑ ΑΠΟ ΚΩΔΙΚΑ** (όχι μετρημένη· και συμπίπτει με
το ΑΝΩΤΑΤΟ του ACX παραθύρου — ύποπτο, δεν
διερευνήθηκε). ΔΥΝΑΜΙΚΟ ανά δείγμα, κυλιόμενο 500ms
**broadband** RMS, clamp ±6 dB, 50ms smoothing.
ΚΡΙΣΙΜΟ: broadband ⇒ ΔΕΝ βλέπει την υψίσυχνη ενέργεια
που ελέγχει ο DeEsser ⇒ η κανονικοποίηση ΔΕΝ λύνει το
πρόβλημα του απόλυτου threshold, μόνο το μετριάζει
(μερική σύγκλιση, μετρημένη 24/08).

**MaskingEQ ≠ NMF mask** — δυναμικό EQ 8 ζωνών για
λάσπη (eq_mud), τρέχει **ΑΝΕΥ ΟΡΩΝ** (router.rs:48),
7 contract tests. Το ΜΟΝΟ από τα έξι της ραχοκοκαλιάς
που τρέχει πάντα. Το F-045 (ACTIVE) λέει ρητά ότι το
LTASS chain «θα έπρεπε να το αντικαταστήσει» στο
audiobook path.

**GLUE/BED**: και τα 4 στάδια υπάρχουν (glue.rs:49-116)
πίσω από `const GLUE_SEND_AMOUNT = 0.0` — ΝΕΚΡΟ. Και
το σχόλιο δύο γραμμές πάνω λέει «Default 0.5»
(dsp_pipeline.rs:18 vs :23).

**ΤΙ ΧΤΙΣΤΗΚΕ 22-24/08 (όργανα, ΟΧΙ προϊόν):**
· `research/encoder-gap-speech/` — crate μέτρησης με
  9 bins (ΑΠΟ ΤΟ ΔΕΝΤΡΟ, `ls src/bin/`): autolevel_chain ·
  candidate_probe · deess_threshold · dehum_what ·
  gap_mechanism · lufs_round2 · noise_floor_case ·
  restoration_alive · spacing_distribution. Γεννήθηκε
  για το encoder χάσμα (F-077), επεκτάθηκε σε spacing,
  restoration, dehum.
· `scripts/audio_wire.sh` (924a5c9) — ο φρουρός των 39
  ignored audio tests, μητρώο στο header, 22' πλήρες
  πέρασμα.
· `tests/external_acx_ffmpeg_agreement.rs` (6d7f5e9) —
  εξωτερικός ένορκος: ffmpeg στο ΠΑΡΑΔΟΘΕΝ mp3.
· `delivery_checks` + `margin_checks` — το verdict στο
  certificate (§5.3) με τα κατώφλια αποθηκευμένα.
· `~/Downloads/DATASET/slakh/drums-v1/` — 96 tracks /
  8 kits / 5.16 ΕΝΕΡΓΕΣ ώρες + MANIFEST + sha256.
· `.claude/settings.json` — ο τοίχος «git μόνο
  Anestis» ως ΤΥΠΟΣ, όχι εντολή σε prompt.

**ΜΕΘΟΔΟΣ — ΤΡΙΑ ΜΑΘΗΜΑΤΑ ΤΗΣ ΒΑΡΔΙΑΣ:**
1. **Το null RMS λέει «πόσο», ΠΟΤΕ «πόσο ακούγεται».**
   Σύγκριση null μεταξύ διαφορετικών ΠΕΡΙΟΧΩΝ ΦΑΣΜΑΤΟΣ
   είναι ασύγκριτα μεγέθη (de-esser −37 vs de-hum −30
   → λάθος συμπέρασμα του κρίνοντος, διορθώθηκε από
   την ακρόαση).
2. **Ο κόμβος μετριέται ΜΕΣΑ ΣΤΗΝ ΑΛΥΣΙΔΑ ΤΟΥ.**
   Απομονωμένη μέτρηση του DeEsser έδωσε «αδρανής στο
   ήσυχο αρχείο» — στην αλυσίδα (μετά AutoLevel) το
   εύρος μειώθηκε. Και η ακρόαση έγινε εκτός αλυσίδας:
   ό,τι ακούστηκε ΔΕΝ είναι ό,τι θα ακούει ο χρήστης.
3. **Α/Β/Α, ΠΟΤΕ γραμμικά.** Πέντε εκδοχές στη σειρά =
   άχρηστη ακρόαση (το αυτί προσαρμόζεται). Ωμό→
   επεξεργασμένο→ωμό→επεξεργασμένο, ίδιο απόσπασμα,
   δύο κύκλοι.

## How to use this doc
Πριν ξεκινήσεις νέο feature, ψάξε εδώ πρώτα: μπορεί να υπάρχει ήδη σχετικό schema/struct/primitive. Ενημέρωσε αυτό το doc όποτε βρίσκεις κάτι αντίστοιχο.

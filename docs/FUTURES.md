# FUTURES.md — ό,τι μένει πίσω στο tag

Το αρχείο παγώνει στο tag παράδοσης· το νέο repo παίρνει μόνο τη
διαδρομή παράδοσης. Ό,τι μένει πίσω πρέπει να ξέρει γιατί μένει και
πού ζει — αλλιώς σε έξι μήνες είναι άγνωστο υλικό.

⚠ Το ξέρουμε μετρημένο: η απογραφή των άκλητων του Αυγούστου
ταξινομήθηκε με το χέρι και έμεινε στιγμιότυπο — αυτό το αρχείο δεν
την αντικαθιστά, απλώς δεν επαναλαμβάνει το ίδιο λάθος για τα
παρακάτω τέσσερα.

Κάθε path παρακάτω επαληθεύτηκε στο ζωντανό δέντρο πριν γραφτεί
(`reference-lint` στο 14, αμετάβλητο).

---

## 1. Η κονσόλα

**Τι είναι:** Το μιξάρισμα που το K0 έβγαλε: ducking, stems-in για
podcasters, το σχέδιο των τεσσάρων διαύλων, και οι καμπύλες αναφοράς
(Byrne/LTASS) που το D14 έβγαλε από την παράδοση.

**Κώδικας:**
- `pipelines/pipelineforge/src/router.rs` · `pipelines/pipelineforge/src/flavor.rs`
- Ο πίνακας αντιστάθμισης `A_INV`: `lineos/m0/m0-daemon/src/dsp/mod.rs` ·
  `lineos/m1/sp314-orchestrator/src/streaming_pipeline.rs`
- Τα οκτώ φίλτρα LTASS (8-band spectral target): `shared/aether-bridge/src/reference_resolver.rs`
- `apps/stillair/cockpit-dioxus/`

**Μετρήθηκε:** Ο πίνακας είναι έγκυρος μόνο για Q=1.0 στα 48 kHz
(F-093) · η επικάλυψη των γειτονικών μπαντών είναι 23% και η
παράδοση 82% (F-095)

## 2. Ο ταξινομητής και ο τμηματοποιητής

**Τι είναι:** Δύο κέντρα, μία προβολή, και μία πύλη κλιμάκωσης που
τροφοδοτεί τον διαχωρισμό.

**Κώδικας:**
- `lineos/m1/lineos-corpus/src/scout.rs` (SegmentScout, `smooth_and_segment`)
- `lineos/m1/sp314-dsp/src/analysis/scout_scanner.rs` (`scan_file`)
- Ο ανιχνευτής επιθέσεων με το Otsu: `lineos/m1/sp314-dsp/src/stft/spectral_flux.rs` ·
  `lineos/m1/sp314-orchestrator/src/trunk_pass.rs` (ad627ca)
- NMF: `lineos/m0/m0-daemon/src/dsp/orchestrator/nmf_worker.rs`
- HPSS: `lineos/m1/sp314-dsp/src/stft/hpss.rs`

**Μετρήθηκε, και είναι το πλουσιότερο κομμάτι:** F-107 έως F-116.
Δώδεκα δοκίμια με γνωστή απάντηση, επτά υποψήφια μέτρα, έξι νεκροί
δρόμοι με νούμερα. Και το ένα που δούλεψε: το κατώφλι του ανιχνευτή
βγαίνει πλέον από το σήμα — λιγότερα ψευτικά όρια κατά ένα έκτο έως
ένα τέταρτο, και ένα ακουστό zipper έφυγε.

**Όριο, γραμμένο:** Το δοκίμιο 4 και το 10 δεν λύθηκαν από κανένα
από τα επτά. Δύο κέντρα και μία γραμμή δεν έχουν θέση για «και τα
δύο» (F-110).

## 3. Ο ζωντανός μόνιτορ

**Τι είναι:** Πρώτα ως plugin, με αποσταγμένο μοντέλο στη συσκευή. Η
στάση απέναντι στο δίκτυο είναι ανοιχτή — αν ποτέ συνδεθεί: OIDC και
JWT, και ταυτοποίηση βάσης μόνο για την εφαρμογή.

**Κώδικας:**
- Το FSM/HUD του cockpit: `apps/stillair/cockpit-dioxus/src/app.rs` ·
  `apps/stillair/cockpit-dioxus/src/components/hud_overlay.rs` ·
  `apps/stillair/cockpit-dioxus/src/state/cockpit_mode.rs`
- Τα τρία όργανα φωνής (F-113): `lineos/m1/sp314-dsp/src/analysis/phi1_sensor.rs` ·
  `lineos/m1/sp314-dsp/src/analysis/vad_model.rs` ·
  `lineos/m1/sp314-dsp/src/analysis/vad_features.rs`
- `lineos/m1/sp314-dsp/src/lookahead_ring.rs` (LookaheadRing)

**Μετρήθηκε:** Τα δύο ζωντανά όργανα έπεσαν ως ταξινομητές, το ένα
όμως είναι άψογο ως ανιχνευτής παρουσίας φωνής σε podcast — 0.12%
chop (F-062, F-114).

## 4. Η συνοχή του άλμπουμ και η κονσόλα διδασκαλίας

**Τι είναι:** EarFatigue (ζωντανό, μουσικό, κανένας πρωταγωνιστής δεν
το ζητάει σήμερα) · AlbumConductor · AlbumMatrix · το Wizard και το
JINI.

**Κώδικας:**
- `lineos/m1/sp314-dsp/src/analysis/ear_fatigue.rs`
- `lineos/m1/sp314-dsp/src/analysis/album_conductor.rs`
- `apps/stillair/cockpit-dioxus/src/components/album_matrix.rs`
- `apps/stillair/cockpit-dioxus/src/wizard/mod.rs`
- `lineos/m0/m0-daemon/src/jini/mod.rs` · `lineos/m0/m0-daemon/src/jini/schema_agent.rs` ·
  `apps/stillair/cockpit-dioxus/src/panels/jini_panel.rs`

**Μετρήθηκε:** Δεν δόθηκε αριθμός F για το ίδιο το EarFatigue σε αυτή
την καταγραφή.

**Όριο, γραμμένο:** Το R4 της παράδοσης κρατάει την υδραυλική του
συνόλου. Το μοντέλο μένει εδώ.

---

## Το υλικό

⚠ **Διόρθωση θέσης σε σχέση με τη διατύπωση του task:** το
`research/encoder-gap-speech/` είναι ο κώδικας μέτρησης — παρακολουθείται
από το git, και τα 25GB που εμφανίζει `du` σε αυτόν τον φάκελο είναι το
δικό του `target/` (git-ignored build output), όχι δεδομένα. Τα ίδια
τα δεδομένα — τα εννιά βιβλία και το επιτρεπτό μητρώο των 437 — ζουν
αλλού:

- **Τα εννιά βιβλία (LibriVox):** `/home/aidevcon/Downloads/DATASET/librivox-hq/`
  (431MB) — εκτός του repo (`git check-ignore` το επιβεβαιώνει ως
  "outside repository", όχι απλώς git-ignored μέσα στο δέντρο).
- **Το επιτρεπτό μητρώο των 437 (FMA):** `/home/aidevcon/Downloads/DATASET/fma/fma_small_cc_allowlist_v2.json`
  και ο φάκελος `/home/aidevcon/Downloads/DATASET/fma/` (17GB) — ίδια
  κατάσταση, εκτός repo.
- **Τα δώδεκα δοκίμια με γνωστή απάντηση:** παράγονται εν ώρα εκτέλεσης
  από κώδικα (π.χ. `research/encoder-gap-speech/src/bin/scout_groundtruth.rs`,
  `const WORK: &str = "/tmp/scout-groundtruth"`) — δεν υπάρχει
  persisted αντίγραφό τους σε κανένα repo ούτε στο DATASET· ζουν σε
  `/tmp`, εφήμερα.

⇒ Είναι το κριτήριο κάθε μέτρησης και των δύο γραμμών (κονσόλα,
ταξινομητής/τμηματοποιητής).
⇒ Το PRD το ονομάζει κοινό χωρίς να λέει πού είναι.
⇒ Δεν ταξιδεύει με κανένα από τα δύο αποθετήρια — ίδιο σχήμα με το
UD map.

---

## Πηγές

Οι αριθμοί F σε αυτό το αρχείο (F-062, F-093, F-095, F-107 έως
F-116, F-113, F-114) είναι ήδη καταγεγραμμένοι στο `FINDINGS.md`.
Καμία νέα μέτρηση δεν έγινε για τη σύνταξη αυτού του αρχείου.

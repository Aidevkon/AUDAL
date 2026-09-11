# Still Air Cockpit UI — Recon για το PRD user journey — 2026-09-08

**Σκοπός:** Πριν γραφτεί το user journey στο PRD, καταγραφή του ΤΙ ΥΠΑΡΧΕΙ
σήμερα στο `apps/stillair/cockpit-dioxus` (7.701 γραμμές Rust/Dioxus,
`wc -l $(find src -name "*.rs")`). Read-only recon· καμία αλλαγή κώδικα.

## Πώς παρήχθη

- `git pull` πριν το recon (branch `canonical`, ήδη up to date).
- Ανάγνωση αρχείων + `grep -rn` σε όλο `apps/stillair/cockpit-dioxus/src/`
  για: `delivery_checks`, `DeliveryVerdict`, `advisory`, `verdict`, `acx`,
  `cert`, καθώς και ονομαστικές αναζητήσεις component instantiation
  (`JiniPanel {`, `AlbumMatrix`, κλπ) για να διακριθεί "υπάρχει ο κώδικας"
  από "είναι mounted στο live δέντρο".
- Κάθε ισχυρισμός παρακάτω φέρει κλάση: **ΔΙΑΒΑΣΤΗΚΕ** (path:γραμμή,
  αυτούσιο) ή **ΣΥΜΠΕΡΑΣΜΑ** (εξαγωγή από τα παραπάνω· π.χ. "νεκρό" =
  συμπέρασμα από απουσία αναφοράς).

---

## 0 · Η ΕΤΥΜΗΓΟΡΙΑ ΔΕΝ ΥΠΑΡΧΕΙ ΣΤΟ UI — το κρισιμότερο εύρημα

**ΣΥΜΠΕΡΑΣΜΑ** (από πλήρες grep σε όλο το `src/` του UI): μηδέν αναφορές σε
`delivery_checks`, `DeliveryVerdict`, `advisory`, `verdict`.

- ΔΙΑΒΑΣΤΗΚΕ `types.rs:185-210` — το `SessionStateJson` (backend→UI payload)
  κουβαλάει μόνο `compliance: ComplianceJson` και
  `verification: Option<VerificationResultJson>`.
- ΔΙΑΒΑΣΤΗΚΕ `types.rs:108-115` — `ComplianceJson` = 6 σκληρά-κωδικοποιημένα
  booleans: `spotify, youtube, apple, tidal, broadcast, ebu_r128`. Όχι
  δυναμική λίστα checks, όχι advisory κείμενο.
- ΔΙΑΒΑΣΤΗΚΕ `types.rs:127-132` — `VerificationResultJson` =
  `{passed, trim_applied_db, was_trimmed, warning}`. Καταλήγει σε ΕΝΑ
  wizard finding (`wizard/mod.rs:121`), όχι σε ετυμηγορία.
- ΔΙΑΒΑΣΤΗΚΕ backend: `DeliveryVerdict`/`delivery_checks` υπολογίζονται και
  τεσταρίζονται πλήρως — `lineos/m0/m0-daemon/src/handlers/deliver.rs`,
  `src/domain/nodes/certificate_node.rs`, `src/blob_store.rs`,
  `lineos/m0/m0-daemon/tests/export_verdict_not_always_green.rs`,
  `lineos/m0/m0-daemon/tests/f077_margin_in_certificate.rs`,
  `lineos/m0/m0-daemon/tests/deliverable_cert_binds_the_file.rs`. Μπαίνει στο certificate
  object backend-side.
- ΔΙΑΒΑΣΤΗΚΕ `pdf_preview.rs` (Certificate modal): φέρνει δεδομένα από
  `get_golden_blob` (LUFS/TP/LRA, stem hashes, QR, Ed25519 signature) — όχι
  από το delivery/verdict handler. Ούτε εκεί εμφανίζεται verdict/advisory.

⇒ Το backend υπολογίζει πλήρη ετυμηγορία παράδοσης ανά πλατφόρμα· το UI δεν
τη διαβάζει πουθενά. Ό,τι βλέπει ο χρήστης για "συμμόρφωση" είναι μόνο τα 6
σταθερά LED του `CompliancePanel` (§3).

---

## 1 · Πίνακας: panel/component → τι δείχνει → ζωντανό;

| # | Panel/Component | Γραμμές | Τι δείχνει | Mounted στο live δέντρο; |
|---|---|---|---|---|
| 1 | `panels/session.rs` (SESSION, αριστερό MFD) | 446 | FM0 drop-idle → FM1 preset selected → MASTER button → FM5 Golden Blob badge + export (WAV/FLAC/MP3/AIFF) + "PDF REPORT" κουμπί | ✅ ΝΑΙ — `sampling_siamese.rs:94` |
| 2 | `panels/insights.rs` (SPATIAL TELEMETRY, κέντρο) | 324 | NeonCanvas (goniometer-style), stereo correlation/width/vector από `session_state.quality`· MID/SIDE μπάρες πίσω από `if false {}` | ✅ ΝΑΙ — `sampling_siamese.rs:120` (MID/SIDE block νεκρό, `insights.rs:127`) |
| 3 | `panels/mastered.rs` (MasteredView overlay) | 314 | BEFORE/AFTER waveform, LUFS/TP/LRA readout, 6 compliance LED (SPOTIFY/YOUTUBE/APPLE/TIDAL/BROADCAST/EBU R128) | ✅ ΝΑΙ, ως full-screen overlay — `app.rs:574` (toggle `show_mastered`) |
| 4 | `panels/jini_panel.rs` ("JINI"/COACH: narrative, persona selector, findings, score bar, action card) | 533 | Persona (BEGINNER/MID/PRO), narrative κείμενο, APPLY/DISMISS card, λίστα wizard findings | ❌ **ΟΧΙ** — μηδέν `JiniPanel {` έξω από το ίδιο το αρχείο. Ήδη επισημασμένο ως "ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ" στο `docs/unused-pub-audit.md` (2026-08-12, entry #2). Η τρίτη στήλη του live triptych είναι στην πραγματικότητα "DSP CHAIN" (§5). |
| 5 | `components/jini/*` (drop_zone, detection, platform_selector, flavour_selector, analysing, awaiting_more) — ΔΙΑΦΟΡΕΤΙΚΟ "JINI" από το #4 | ~150 συνολικά | Η πραγματική εισαγωγική ροή, κάτω 25% ("Hangar") | ✅ ΝΑΙ — `app.rs:598-710` |
| 6 | `components/album_matrix.rs` (AlbumMatrix — per-track batch πίνακας, status Waiting/Scanning/Processing/Certified, "get_album_certificate") | 220 | Πολλαπλά tracks με LUFS/target/ducking/width ανά track | ❌ **ΟΧΙ** — μηδέν αναφορές `AlbumMatrix` εκτός του ίδιου αρχείου. Ορφανό. |

---

## 2 · Η είσοδος αρχείου — BATCH ή ένα-ένα;

**Απάντηση: όχι πραγματικό batch.** Το UI δέχεται πολλαπλά paths στο
drag-drop επίπεδο, λέει στον χρήστη "Album, N tracks", αλλά μαστοράρει
μόνο ένα.

- ΔΙΑΒΑΣΤΗΚΕ `app.rs:417-479` — native Tauri `tauri://drag-drop` listener·
  διαβάζει array από paths (`paths_array`, μπορεί να είναι πολλά αρχεία/
  φάκελος).
- ΔΙΑΒΑΣΤΗΚΕ `app.rs:442-452` (σχόλιο μέσα στον ίδιο κώδικα): *"Keep
  dropped_path (first file) for the existing single-track flow —
  untouched behavior. dropped_paths (all files) is new: not yet consumed
  by any dispatch path... multi-file drops silently discarded all but the
  first."* → κρατά μόνο `all_paths.first()` (`app.rs:450-452`) για τη
  ροή mastering.
- ΔΙΑΒΑΣΤΗΚΕ `components/jini/detection.rs:26-33` — αν έπεσαν N αρχεία, ο
  χρήστης βλέπει **"Album. N tracks."** μετά από 1200ms fake timer
  (`detection.rs:17-18`) — και μετά προχωράει στο platform/flavour picker
  για να μαστοράρει **μόνο το track #1** (`app.rs:695`,
  `current_path = dropped_path`).
- ΔΙΑΒΑΣΤΗΚΕ `app.rs:630-638` — κουμπί "click to browse" καλεί IPC
  `open_audio_file` → επιστρέφει `Option<AudioMeta>`, δηλαδή ένα αρχείο τη
  φορά (όχι λίστα).
- Το πραγματικό batch-tracking UI (`AlbumMatrix`, πίνακας 1 της §1) υπάρχει
  ως κώδικας αλλά είναι ορφανό — δεν μπορεί να δει ο χρήστης τα άλλα N-1
  tracks που "χάθηκαν".

---

## 3 · Επιλογή προορισμού (ACX και άλλα)

- ΔΙΑΒΑΣΤΗΚΕ `state/presets.rs:39-78` — `PLATFORMS` =
  `spotify, apple_music, apple_podcast, broadcast, youtube, acx`.
- **ΕΠΑΛΗΘΕΥΤΗΚΕ: ναι, το `acx` μπήκε.** `presets.rs:73-77`, id="acx",
  label="ACX / Audiobook", lufs=-20.5· σχόλιο στο ίδιο αρχείο
  (`presets.rs:65-72`) χρονολογεί την αλλαγή 2026-08-25 και εξηγεί ότι πριν
  από αυτήν το preset υπήρχε στο CATALOGUE αλλά κανένα μενού δεν το
  πρόσφερε.
- Ορατό: ναι — κουμπί στο `JiniPlatformSelector`
  (`platform_selector.rs:16-27`, wired live `app.rs:663-669`) και ξανά ως
  ετικέτα στο post-mastering `SelectedPreset` (`session.rs:117-138`).
- **Mismatch** (ΣΥΜΠΕΡΑΣΜΑ): το `CompliancePanel` του MasteredView
  (`mastered.rs:286-293`) δείχνει LED για
  spotify/youtube/apple/**tidal**/broadcast/**ebu_r128** — tidal και
  ebu_r128 δεν είναι καν επιλέξιμοι προορισμοί, ενώ acx/apple_podcast
  (επιλέξιμοι) δεν έχουν LED καθόλου.
- "Θυμάται επιλογή": `last_platform`/`last_flavour` signals υπάρχουν
  (`app.rs:80-81`, γράφονται `app.rs:148-149`) αλλά δεν βρέθηκε ανάγνωσή
  τους για προ-συμπλήρωση επόμενης επιλογής — μοιάζει με state-mirror για
  diagnostics, όχι preference persistence.

---

## 5 · Το CERT

- ΔΙΑΒΑΣΤΗΚΕ: κουμπί "PDF REPORT" (`session.rs:405-422`) ανοίγει modal
  (`pdf_preview.rs`) με ετικέτα "CERTIFIED", QR code, stem-DNA hashes,
  Ed25519 signature preview, "EXTENDED VIEW" (forensic: full hashes,
  processing timeline), "EXPORT PNG".
- Άρα υπάρχει ρητό UI — αλλά (§0) χωρίς delivery-verdict/advisory
  περιεχόμενο μέσα του.

---

## 6 · Πρόοδος κατά τη διάρκεια

- ΔΙΑΒΑΣΤΗΚΕ `components/jini/analysing.rs` — μόνο **κειμενική ετικέτα**
  ανά στάδιο ("Decoding raw audio data.", "Reading the color of your
  sound.", "Isolating structural stems.", "Mapping the stereo field.",
  "Calibrating dynamics. Securing ceiling.", "Certified."). Καμία μπάρα,
  κανένα ποσοστό.
- ΔΙΑΒΑΣΤΗΚΕ `app.rs:483-512` — τα στάδια μπαίνουν σε ουρά
  (`stage_queue`) και αδειάζουν ένα κάθε 500ms με client-side timer· **δεν**
  είναι 1-προς-1 συγχρονισμένα με το πραγματικό elapsed του backend.
- ΔΙΑΒΑΣΤΗΚΕ `app.rs:369-376` — το πραγματικό `mastering://progress` event
  γεμίζει `journey_elapsed_ms`, αλλά αυτό περνάει ως prop στο
  `SamplingSiamese` (`sampling_siamese.rs:27`) **χωρίς να διαβάζεται ποτέ
  μέσα στο σώμα του component** (επιβεβαιώθηκε με grep — καμία άλλη
  αναφορά). Νεκρό prop· ο αριθμός δεν εμφανίζεται πουθενά.
- Ανά αρχείο ή συνολικά: αφού επεξεργάζεται μόνο ένα αρχείο τη φορά (§2),
  δεν υπάρχει σήμερα αντικείμενο για "συνολική" πρόοδο άλμπουμ.

---

## 7 · Τι είναι το jini_panel (533 γρ.)

Βλ. γραμμή 4 του πίνακα (§1). Είναι το πλήρες "COACH" UI (persona
narrative, findings, score bar, action card) αλλά **δεν είναι mounted
πουθενά** — μηδέν `JiniPanel {` έξω από το ίδιο αρχείο. Ήδη καταγεγραμμένο
ως "ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ" στο `docs/unused-pub-audit.md` (2026-08-12).

Το "JINI" που πραγματικά βλέπει ο χρήστης σήμερα είναι μια **διαφορετική,
πολύ πιο λιτή** ομάδα components (`components/jini/*`, drop_zone →
detection → platform_selector → flavour_selector → analysing), χωρίς
persona selector, χωρίς findings list, χωρίς score bar. Τα δύο "JINI" δεν
πρέπει να συγχέονται στο PRD.

---

## 8 · Πλάγιο εύρημα — DSP CHAIN panel είναι demo, όχι live δεδομένα

ΔΙΑΒΑΣΤΗΚΕ `sampling_siamese.rs:145-173` — η τρίτη (δεξιά) στήλη του
κύριου triptych, τίτλος **"DSP CHAIN"**, δείχνει EQ/Compressor/Limiter/
Saturation καμπύλες με **hardcoded literal τιμές** μέσα στο ίδιο το rsx!
(`low_db: 2.5`, `threshold_db: -18.0`, `ceiling_dbtp: -1.0`,
`thd_percent: 1.8`, κλπ) — ίδιες σε κάθε session, ανεξάρτητα από το
πραγματικό `session_state`.

Το πραγματικό `DspChainStateJson` από το backend (`types.rs:209`)
χρησιμοποιείται μόνο για 3 μικρά annunciator dots (EQ/COMP/SAT) στο
transport bar (`transport_bar.rs:168-172, 227-229`) — όχι για το panel με
τις καμπύλες.

---

## Σύνοψη για τον συγγραφέα του PRD

Το πραγματικό, ζωντανό user journey σήμερα είναι:

1. Drop/browse **ένα** αρχείο (ακόμη κι αν έπεσαν πολλά, ή/και ακόμη κι αν
   εμφανίστηκε "Album. N tracks.").
2. Επιλογή πλατφόρμας (6 επιλογές, incl. ACX) → επιλογή flavour (6
   επιλογές) → mastering.
3. Πρόοδος: μόνο κειμενικές ετικέτες σταδίου, χωρίς αριθμό/μπάρα.
4. Αποτέλεσμα: SESSION panel (export controls) + INSIGHTS panel (stereo
   telemetry) + ένα demo "DSP CHAIN" panel που δεν αλλάζει ποτέ.
5. Compliance: 6 σταθερά LED σε ξεχωριστό MasteredView overlay — δεν
   αντιστοιχούν στη λίστα επιλέξιμων πλατφορμών.
6. Certificate PDF διαθέσιμο κατ' αίτηση — LUFS/TP/LRA + hashes + QR, όχι
   verdict/advisory.
7. Ό,τι κρίση παράδοσης (`DeliveryVerdict`/`delivery_checks`) υπολογίζει
   το backend, δεν φτάνει ποτέ σε αυτό το UI.
8. Δύο ξεχωριστά, ασύνδετα "JINI": το ζωντανό wizard (λιτό) και το
   ορφανό πλήρες coach panel (533 γρ., μη mounted) — μαζί με το επίσης
   ορφανό `AlbumMatrix` (220 γρ.).

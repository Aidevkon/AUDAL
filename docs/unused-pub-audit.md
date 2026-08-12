# Unused pub audit — 2026-08-12

## Πώς παρήχθη

- **Εντολή:** `cargo workspace-unused-pub`
- **Έκδοση εργαλείου:** `cargo-workspace-unused-pub 0.1.0`
- **Φιλτράρισμα:** Από τα 353 αποτελέσματα που επέστρεψε το εργαλείο σε όλο το workspace, απομονώθηκαν τα 313 που βρίσκονται σε αρχεία δοκιμών (`tests/`, `#[test]`) ως false positives. Ερευνήθηκαν εξαντλητικά οι 40 δημόσιες συναρτήσεις/δομές που βρίσκονται στον παραγωγικό κώδικα (`src/`).

---

## Σύνοψη

| Κατηγορία | Πλήθος |
|---|---|
| **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ** | 15 |
| **Β · ΕΡΓΑΛΕΙΑ ΑΞΙΟΛΟΓΗΣΗΣ** | 2 |
| **Γ · ΔΗΜΟΣΙΟ API** | 19 |
| **Δ · ΝΕΚΡΟ** | 4 |
| **Ε · ΑΓΝΩΣΤΟ** | 0 |

### Υποψήφιοι Κατηγορίας Α (Ασύνδετη Πρόθεση)
1. `swap_graph` (`apps/runtime/loom/src/stem_engine.rs`)
2. `JiniPanel` (`apps/stillair/cockpit-dioxus/src/panels/jini_panel.rs`)
3. `filter_findings` (`apps/stillair/cockpit-dioxus/src/wizard/mod.rs`)
4. `record_intent` (`integration/src/proof_log.rs`)
5. `record_zone_adj` (`integration/src/proof_log.rs`)
6. `record_feedback` (`lineos/m0/m0-daemon/src/agents/corpus_agent.rs`)
7. `create_commit` (`lineos/m0/m0-daemon/src/agents/git_agent.rs`)
8. `update_persona` (`lineos/m0/m0-daemon/src/agents/persona_agent.rs`)
9. `jini_suggest_album_name` (`lineos/m0/m0-daemon/src/jini/mod.rs`)
10. `apply_output_makeup_no_clip` (`lineos/m1/sp314-dsp/src/pipeline/gain.rs`)
11. `is_wide` (`lineos/m1/sp314-dsp/src/transforms/pca.rs`)
12. `dominance` (`lineos/m1/sp314-dsp/src/transforms/pca.rs`)
13. `OversampledSoftClipper` (`lineos/m1/sp314-dsp/src/limiter/clipper.rs`)
14. `measure_clipping_ratio_post_process` (`lineos/m1/sp314-dsp/src/pipeline/autotune.rs`)
15. `write_fatal` (`lineos/m0/m0-daemon/src/audit.rs`)

---

## Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ

### 1. `swap_graph`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn swap_graph(&mut self, mut new_graph: DspGraph) {
      new_graph.reset();
      self.graph = new_graph;
  }
  ```
  Επαναφέρει την κατάσταση του νέου `DspGraph` και αντικαθιστά το ενεργό DSP γράφημα στο `StemEngine`.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `ΑΓΝΩΣΤΟ`
  - Commit message: `ΑΓΝΩΣΤΟ`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "swap_graph"`: 0 κλήσεις εκτός της δήλωσης στο `apps/runtime/loom/src/stem_engine.rs`.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Μέθοδος θερμής αντικατάστασης γραφήματος ανά stem. Το `LoomEngine` εκθέτει μόνο μεταλλαγή παραμέτρων ανά stem, αφήνοντας την αντικατάσταση γραφήματος ασύνδετη.

---

### 2. `JiniPanel`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  #[component]
  pub fn JiniPanel(
      mode: Signal<CockpitMode>,
      session_state: Signal<Option<SessionStateJson>>,
      wizard_findings: ReadOnlySignal<Vec<crate::wizard::WizardFinding>>,
      jini_suggestion: Signal<Option<JiniSuggestionJson>>,
      jini_persona: Signal<JiniPersonaState>,
  ) -> Element
  ```
  Dioxus UI component που αποδίδει την ενότητα αφήγησης JINI, επιλογέα persona, κάρτες ενεργειών (Apply/Dismiss) και τη λίστα ευρημάτων.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments:
    ```rust
    //! panels/coach.rs — JINI Panel (personality narrative + findings) · J-P5
    //! Authority: JINI Spec v1.0 §7 · Phase 11 task-decomposition P11-007
    ```
  - Commit message: `feat(cockpit): P11-001 through P11-008`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "JiniPanel"`: 0 κλήσεις στο Rust UI code.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Κατασκευάστηκε για το JINI narrative UI στο Phase 11, αλλά το `app.rs` αποδίδει απευθείας κάρτες `HangarInterviewState` παρακάμπτοντας το panel.

---

### 3. `filter_findings`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn filter_findings<'a>(
      &self,
      findings: &'a [WizardFinding],
      now_ms: f64,
  ) -> Vec<&'a WizardFinding> {
      findings
          .iter()
          .filter(|f| self.is_cooled_down(f.id, now_ms))
          .collect()
  }
  ```
  Φιλτράρει μια σειρά ευρημάτων Wizard βάσει του cooldown tracker (45 δευτερόλεπτα).
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments:
    ```rust
    /// Finding cooldown tracker.
    /// Prevents same finding from re-triggering before cooldown expires.
    /// Relevant only when real-time telemetry polling is added.
    /// Authority: Wizard Constitution v1.1 §6 — 45 second cooldown.
    ```
  - Commit message: `feat(cockpit): P11-001 through P11-008`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "filter_findings"`: 0 κλήσεις στον κώδικα.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Σχεδιάστηκε για το real-time telemetry polling βάσει του Wizard Constitution v1.1 §6, το οποίο δεν έχει συνδεθεί ακόμη στο UI loop.

---

### 4. `record_intent`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn record_intent(&mut self, intent: &Intent) {
      self.intent = Some(intent.clone());
  }
  ```
  Αποθηκεύει το `Intent` στο `ProofLog`.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments:
    ```rust
    // integration/src/proof_log.rs — ProofLog stub
    // Authority: spec/locked/S-009_integration_firewall.md v1.0
    // Full implementation in S-010 (Execution Proof).
    ```
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "record_intent"`: 0 κλήσεις στον κώδικα παραγωγής (υπάρχει μόνο στη δήλωση και στο spec S-010).
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Hook καταγραφής intent για το σύστημα Proof Log (S-010) που δεν καλείται από τα pipeline nodes του `m0-daemon`.

---

### 5. `record_zone_adj`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn record_zone_adj(&mut self, zones: &ZoneAdjustments) {
      self.zone_adj = Some(zones.clone());
  }
  ```
  Αποθηκεύει τα `ZoneAdjustments` στο `ProofLog`.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments:
    ```rust
    // integration/src/proof_log.rs — ProofLog stub
    // Authority: spec/locked/S-009_integration_firewall.md v1.0
    ```
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "record_zone_adj"`: 0 κλήσεις στον κώδικα παραγωγής.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Hook καταγραφής zone adjustments για το Proof Log (S-010) που δεν καλείται από τον DSP επεξεργαστή.

---

### 6. `record_feedback`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub async fn record_feedback(
      _db: &DbConn,
      _user_id: &str,
      _commit_id: &str,
      _finding_type: &str,
      _action: CorpusAction,
      _delta: Option<f32>,
  ) -> Result<(), String> {
      Ok(())
  }
  ```
  Stub συνάρτηση καταγραφής feedback χρήστη (Apply, Ignore, Cancelled, Reverted) στη βάση δεδομένων.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments:
    ```rust
    // corpus_agent::get_finding_score not yet implemented (dormant — wired in Corpus Phase)
    ```
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "record_feedback"`: 0 κλήσεις στον κώδικα παραγωγής.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Stub agent για το Corpus Phase που υποδέχεται feedback χρήστη, αλλά δεν έχει συνδεθεί με τα API endpoints του `m0-daemon`.

---

### 7. `create_commit`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub async fn create_commit(
      _db: &DbConn,
      project_id: &str,
      branch: &str,
      dsp_state: &DspStateJson,
      message: &str,
  ) -> Result<MixCommit, String> {
      let ts = now_unix_ms();
      let hash = generate_hash(project_id, ts, dsp_state);

      Ok(MixCommit {
          id: None,
          project_id: project_id.to_string(),
          hash,
          parent_hash: None,
          branch_name: branch.to_string(),
          message: message.to_string(),
          dsp_state: dsp_state.clone(),
          blob_id: None,
          timestamp: ts,
      })
  }
  ```
  Δημιουργεί δομή `MixCommit` με blake3 hash του DSP state.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `git_agent::checkout not yet implemented (dormant — wired in Git Phase)`
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "create_commit"`: 0 κλήσεις στον κώδικα.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Συνάρτηση του Git Agent για τη δημιουργία commits ιστορικού μίξης, σε αναμονή της πλήρους ενοποίησης του Git Phase.

---

### 8. `update_persona`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub async fn update_persona(
      _db: &DbConn,
      _user_id: &str,
      sessions_completed: u32,
  ) -> Result<String, String> {
      let persona = match sessions_completed {
          0..=4 => "beginner",
          5..=19 => "intermediate",
          _ => "pro",
      };
      Ok(persona.to_string())
  }
  ```
  Υπολογίζει το persona του χρήστη βάσει ολοκληρωμένων συνεδριών.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `ΑΓΝΩΣΤΟ`
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "update_persona"`: 0 κλήσεις στον κώδικα.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Stub agent για την ενημέρωση προφίλ χρήστη, μη συνδεδεμένος με τη ροή onboarding.

---

### 9. `jini_suggest_album_name`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub async fn jini_suggest_album_name(
      _tracks: &[String],
      _genre_hint: Option<&str>,
  ) -> Result<Vec<String>, String> {
      Ok(vec![
          "Sonic Horizons".into(),
          "Midnight Sessions".into(),
          "Ethereal Tapes".into(),
      ])
  }
  ```
  Επιστρέφει προτάσεις ονομάτων άλμπουμ από το JINI AI.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `//! JINI AI module — narrative & suggest engine. Authority: JINI Spec v1.0`
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "jini_suggest_album_name"`: 0 κλήσεις στον κώδικα.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Μηχανή προτάσεων JINI που δεν έχει συνδεθεί με τα UI panels ή τα HTTP handlers.

---

### 10. `apply_output_makeup_no_clip`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  #[inline]
  pub fn apply_output_makeup_no_clip(&self, left: &mut [f32], right: &mut [f32]) {
      for i in 0..left.len() {
          left[i] *= self.makeup_linear;
          right[i] *= self.makeup_linear;
      }
  }
  ```
  Εφαρμόζει makeup gain στα δείγματα ΧΩΡΙΣ περιορισμό/ψαλίδισμα (hard clip) στο `[-1.0, 1.0]`.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `ΑΓΝΩΣΤΟ`
  - Commit message: `feat(dsp): Phase 2 — sp314-dsp mastering engine`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "apply_output_makeup_no_clip"`: 0 κλήσεις εκτός της δήλωσής του στο `gain.rs`.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ** — και ΟΧΙ επειδή η παραγωγή προτίμησε την clipping εκδοχή.

  ΜΕΤΡΗΜΕΝΟ: ΚΑΜΙΑ από τις δύο δεν καλείται εκτός tests. Ολόκληρο το GainStage παρακάμφθηκε.

  Το makeup gain υλοποιείται ΞΑΝΑ, inline, στο dsp/mod.rs ως correction_linear — η τιμή που τυπώνει το [W17-LUFS-CORR]. Δύο υλοποιήσεις του ίδιου πράγματος: η μία ονομασμένη και νεκρή σε δικό της module, η άλλη ζωντανή και ανώνυμη μέσα σε 50 γραμμές post-processing.

  ΙΔΙΟ ΣΧΗΜΑ με το FiveDotOneStage::render vs render_chunk — διπλή υλοποίηση όπου η μία αποκλίνει σιωπηλά.

---

### 11. `is_wide`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  /// Is signal wide? (low correlation)
  pub fn is_wide(&self) -> bool {
      self.correlation.abs() < 0.1
  }
  ```
  Ελέγχει αν το σήμα έχει χαμηλή στερεοφωνική συσχέτιση (πολύ πλατύ πεδίο).
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Is signal wide? (low correlation)`
  - Commit message: `feat(sp314-dsp): constitutional compliance + v0.3.0 foundation`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "is_wide"`: 0 κλήσεις στον κώδικα. Το `spatial/mod.rs` καλεί την `pca_spatial` και διαβάζει τα `pc1_ratio` και `ms_angle_rad`, αλλά αγνοεί την μέθοδο `is_wide()`.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Δομικός δείκτης πλατύτητας PCA που υπολογίζεται στο `PcaSpatialResult` αλλά δεν καταναλώνεται από τους αλγόριθμους τοποθέτησης stem.

---

### 12. `dominance`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  /// Energy split: how much in dominant axis
  pub fn dominance(&self) -> f32 {
      self.pc1_ratio
  }
  ```
  Επιστρέφει το ποσοστό ενέργειας στον κύριο άξονα PCA.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Energy split: how much in dominant axis`
  - Commit message: `feat(sp314-dsp): constitutional compliance + v0.3.0 foundation`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "dominance"`: 0 κλήσεις στον κώδικα.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Helper accessor ποσοστού ενέργειας στον κύριο άξονα PCA που δεν καταναλώνεται από το spatial node.

---

### 13. `OversampledSoftClipper`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub struct OversampledSoftClipper {
      up_l: PolyphaseUpsampler4x,
      up_r: PolyphaseUpsampler4x,
      down_l: PolyphaseDownsampler4x,
      down_r: PolyphaseDownsampler4x,
      enabled: bool,
  }
  ```
  4x oversampled soft clipper με πολυφασικά φίλτρα για εξάλειψη aliasing.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// 4x Oversampled Soft Clipper (polyphase FIR implementation)`
  - Commit message: `feat(sp314-dsp): constitutional compliance + v0.3.0 foundation`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "OversampledSoftClipper"`: Εξάγεται στο `limiter/mod.rs` και δοκιμάζεται στο `tests/clipper_contract.rs`, αλλά **ΔΕΝ καλείται στην παραγωγική αλυσίδα mastering** (όπου χρησιμοποιείται το `BrickwallLimiter`).
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**. Πλήρως υλοποιημένο και δοκιμασμένο DSP module παραμόρφωσης/soft clipping, το οποίο αντικαταστάθηκε/παραλείφθηκε από το κύριο pipeline render.

---

### 14. `measure_clipping_ratio_post_process`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn measure_clipping_ratio_post_process(left: &[f32], right: &[f32]) -> f32 {
      let total = (left.len() + right.len()) as f32;
      if total == 0.0 {
          return 0.0;
      }
      let mut over = 0usize;
      for i in 0..left.len() {
          if libm::fabsf(left[i]) >= 0.9999_f32 {
              over += 1;
          }
          if libm::fabsf(right[i]) >= 0.9999_f32 {
              over += 1;
          }
      }
      over as f32 / total
  }
  ```
  Μετράει το ποσοστό των δειγμάτων που τερμάτισαν στο ceiling (`>= 0.9999`) μετά την επεξεργασία.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments:
    ```rust
    /// Count samples that hit the Engine's hard clipper (>= 0.9999) AFTER processing.
    ///
    /// WHY post-process: measuring on raw signal × makeup_linear ignores
    /// EQ boosts, compressor gain reduction, and parallel mix energy addition.
    /// The engine hard-clips at ±1.0 — saturated samples land at exactly 1.0.
    /// Threshold 0.9999 catches these without false positives on legitimate peaks.
    ```
  - Commit message: `feat(dsp): Phase 2 — sp314-dsp mastering engine`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "measure_clipping_ratio_post_process"`: 0 κλήσεις στον κώδικα.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**.

  ΔΕΝ απαιτεί ground truth — τρέχει στο τελικό σήμα. Ο υποψήφιος καταναλωτής είναι το CERTIFICATE: ένας narrator ή μουσικός που διαβάζει clipping_ratio ξέρει αν το υλικό του παραμορφώθηκε, κάτι που σήμερα δεν του λέει κανείς.

  Το doc comment δείχνει ότι κάποιος έκανε το λάθος πρώτα και μετά κατάλαβε γιατί:
    "WHY post-process: measuring on raw signal × makeup_linear ignores EQ boosts, compressor gain reduction, and parallel mix energy addition."

  ΜΕΤΡΗΜΕΝΟ 2026-08-12: το [W17-LUFS-CORR] έδειξε peak_raw_db=3.3151 πριν το limiting — 3.3 dB πάνω από το πλήρες κλίμακα. Αυτή η συνάρτηση θα έλεγε αν κάτι ψαλιδίστηκε.

---

### 15. `write_fatal`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn write_fatal(&self, entry: AuditEntry) {
      if let Err(e) = self.write(entry) {
          eprintln!("FATAL: Audit write failure — M0 must halt: {e}");
          std::process::exit(1);
      }
  }
  ```
  Εγγράφει εγγραφή ελέγχου και πραγματοποιεί `process::exit(1)` αν αποτύχει η εγγραφή.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Convenience: write audit entry — panics on failure (fatal by design).`
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "write_fatal"`: 0 κλήσεις (ο κώδικας καλεί απευθείας την `write()` διαχειριζόμενος τα errors).
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Α · ΑΣΥΝΔΕΤΗ ΠΡΟΘΕΣΗ**.

  Το σχόλιο λέει "fatal by design" και ο κώδικας κάνει process::exit(1). Δηλαδή κάποιος αποφάσισε ότι ο δαίμονας ΠΡΕΠΕΙ ΝΑ ΣΤΑΜΑΤΗΣΕΙ αν δεν μπορεί να γράψει audit — μια εγγύηση που δεν επιβάλλεται πουθενά.

  Η παραγωγή καλεί την write() και χειρίζεται το σφάλμα τοπικά, δηλαδή συνεχίζει χωρίς audit trail. Αυτό είναι ΑΚΡΙΒΩΣ το αντίθετο της πρόθεσης.

  Δεν είναι convenience method. Είναι invariant που δεν συνδέθηκε.

---

## Β · ΕΡΓΑΛΕΙΑ ΑΞΙΟΛΟΓΗΣΗΣ

### 1. `all_above`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn all_above(&self, threshold_db: f32) -> bool {
      self.drums > threshold_db
          && self.bass > threshold_db
          && self.harmonics > threshold_db
          && self.ambience > threshold_db
  }
  ```
  Ελέγχει αν το SDR (Signal-to-Distortion Ratio) και των 4 stems ξεπερνά ένα κατώφλι dB.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments:
    ```rust
    //! SDR — Signal-to-Distortion Ratio for stem separation quality.
    //! Authority: NMF Upgrade Plan v2.1 NMF-V2-P5
    //! Gate: SDR > 6dB per stem = acceptable separation quality.
    ```
  - Commit message: `feat(sp314-dsp): constitutional compliance + v0.3.0 foundation`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "all_above"`: 0 κλήσεις στον κώδικα παραγωγής.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Β · ΕΡΓΑΛΕΙΟ ΑΞΙΟΛΟΓΗΣΗΣ**. Χρειάζεται ground truth reference stems για να υπολογίσει το SDR, κάτι που δεν υπάρχει κατά το runtime mastering, παρά μόνο σε offline benchmark evaluation.

---

### 2. `bass_drums_distance`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn bass_drums_distance(&self) -> f32 {
      let diff = self.bass_transient_density - self.drums_transient_density;
      libm::fabsf(diff)
  }
  ```
  Υπολογίζει την απόσταση πυκνότητας transients μεταξύ μπάσου και κρουστών.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Transient density separation metrics`
  - Commit message: `feat(sp314-dsp): constitutional compliance + v0.3.0 foundation`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "bass_drums_distance"`: 0 κλήσεις στον κώδικα.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Β · ΕΡΓΑΛΕΙΟ ΑΞΙΟΛΟΓΗΣΗΣ**. Διαγνωστική μετρική διαχωρισμού οργάνων στη χαμηλή περιοχή συχνοτήτων.

---

## Γ · ΔΗΜΟΣΙΟ API

### 1. `seek_to_ms`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn seek_to_ms(&mut self, position_ms: f64)
  ```
  Μετατοπίζει τη θέση αναπαραγωγής της μηχανής Loom στο καθορισμένο millisecond.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: Telemetry & transport API specification (`lineos/docs/telemetry-bridge-spec-v1_2.md`).
  - Commit message: `feat(loom): WebAudio worklet engine`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "seek_to_ms"`: **ΚΑΛΕΙΤΑΙ** στο `apps/runtime/loom/js/worklet.js:52` (`this.engine.seek_to_ms(data.position_ms);`).
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API** (FALSE POSITIVE του εργαλείου - καλείται από το JavaScript AudioWorklet μέσω WASM FFI bindings).

---

### 2. `current_position_ms`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn current_position_ms(&self) -> f64
  ```
  Επιστρέφει την τρέχουσα θέση αναπαραγωγής σε milliseconds.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: Telemetry & transport API specification.
  - Commit message: `feat(loom): WebAudio worklet engine`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "current_position_ms"`: Εξάγεται μέσω `#[wasm_bindgen]` στα TS/JS WASM bindings (`e11.d.ts`, `e11.js`).
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API** (FALSE POSITIVE του εργαλείου - WASM FFI getter για το UI/telemetry host).

---

### 3. `is_interactive`, `is_mastering`, `has_file`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn is_interactive(&self) -> bool
  pub fn is_mastering(&self) -> bool
  pub fn has_file(&self) -> bool
  ```
  Μέθοδοι ελέγχου κατάστασης στο enum `CockpitMode`.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments:
    ```rust
    /// UI interactions allowed in all modes except Mastering, Exporting, Fault.
    /// True if currently mastering — disables transport controls.
    /// True if a file is loaded (any state after FM0 except Fault).
    ```
  - Commit message: `feat(cockpit): P11-001 through P11-008`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "is_interactive"`: 0 κλήσεις (τα components χρησιμοποιούν inline `matches!` patterns).
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Δημόσιοι accessors ελέγχου κατάστασης FSM enum.

---

### 4. `get_lame_version`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn get_lame_version() -> *const std::os::raw::c_char;
  ```
  Raw FFI binding στην C συνάρτηση `get_lame_version()` της `libmp3lame`.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Get LAME version string (for logging / compliance output).`
  - Commit message: `feat(export): Phase 13 — MP3 (LAME dynamic)`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "get_lame_version"`: 0 κλήσεις στο Rust code εκτός της FFI δήλωσης.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. FFI binding χαμηλού επιπέδου στο `-sys` crate.

---

### 5. `total_sessions`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn total_sessions(&self) -> usize
  ```
  Επιστρέφει το μέγιστο πλήθος συνεδριών εκπαίδευσης στο `UserMarkovModel`.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Total sessions trained across all presets.`
  - Commit message: `feat(corpus): User Markov model store`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "total_sessions"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Δημόσιος getter στατιστικών εκπαίδευσης corpus.

---

### 6. `resolve_persona`, `is_completed`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn resolve_persona(&mut self)
  pub fn is_completed(&self) -> bool
  ```
  Μέθοδοι διαχείρισης κατάστασης onboarding (`OnboardingState`).
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `//! Onboarding state contracts. Authority: hangar-onboarding-spec v1.0`
  - Commit message: `feat(types): Onboarding contracts`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "resolve_persona"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Μέθοδοι συμβολαίου του μοντέλου onboarding.

---

### 7. `issues_at_least`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn issues_at_least(&self, min: &Severity) -> Vec<&Issue>
  ```
  Επιστρέφει τα ευρήματα του Rule Engine που ισούνται ή ξεπερνούν ένα επίπεδο σοβαρότητας.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Returns all issues at or above the given severity level.`
  - Commit message: `feat(rule-engine): Phase 4 — deterministic coach-core`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "issues_at_least"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Δημόσια μέθοδος φιλτραρίσματος ευρημάτων για εξωτερικούς καταναλωτές.

---

### 8. `is_certified`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn is_certified(&self) -> bool {
      matches!(self.variant, StoredBlobVariantV2::Certified { .. })
  }
  ```
  Ελέγχει αν το `StoredBlobV2` είναι πιστοποιημένο.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Returns true if this blob is certified (signed by M0 authority).`
  - Commit message: `feat(m0): StoredBlobV2 refactor`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "is_certified"`: 0 κλήσεις. Το `write_sidecar` και τα handlers χρησιμοποιούν την `uncertified_reason()`.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Boolean accessor του `StoredBlobV2`.

---

### 9. `await_health_gate`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub async fn await_health_gate(gate: &HealthGate) -> anyhow::Result<()>
  ```
  Αναμένει την ολοκλήρωση του ελέγχου υγείας του δαίμονα.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Await until health gate is open`
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "await_health_gate"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Helper συνάρτηση αναμονής health gate.

---

### 10. `check_inbound`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn check_inbound(&self, path: &str) -> PolicyDecision
  ```
  Ελέγχει αν μια εισερχόμενη διαδρομή αρχείου επιτρέπεται από την πολιτική ασφαλείας M0.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Check inbound file path policy`
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "check_inbound"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Συνάρτηση ελέγχου policy engine.

---

### 11. `registry_data`, `checksums_data`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn registry_data(&self) -> &M0RegistryJson
  pub fn checksums_data(&self) -> &ChecksumsJson
  ```
  Accessors δεδομένων μητρώου και checksums.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Registry data accessor`
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "registry_data"`, `grep -rn "checksums_data"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Accessors του `RegistryState`.

---

### 12. `measure_true_peak_dbtp_mono`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn measure_true_peak_dbtp_mono(signal: &[f32]) -> f32 {
      let mut detector = TruePeakDetector::new();
      let mut max_tp = 0.0_f32;
      for &s in signal {
          let tp = detector.process(s, s);
          if tp > max_tp {
              max_tp = tp;
          }
      }
      if max_tp > 1e-10 {
          20.0 * libm::log10f(max_tp)
      } else {
          -144.0
      }
  }
  ```
  Μονοφωνική παραλλαγή μέτρησης True Peak σε dBTP.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Mono variant — feeds the same sample to both detector channels.`
  - Commit message: `feat(sp314-dsp): constitutional compliance + v0.3.0 foundation`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "measure_true_peak_dbtp_mono"`: 0 κλήσεις (χρησιμοποιείται η στερεοφωνική `measure_true_peak_dbtp`).
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Παράλληλη utility συνάρτηση μέτρησης για μονοφωνικά buffers.

---

### 13. `setup_streams`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn setup_streams(...)
  ```
  Ρυθμίζει τα cpal audio input/output streams.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: Real-time Audio I/O setup module.
  - Commit message: `feat(sp314-dsp): constitutional compliance + v0.3.0 foundation`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "setup_streams"`: 0 κλήσεις στον παραγωγικό αλγόριθμο mastering.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Συνάρτηση αρχικοποίησης για το real-time I/O subsystem.

---

### 14. `set_node_glide_ms`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn set_node_glide_ms(&mut self, node_id: &str, glide_ms: f32) -> Result<(), GraphError>
  ```
  Ορίζει τον χρόνο ολίσθησης (glide) παραμέτρων για ένα DSP node στο `sp314-nodes`.
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Set parameter glide time for node`
  - Commit message: `feat(sp314-nodes): DspGraph infrastructure`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "set_node_glide_ms"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Γ · ΔΗΜΟΣΙΟ API**. Μέθοδος ρύθμισης smoothing παραμέτρων του `DspGraph`.

---

## Δ · ΝΕΚΡΟ

### 1. `ReadoutItem`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  #[component]
  pub fn ReadoutItem(label: String, value: String, lit: bool) -> Element {
      rsx! {
          div { class: "cr",
              span { class: "cr-l", "{label}" }
              span { class: if lit { "cr-v lit" } else { "cr-v" }, "{value}" }
          }
      }
  }
  ```
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: None.
  - Commit message: `feat(cockpit): P11-001 through P11-008` (Task 6.4 shared readout component).
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "ReadoutItem"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Δ · ΝΕΚΡΟ**. Αντικαταστάθηκε από απευθείας inline `span` στοιχεία στα υπο-οντότητες του UI (`eq.rs`, `compressor.rs`, `limiter.rs`, `sat.rs`).

---

### 2. `autotune_dsp`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn autotune_dsp(audio: &StereoBuffer, intent: &MasteringIntent) -> AutotuneResult
  ```
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `//! Autotune DSP bridge`
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "autotune_dsp"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Δ · ΝΕΚΡΟ**. Παλαιός μηχανισμός autotune του m0-daemon που αντικαταστάθηκε από την υλοποίηση `sp314_dsp::pipeline::autotune::autotune`.

---

### 3. `wav_to_raw_pcm`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn wav_to_raw_pcm(wav_path: &str, raw_path: &std::path::Path) -> Result<(usize, u32), String>
  ```
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Convert WAV file to raw f32 PCM samples`
  - Commit message: `feat(m0): Phase 1 — M0 Moat complete`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "wav_to_raw_pcm"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Δ · ΝΕΚΡΟ**. Πρώιμος helper μετατροπής WAV σε raw f32, ο οποίος αντικαταστάθηκε από τις ροές `ManagedPcm` και `symphonia` decoders.

---

### 4. `inverse_chunk`
- **1. ΤΙ ΚΑΝΕΙ:**
  ```rust
  pub fn inverse_chunk(&mut self, frames: &[Vec<Complex<f32>>], original_len: usize) -> Vec<f32>
  ```
- **2. ΓΙΑΤΙ ΓΡΑΦΤΗΚΕ:**
  - Doc comments: `/// Chunked ISTFT synthesis`
  - Commit message: `feat(sp314-dsp): constitutional compliance + v0.3.0 foundation`
- **3. ΕΙΝΑΙ ΟΝΤΩΣ ΑΚΛΗΤΟ:**
  - `grep -rn "inverse_chunk"`: 0 κλήσεις.
- **4. ΚΑΤΗΓΟΡΙΑ & ΑΙΤΙΟΛΟΓΗΣΗ:**
  **Δ · ΝΕΚΡΟ**. Αντικαταστάθηκε από την inline ISTFT σύνθεση στο `two_pass.rs`. Επιπλέον, το drums stem παρακάμπτει πλήρως το ISTFT εκτελώντας πολλαπλασιασμό στον χρόνο (`data.core_chunk * drums_weights`).

---

## Ε · ΑΓΝΩΣΤΟ

*Καμία συνάρτηση δεν εμπίπτει σε αυτή την κατηγορία.*

---

## ΤΟ ΜΟΤΙΒΟ ΤΗΣ ΔΙΠΛΗΣ ΥΛΟΠΟΙΗΣΗΣ

Τρεις περιπτώσεις σε αυτό το πέρασμα όπου το ΙΔΙΟ πράγμα υλοποιείται δύο φορές, και η ονομασμένη εκδοχή είναι η νεκρή:

| ονομασμένο, νεκρό | ζωντανό, ανώνυμο |
|---|---|
| `GainStage::apply_output_makeup*` | `correction_linear` inline στο dsp/mod.rs |
| `StftProcessor::inverse_chunk` | inline ISTFT στο two_pass.rs |
| `measure_true_peak_dbtp_mono` | `TruePeakMeter` με `process_chunk(m, m)` στο export.rs |

ΚΑΙ ΜΙΑ ΤΕΤΑΡΤΗ ΕΚΤΟΣ ΑΥΤΗΣ ΤΗΣ ΛΙΣΤΑΣ:
`FiveDotOneStage::render` (batch, συμμετρικό) έναντι `render_chunk` (ασύμμετρο μετά το 41710dd). Εδώ ΚΑΙ ΤΑ ΔΥΟ καλούνται — το πρώτο για τον firewall, το δεύτερο για το σήμα — και αποκλίνουν ΗΔΗ.

ΓΙΑΤΙ ΣΥΜΒΑΙΝΕΙ: όταν μια συνεδρία χρειάζεται λειτουργία που υπάρχει αλλού, είναι φθηνότερο να τη γράψει inline παρά να βρει και να συνδέσει την υπάρχουσα. Το αποτέλεσμα δουλεύει, τα tests περνάνε, και δύο πηγές αλήθειας αρχίζουν να αποκλίνουν.

Το `code-graph clones` υποτίθεται ότι βρίσκει τέτοια ζευγάρια. ΑΔΟΚΙΜΑΣΤΟ σε αυτό το repo.

---

## Τι ΔΕΝ κάλυψε αυτό το πέρασμα

Το εργαλείο `cargo workspace-unused-pub` εντοπίζει **αποκλειστικά δημόσιες συναρτήσεις και δομές (`pub fn`, `pub struct`)** που δεν έχουν εμφανείς κλήσεις εκτός του module τους.

**ΔΕΝ εντοπίζει:**
1. **Αχρησιμοποίητα Πεδία (`pub` struct fields):** Παράδειγμα το `ChannelAssignment.side_weight`, το οποίο ήταν πεδίο δομής, υπολογιζόταν κανονικά, περνούσε από struct σε struct, αλλά κανείς δεν διάβαζε την τιμή του.
2. **Σταθερές (`pub const`):** Σταθερές που ορίστηκαν αλλά δεν διαβάζονται σε κανέναν υπολογισμό.
3. **Τιμές που πετιούνται σιωπηρά (Dropped Data / Partial Conversions):** Πεδία που μεταφέρονται κατά 50% σε μετατροπές `From`/`Into` (όπως η μετατροπή `DeliverySpec` -> `LoudnessTarget` που πετάει 2 από τα 6 πεδία).
4. **WASM / FFI Bindings False Positives:** Το εργαλείο σημαίνει ως "αχρησιμοποίητες" συναρτήσεις όπως η `seek_to_ms` και η `current_position_ms`, οι οποίες καλούνται δυναμικά από το JavaScript AudioWorklet runtime ή εξάγονται στο WASM ABI.

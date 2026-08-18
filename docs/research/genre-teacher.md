# Research Log — Genre Teacher & Distillation

## 1. Genre Lane & Feature Distillation Limits

* **Feature Vector (57 Scalars)**: 26 MFCC running statistics (mean & std per coefficient, υπολογισμένα ανέξοδα από το `SegmentScout` flux pass), spectral flux, zero-crossing rate, BPM autocorrelation, 8-band mel energy profile.
* **Feature Ceiling**: 79% ακρίβεια ταξινόμησης σε πολυ-ταξικό setup με τα 57 χειροποίητα scalars.
* **CLAP Teacher Benchmark**: 82.7% συνολική ακρίβεια / 94% σε rap.
* **Distillation Failure (Μετρημένο ×3)**: Η προσπάθεια συμπύκνωσης (distillation) της γνώσης του CLAP teacher στα 57 scalars ΑΠΕΤΥΧΕ.
  * *Διάγνωση*: 57 χρονικά στατικά scalars δεν μπορούν να εκφράσουν τη δυναμική εξέλιξη της φωνής και του χρόνου ($57 \text{ scalars} \neq \text{φωνή/χρόνος}$).
* **Dataset & Bias Note**: $N=830$ tracks ταξινομημένα με CLAP-voting.
  * *Σημείωση Bias*: Το CLAP φέρει δικά του συστηματικά σφάλματα και προκαταλήψεις (domain bias), τα οποία μεταφέρονται αυτούσια στο voting.
* **UMAP Visualization**: Χαρτογράφηση των 57-dimensional feature vectors σε 2D UMAP space επιβεβαίωσε την απουσία καθαρών διαχωριστικών υπερεπιπέδων για ορισμένα genres (π.χ. acoustic vs pop).
* **Αρχιτεκτονικά Συμπεράσματα & Φράγματα**:
  * **VAD Refactor**: ΑΚΥΡΟ.
  * **Runtime Neural Inference / CNN**: ΠΟΤΕ στο binary runtime (δόγμα: μηδενικές εξωτερικές εξαρτήσεις, ML ΜΟΝΟ στο factory/offline).

---

## 2. Οι Τρεις Γενιές Teacher-Student

Η εξέλιξη των μοντέλων μάθησης στο CREATOR OS ακολούθησε τρεις διαδοχικές γενιές teacher-student:
1. **1st Gen (Speech & VAD Gating)**: Silero VAD (Teacher) $\rightarrow$ Rust `microVAD` (Student, DSP heuristics, cepstral flux & energy guards).
2. **2nd Gen (Genre & Timbre Classification)**: CLAP zero-shot (Teacher) $\rightarrow$ 57-scalar feature classifier (Student, distillation attempt — μετρημένο ταβάνι 79%).
3. **3rd Gen (Spectral Priors & NMFD)**: MUSDB18-HQ STEMs (Teacher) $\rightarrow$ Discriminative NMFD template matrices (`w_drums_v1`, `w_music_v1.bin`, Student embedded arrays στο Rust DSP runtime).

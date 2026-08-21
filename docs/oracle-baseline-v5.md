# Oracle Baseline v5 — Ο ΚΑΘΡΕΦΤΗΣ της F-066 Ετυμηγορίας

**Ημερομηνία**: 2026-08-21  
**HEAD Hash**: `e75bc1c77f7b6493d239de4dcfd0d0514d42fb23`  
**Σκοπός**: Ο ΚΑΘΡΕΦΤΗΣ της F-066 ετυμηγορίας — το v6 συγκρίνεται ΜΟΝΟ με αυτό, με ΤΑ ΙΔΙΑ όργανα/συμβάσεις.

---

## 1. Η Αλυσίδα-Σκεπτικό (Rationale Chain)

> **«NMF mask → Ducker → crest → Limiter → TP/LUFS: γι' αυτό το πάνελ είναι ΠΛΗΡΕΣ, όχι μόνο separation»**

Το πάνελ μέτρησης του Oracle Baseline v5 καλύπτει ολόκληρη την αλυσίδα επεξεργασίας του σήματος:
1. **NMF Mask & Separation**: Επαληθεύεται η πιστότητα διαχωρισμού των 4 στελεχών (four_stem_contract) και οι επιδόσεις SDR σε MUSDB18-HQ (oracle_sdr_musdb).
2. **Dynamic Ducking**: Αξιολογείται η αλληλεπίδραση φωνής και μουσικής υπό τους κανόνες VAD/Ducker.
3. **Crest & Spatial Conformity**: Επαληθεύεται η συμφωνία του 6ch spatial folddown με το stereo παραδοτέο (spatial_folddown_agrees_with_stereo).
4. **Limiter & Mastering Conformance**: Μετρώνται τα τελικά επίπεδα Integrated LUFS, True Peak, LRA και RMS.
5. **Determinism Invariants**: Εξασφαλίζεται η απόλυτη bit-exact επαναληψιμότητα σε μεμονωμένα renders (INV-DET-1) και σε batch renders άλμπουμ (INV-DET-2).

---

## 2. Πίνακας Αποτελεσμάτων Πάνελ (Panel Members)

| Μέλος Πάνελ | Μετρημένες Τιμές (RELEASE Baseline) | Log Αρχείο | SHA-256 του Log |
| :--- | :--- | :--- | :--- |
| **1. Four Stem Contract** | 4 passed · Reconstruction SDR = 100.0 dB · Rec Error = 1.79e-7 · Drums@impulse = 0.8788 · Bass@impulse = 0.0429 | `contract.log` | `58a40808727cafbe7ae788977bb91a5c1b667ebd38d09eadef10724abdf02740` |
| **2. Spatial Folddown** | 1 passed · L corr = 0.9994 (lag=0) · R corr = 0.9993 (lag=0) · Frames = 480000 | `folddown.log` | `ea54f535aa15e56aba96fcf26271477c12f778afc57e94046294ba642905be96` |
| **3. INV-DET-1 Render** | 1 passed · SHA A == SHA B (`e682a3dba78cf64797cbc67dcb5ce049d4cb6c5c422e9c999a7341b1ad937b09`) | `invdet1.log` | `7179856ca27172cdeaabf7c96dbd7e205f43cc09f913ac48147f4fa050a5cd0b` |
| **4. INV-DET-2 Batch** | 1 passed · Album SHA A == Album SHA B (batch album render determinism ok) | `invdet2.log` | `256ea17586305ff642d4de30308dbf23b9bd317ab55e0e0980ed6dfefda53895` |
| **5. Oracle Certificate** | 1 passed · LUFS = -14.00 · TP = -2.16 dB · LRA = 4.96 · RMS = -15.21 dB · SHA = `e682a3dba78cf64797cbc67dcb5ce049d4cb6c5c422e9c999a7341b1ad937b09` | `oracle_cert.log` | `a1cdba96e6dfd5b5c0c623119286475735495f9025ccc713cce9a31c347a017a` |
| **6. SDR Track 1** | Tom McKenzie - Directions: Vocals = 0.14 dB · Bass = 0.47 dB · Drums = 3.23 dB · Other = 0.20 dB · Recon = 119.24 dB | `sdr_track1.log` | `cea1e120243a3c0dfd1bac042b52e85f537d5f58c9f6f045bfaf72a334aefd05` |
| **7. SDR Track 2** | Louis Cressy Band - Good Time: Vocals = 0.90 dB · Bass = 1.68 dB · Drums = 1.44 dB · Other = 1.36 dB · Recon = 139.29 dB | `sdr_track2.log` | `829e3653582b2931681791787c3ce1e587f583cd5b046ca61117b821f4122146` |
| **8. SDR Track 3** | Skelpolu - Resurrection: Vocals = -1.17 dB · Bass = 3.82 dB · Drums = 1.10 dB · Other = -0.36 dB · Recon = 104.07 dB | `sdr_track3.log` | `d36794205abaf15dfff6688b1757265e87fbd230445342557b34bd1af22109a3` |

---

## 3. Συμβάσεις SDR Οργάνου (Instrument Conventions)

* **Mono Conversion**: (L+R)/2 σε ΟΛΑ τα αρχεία, ΠΡΙΝ το resample.
* **Resampling**: 44.1→48 kHz με rubato SincFixedIn, ΙΔΙΕΣ παράμετροι σε mixture ΚΑΙ ground-truth stems (ίδια μεταχείριση = δίκαιη σύγκριση).
* **Stem Mapping 5→4 (σύμβαση του ΟΡΓΑΝΟΥ)**: voice→vocals · bass→bass · drums→drums · harmonics+ambience (άθροισμα)→other. ΚΑΜΙΑ χρονική ευθυγράμμιση δεν εκτελείται — ο renderer επιστρέφει ίδιο μήκος με το input.
* **Trim**: fft_size=2048 δείγματα από ΚΑΘΕ άκρο πριν το SDR (STFT edge artifacts) — ΟΧΙ αφαίρεση μηδενικών.
* **Συγκρισιμότητα**: Το όργανο SDR είναι **συγκρίσιμο ΜΟΝΟ με τον εαυτό του** υπό τις ίδιες ακριβώς παραμέτρους και εκτελέσιμα.

---

## 4. Dataset Provenance

**Εκδοχή Dataset**: `MUSDB18-HQ uncompressed WAV 44100Hz — ΟΧΙ το compressed MUSDB18`  
**SHA-256 των 15 WAV αρχείων (5 ανά κομμάτι)** (από `dataset.log`):

```text
b6988c2558a2aca370518d7ca10e6c2b6d57fc7b62abf4f2579665ea0ee17d7c  Tom McKenzie - Directions/bass.wav
e6ad76b331d7f4c8e13c4b4a5896f81e8b3b4f08796a08cfed7f23d1f39a1af1  Tom McKenzie - Directions/drums.wav
77f9e18708a523cf42abc910118fbd7cc46c7cc1f221a1a0344d22a5f2d149ec  Tom McKenzie - Directions/mixture.wav
60e45df51993886a06d70d03f9c847f91a960a4f0cd0ddf2755f31d44be35846  Tom McKenzie - Directions/other.wav
57d09425e7b2468ffe6bb5f4e6724d6aed317052895674e4ebc63e68c5b2eeb2  Tom McKenzie - Directions/vocals.wav

e95acfc41d74cc648c3a16ada47d672fc467b4937e4dcf841ce81636748e2a16  Louis Cressy Band - Good Time/bass.wav
4c1dce3f229f3c3c036f8e64e165d5fe2860a206b7b3e5057d2a30a09289c417  Louis Cressy Band - Good Time/drums.wav
841f0ae493c0e3d9fbba9878dda54ed1901b997294ba3575cd101159a5619916  Louis Cressy Band - Good Time/mixture.wav
6490131a0bde6793fa0210155c23136fbc61255b80d849358af0e20ab5625d81  Louis Cressy Band - Good Time/other.wav
30463004efb12d6d46e9831371b3bf436840c940f69e7c91cea56e4ad9fbcd79  Louis Cressy Band - Good Time/vocals.wav

2aa578c4b1333d0da6de5d0c4ab4983fef0a441d0e4f4c1abbcb5d7ecfe6eea8  Skelpolu - Resurrection/bass.wav
730e08ca4f9c10623bb472a45cbb36f016b9d219b1dc25b0191be2f7e128a0b8  Skelpolu - Resurrection/drums.wav
cc1e097be9d78aaf630f0663020fc48287fadb1e696975d77295b8eb14166ff9  Skelpolu - Resurrection/mixture.wav
582564d31a939ea5948f27075a0f9b98edb6dd1f62898e3f0cb6edf330919e30  Skelpolu - Resurrection/other.wav
2d041d6f662e49ac1b57a64b236b8543db082c2f173fbc1ba8ffb489dc866d93  Skelpolu - Resurrection/vocals.wav
```

---

## 5. Binary Provenance

* **Git HEAD**: `e75bc1c77f7b6493d239de4dcfd0d0514d42fb23`
* **Rustc Version**: `rustc 1.94.0 (4a4ef493e 2026-03-02)`
* **Cargo Profile Opt-Level**: `opt-level = 3` (Release Profile)
* **Target CPU Architecture**: `target-cpu = x86-64` (`.cargo/config.toml`)

---

## 6. Αποκλίσεις Release vs. Dev (Release-vs-Dev Discrepancies)

* **Ετυμηγορία**: Τα αποτελέσματα του **Release profile (`--release`)** ταυτίζονται πλήρως με τις μετρήσεις Dev mode στις βασικές παραμέτρους (LUFS = -14.00, True Peak = -2.16 dB, LRA = 4.96, RMS = -15.21 dB, `master_sha256` = `e682a3dba78cf64797cbc67dcb5ce049d4cb6c5c422e9c999a7341b1ad937b09`).
* **Κανόνας**: Η επίσημη baseline μέτρηση καθορίζεται αυστηρά από το **RELEASE build profile**, το οποίο αποτελεί το μοναδικό σημείο αναφοράς για τις μελλοντικές συγκρίσεις v6.

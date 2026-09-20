//! Genuine local helpers `execute_streaming_plan`/`run_deliver_core`
//! need from m0-daemon's blob_store.rs — moved here 21/09, verbatim.
//! blob_store.rs itself stays (F-129: paths, hashing, signing) — its
//! other path functions (raw_dump_spatial_path, mastering_path,
//! scratch_l_path/scratch_r_path, premaster_l_path/premaster_r_path,
//! vad_trace_path) keep their own copy of `sanitize_path_component`
//! there; this is the one `mastered_path`/`raw_dump_path` need.

pub fn sanitize_path_component(s: &str) -> String {
    s.replace('/', "").replace('\\', "")
}

/// BUG-DECODE-1: ο πρώτος writer (decode).
pub fn raw_dump_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!("m0d-raw-{}.pcm", sanitize_path_component(blob_id)))
}

/// Post-render raw PCM tap — streaming executor path.
pub fn mastered_path(blob_id: &str) -> std::path::PathBuf {
    crate::spool::spool_dir().join(format!(
        "m0d-mastered-{}.pcm",
        sanitize_path_component(blob_id)
    ))
}

/// Η ΣΥΝΟΛΙΚΗ ΕΤΥΜΗΓΟΡΙΑ — ΣΥΝΑΡΤΗΣΗ ΠΑΝΩ ΣΤΙΣ ΓΡΑΜΜΕΣ, ΟΧΙ ΓΝΩΣΗ.
///
/// ΓΙΑΤΙ ΥΠΑΡΧΕΙ: μέχρι τις 25/08 ένα αρχείο με `tail 6 s` έγραφε
/// `"fail"` στα `delivery_checks` ΚΑΙ `passes_acx: true` στο manifest —
/// **το ίδιο έγγραφο έλεγε «περνάει» και «παραβίαση»**. Η αιτία δεν ήταν
/// σφάλμα υπολογισμού: δεν υπήρχε πουθενά σημείο που να απαντά «αυτό το
/// αρχείο συμμορφώνεται;». Υπήρχαν δύο μισές απαντήσεις με ονόματα που
/// διεκδικούσαν και οι δύο ολόκληρο το ερώτημα.
///
/// Ο συνθέτης **δεν ξέρει κανένα κριτήριο**. Ξέρει μόνο πώς διαβάζονται
/// οι ετυμηγορίες. Ένα νέο κριτήριο σε οποιοδήποτε crate επιστρέφει
/// γραμμές και μπαίνει μόνο του — τίποτα δεν μετακομίζει εδώ.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DeliveryVerdict {
    /// Συμμορφώνεται το αρχείο; **Καμία `fail`, κανένα κριτήριο να
    /// λείπει.** Το `advisory` ΔΕΝ μετράει.
    pub complies: bool,
    /// Ονόματα μετρικών με `"fail"`.
    pub failed: Vec<String>,
    /// Ονόματα μετρικών με `"advisory"`.
    ///
    /// ⚠ ΓΙΑΤΙ ΔΕΝ ΕΠΙΣΤΡΕΦΕΤΑΙ ΣΚΕΤΟ `bool`: ένα αρχείο με έξι
    /// ανεκπλήρωτες συστάσεις **περνάει** — σωστά — αλλά ο αφηγητής
    /// θέλει να το ξέρει. Με σκέτο bool η τρίτη κατάσταση θα
    /// υπολογιζόταν και θα πεταγόταν στη σύνοψη, το μοτίβο που αυτό το
    /// repo έχει βρει δεκατρείς φορές.
    pub advisory: Vec<String>,
    /// Κριτήρια που **το συμβόλαιο δήλωνε** και **καμία γραμμή δεν
    /// κάλυψε**. Απουσία αναμενόμενης γραμμής **ΔΕΝ είναι pass**
    /// (κανόνας §5.2 στο επίπεδο του συνόλου): ένας παραγωγός που
    /// σιωπά θα διαβαζόταν αλλιώς ως «όλα καλά».
    pub missing: Vec<String>,
}

impl DeliveryVerdict {
    /// ΤΟ FOLD. **ΕΙΝΑΙ ΠΟΛΙΤΙΚΗ, ΟΧΙ ΜΗΧΑΝΙΚΗ** — το «advisory δεν
    /// μετράει στη συμμόρφωση» είναι ΑΠΟΦΑΣΗ, όχι ταυτότητα: λέει ότι
    /// οι συστάσεις του οίκου δεν επηρεάζουν την αποδοχή. Γι' αυτό έχει
    /// δικό του oracle, όπως κάθε παραγωγός.
    ///
    /// Δεν είναι «μηδέν γνώση» — είναι **μία γραμμή γνώσης αντί για
    /// οκτώ**. Αυτή είναι και όλη η διαφορά.
    pub fn compose(
        spec: &lineos_types::presets::DeliverySpec,
        checks: &[lineos_types::certificate::DeliveryCheck],
    ) -> Self {
        let failed: Vec<String> = checks
            .iter()
            .filter(|c| c.verdict == "fail")
            .map(|c| format!("{}/{}", c.metric, c.bound))
            .collect();
        let advisory: Vec<String> = checks
            .iter()
            .filter(|c| c.verdict == "advisory")
            .map(|c| format!("{}/{}", c.metric, c.bound))
            .collect();

        // ΤΙ ΠΕΡΙΜΕΝΩ — ΑΠΟ ΤΟ ΣΥΜΒΟΛΑΙΟ, όχι από λίστα εδώ. Ό,τι
        // δηλώνει ο προορισμός πρέπει να έχει καλυφθεί από γραμμή.
        let mut expected: Vec<(&str, &str)> = vec![("peak", "max")];
        if spec.rms_window_db.is_some() {
            expected.push(("rms", "min"));
            expected.push(("rms", "max"));
        }
        if spec.max_noise_floor_db.is_some() {
            expected.push(("noise_floor", "max"));
        }
        if spec.room_tone_max_s.is_some() {
            expected.push(("head_spacing", "max"));
            expected.push(("tail_spacing", "max"));
        }
        if spec.room_tone_recommend_min_s.is_some() {
            expected.push(("head_spacing", "min"));
            expected.push(("tail_spacing", "min"));
        }
        if spec.required_sample_rate_hz.is_some() {
            expected.push(("sample_rate", "max"));
        }
        if spec.emitted_channels.is_some() {
            expected.push(("channels", "max"));
        }
        if spec.min_bitrate_kbps.is_some() {
            expected.push(("bitrate", "min"));
        }

        let missing: Vec<String> = expected
            .into_iter()
            .filter(|(m, b)| !checks.iter().any(|c| c.metric == *m && c.bound == *b))
            .map(|(m, b)| format!("{m}/{b}"))
            .collect();

        Self {
            complies: failed.is_empty() && missing.is_empty(),
            failed,
            advisory,
            missing,
        }
    }
}

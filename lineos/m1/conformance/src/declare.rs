//! declare — η δήλωση συμμόρφωσης: report του πυρήνα → εγγραφές §5.3.

use lineos_types::certificate::DeliveryCheck;

/// Η ΜΙΑ μετάφραση `AcxMarginCheck` → §5.3 εγγραφή.
///
/// Ο ΚΑΝΟΝΑΣ ζει ήδη σε ένα σημείο — `AcxCheckReport::margin_checks()`
/// (acx_check.rs), όπως δηλώνει το doc του `AcxMarginCheck`. Αυτό εδώ
/// είναι η ΜΕΤΑΦΡΑΣΗ του σε εγγραφή sidecar, και ήταν γραμμένη inline
/// στο `deliver.rs`. Από τη στιγμή που δεύτερος καλών τη χρειάζεται
/// (`/export`), inline σημαίνει δύο αντίγραφα που μπορούν να
/// αποκλίνουν — άρα βγαίνει εδώ, δίπλα στον τύπο που παράγει.
/// ΜΙΑ ΥΛΟΠΟΙΗΣΗ, ΔΥΟ ΚΑΛΟΥΝΤΕΣ.
///
/// Απουσία μετρικής (π.χ. noise_floor σε αρχείο < 1 s) = **καμία
/// εγγραφή**, όχι κατασκευασμένη — ο κανόνας απουσίας (§5.2) εφαρμόζεται
/// ήδη μέσα στο `margin_checks()`.
pub fn delivery_checks_from_margin_checks(
    report: &sp314_dsp::analysis::acx_check::AcxCheckReport,
) -> Vec<DeliveryCheck> {
    report
        .margin_checks()
        .into_iter()
        .map(|c| DeliveryCheck {
            metric: c.metric.to_string(),
            measured: c.measured_db,
            required: c.required_db,
            bound: c.bound.to_string(),
            margin_applied: c.margin_applied_db,
            verdict: if c.verdict { "pass" } else { "fail" }.to_string(),
            unit: "db".to_string(),
        })
        .collect()
}

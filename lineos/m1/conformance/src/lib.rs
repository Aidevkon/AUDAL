//! conformance — η διαδρομή της παράδοσης: αποκωδικοποίηση,
//! μέτρηση, διόρθωση, κωδικοποίηση, δήλωση. Καλεί τον
//! ντετερμινιστικό πυρήνα, δεν τον αντικαθιστά. Μηδέν HTTP,
//! μηδέν βάση, μηδέν διακομιστής — κατάσταση έρχεται ως όρισμα.
//!
//! symphonia lives here, and sp314-dsp still never imports it.

pub mod decode;
pub mod declare;
pub mod export;
pub mod io_flac;

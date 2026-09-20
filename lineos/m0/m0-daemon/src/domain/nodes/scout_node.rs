//! Μετακόμισε στο conformance/src/scout_node.rs στις 21/09 (F-137).
//! Γέφυρα· το run_dsp_internal καλεί με πλήρη διαδρομή όπως
//! ζητήθηκε, η γέφυρα μένει ώστε το `pub mod scout_node;` να μη
//! σπάσει. Ίδιο σχήμα με handlers/decode.rs.
pub use conformance::scout_node::*;

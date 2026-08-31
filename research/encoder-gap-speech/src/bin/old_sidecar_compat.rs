//! ΕΠΑΛΗΘΕΥΣΗ (F-085, 2026-08-25): διαβάζονται ΑΚΟΜΑ τα παλιά sidecars
//! που περιέχουν το αφαιρεμένο `phase_coherence`;
//! Το serde ΑΓΝΟΕΙ άγνωστα πεδία by default — αλλά αυτό είναι υπόθεση
//! μέχρι να τρέξει. Εδώ τρέχει, πάνω στον ΠΡΑΓΜΑΤΙΚΟ τύπο.

fn main() {
    // Ακριβώς ό,τι έγραφε ένα sidecar πριν την αφαίρεση.
    let old = r#"{
        "stereo_correlation": 0.87,
        "phase_coherence": 0.97,
        "stereo_width": 0.13,
        "dynamic_range_db": 11.5,
        "rms_db": -20.1,
        "spectral_centroid": 3200.0,
        "spectral_flatness": 0.12,
        "clips_detected": 0,
        "clip_free": true
    }"#;

    match serde_json::from_str::<m0d::blob_store::StoredQuality>(old) {
        Ok(q) => {
            println!("MEASURED παλιό sidecar ΔΙΑΒΑΣΤΗΚΕ ✅");
            println!("  stereo_correlation = {}", q.stereo_correlation);
            println!("  stereo_width       = {}", q.stereo_width);
            println!("  spectral_centroid  = {}", q.spectral_centroid);
            println!("  clips_detected     = {}", q.clips_detected);
            println!("  ⇒ το άγνωστο phase_coherence αγνοήθηκε σιωπηλά");
        }
        Err(e) => println!("FAILED παλιό sidecar ΔΕΝ διαβάζεται: {e}"),
    }

    // Και το αντίστροφο: το νέο JSON δεν γράφει πια το πεδίο.
    let q = m0d::blob_store::StoredQuality::default();
    let out = serde_json::to_string(&q).unwrap();
    println!("MEASURED νέο serialize περιέχει phase_coherence; {}",
        out.contains("phase_coherence"));
}

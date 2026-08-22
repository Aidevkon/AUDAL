#!/usr/bin/env bash
# audio_wire.sh — ο φρουρός των #[ignore] tests κάτω από
# lineos/m0/m0-daemon/tests/. ΝΟΜΟΣ 2 (.claude/CLAUDE.md): "Wire που
# αγγίζει ήχο ⇒ ρητό τρέξιμο των #[ignore] gates (--ignored). Gate που
# δεν τρέχει δεν φρουρεί." Αυτό το script είναι εκείνο το ρητό τρέξιμο.
#
# Απογραφή: grep -rln "#\[ignore" lineos/m0/m0-daemon/tests/ — 30
# αρχεία, 39 ignored test functions, 2026-08-23. Πλήρης λίστα αρχείων
# αυτούσια στην αναφορά (.reports/2026-08-23-audio-wire.md), ΟΧΙ εδώ —
# εδώ μόνο το ΜΗΤΡΩΟ (τι φυλάει το καθένα, ποια src paths το ξυπνούν).
#
# ═══════════════════════════════════════════════════════════════════
# ΜΗΤΡΩΟ — ένα test ανά γραμμή: όνομα · τι φυλάει · ποια src paths
# το ξυπνούν (αν κάποιος αγγίξει αυτά τα paths, ΑΥΤΟ το gate πρέπει
# να τρέξει ρητά πριν merge).
# ═══════════════════════════════════════════════════════════════════
#
# --- output_acx_delivered_certificate.rs ---
# output_acx_writes_and_signs_the_delivered_certificate
#     Το output_acx_* (§5.6 Δ2) γράφεται σωστά στο ξαναγραμμένο
#     sidecar μετά το deliver, ίδιο με το manifest.json, signature
#     επαληθεύει.
#     src: handlers/deliver.rs (run_deliver_core) · blob_store.rs
#     (write_sidecar/find_sidecar/StoredLoudness) · handlers/export.rs
#     (export_mp3_acx, ΔΕΝ αγγίχτηκε αλλά διαβάζεται)
#
# --- external_acx_ffmpeg_agreement.rs ---
# external_ffmpeg_agrees_with_output_acx_certificate
#     Εξωτερικός ένορκος (ffmpeg astats) συμφωνεί με το output_acx_*
#     του certificate εντός δηλωμένων ορίων (§5.6 Δ2 follow-up,
#     relocked 2026-08-23).
#     src: handlers/deliver.rs · handlers/export.rs (export_mp3_acx) ·
#     εξωτερικό binary: ffmpeg (ΟΧΙ crate)
#
# --- acx_export_rms_window.rs ---
# acx_export_lifts_quiet_signal_into_rms_window
# acx_export_leaves_compliant_signal_alone
# acx_export_lowers_hot_signal_into_rms_window
#     Η ACX RMS-window correction (target [-23,-18] dBFS) σηκώνει
#     ήσυχο υλικό, αφήνει ήδη-συμμορφούμενο ανέγγιχτο, κατεβάζει
#     δυνατό — μετρημένο με τον ΙΔΙΟ AcxCheckAnalyzer πριν/μετά.
#     src: handlers/export.rs (export_mp3_acx, RMS correction + true
#     peak trim) · sp314-dsp/analysis/acx_check.rs (AcxCheckAnalyzer)
#
# --- export_mp3_acx.rs ---
# test_export_mp3_acx_ffprobe
#     Το delivered mp3 έχει το σωστό sample_rate/bitrate (44100/
#     192000) όπως το βλέπει ffprobe — ίδιο πνεύμα με τον εξωτερικό
#     ένορκο, στενότερο σκοπεύον (container metadata, όχι samples).
#     src: handlers/export.rs (export_mp3_acx, resample 48k→44.1k,
#     LAME encode) · εξωτερικό binary: ffprobe
#
# --- deliver_acx.rs ---
# test_deliver_acx_ffprobe
#     Το ίδιο ffprobe-check πάνω από ΟΛΟΚΛΗΡΟ το deliver flow (πολλά
#     tracks, manifest), όχι μόνο τον export path μόνο του.
#     src: handlers/deliver.rs (run_deliver_core, validate_and_plan) ·
#     handlers/export.rs · εξωτερικό binary: ffprobe
#
# --- export_flac_real.rs ---
# test_export_flac_ffprobe
#     Το FLAC deliverable (album/master path) έχει τα σωστά technical
#     metadata κατά ffprobe.
#     src: handlers/export.rs (export_flac) · sp314-dsp/io/
#     flac_encode.rs
#
# --- inv_persist_1_certificate_survives_restart.rs ---
# inv_persist_1_certificate_survives_restart
#     Το certificate σε δίσκο επιζεί ΕΠΑΝΕΚΚΙΝΗΣΗ daemon (RAM cache
#     άδειο) — read_sidecar/find_sidecar ξαναφτιάχνουν το ίδιο blob.
#     ~15s (release, δηλωμένο στο ignore reason).
#     src: blob_store.rs (write_sidecar/read_sidecar/find_sidecar/
#     blob_storage_path) · domain/dsp_pipeline.rs (run_dsp, πλήρες
#     render)
#
# --- inv_det_1_render_determinism.rs ---
# inv_det_1_render_determinism
#     Ίδιο input + seed ⇒ bit-identical PCM σε δύο ανεξάρτητα renders.
#     ~15s (release, δηλωμένο).
#     src: domain/dsp_pipeline.rs (run_dsp) — ΟΛΟΚΛΗΡΟ το DSP pipeline
#
# --- inv_det_2_batch_determinism.rs ---
# inv_det_2_batch_determinism
#     Ίδιο για batch rendering (run_batch, 2 tracks) — καμία
#     διασταύρωση state μεταξύ tracks στο ίδιο batch run.
#     ~12s (release, μετρημένο 2026-08-21, δηλωμένο).
#     src: agents/batch.rs (run_batch) · agents/operator.rs
#     (MasteringParams) · domain/nodes/album_certificate_node.rs
#
# --- oracle_certificate.rs ---
# oracle_certificate
#     Baseline oracle instrument — πλήρες render, snapshot σύγκριση σε
#     freeze/compare points (όχι κανονικό regression gate).
#     src: domain/dsp_pipeline.rs (run_dsp) · handlers/master.rs
#     (MasterRequest)
#
# --- w1_vad_trace.rs ---
# render_vad_trace_vehicle
#     Παράγει VAD trace CSV πάνω από πλήρες render — διαγνωστικό, όχι
#     pass/fail assertion βαρύ.
#     src: domain/dsp_pipeline.rs (run_dsp) · domain/nodes/
#     render_node.rs (VAD trace writer, blob_store::vad_trace_path)
#
# --- w2_duck_gate.rs ---
# w2_duck_gate_synth
# w2_duck_gate_real
# w2_duck_gate_synth_variance
# w3b_mix_levels_gate
# w4_ceiling_gate
# w4_ceiling_sweep
#     Ducking/mix-level/ceiling gates πάνω σε synth+real fixtures —
#     πλήρες render ανά test. ~2min έκαστο (δηλωμένο στο ignore
#     reason, "full render + /tmp fixtures").
#     src: domain/dsp_pipeline.rs (run_dsp) · control_bus.rs (Ducker) ·
#     sp314-dsp limiter/ceiling paths
#
# --- w6c_nmfd_cost.rs ---
# w6c_nmfd_cost
#     Κόστος/επίδραση του NMFD separation pass στο πλήρες render.
#     src: domain/dsp_pipeline.rs (run_dsp) · sp314-dsp/stft/nmfd.rs
#
# --- w10_duck_dynamics.rs ---
# w10_duck_dynamics
#     Δυναμική του ducking (RMS L/R πριν/μετά) πάνω σε πλήρες render.
#     src: domain/dsp_pipeline.rs (run_dsp) · control_bus.rs (Ducker)
#
# --- w17_bed_sweep.rs ---
# w17_bed_sweep
#     Sweep πάνω από bed boost levels (MixLevels) — full render ανά
#     τιμή.
#     src: domain/dsp_pipeline.rs (run_dsp) · handlers/master.rs
#     (MixLevels)
#
# --- w17_mix_balance.rs ---
# w17_mix_balance
#     Ισορροπία μίξης stems (MixLevels) πάνω σε πλήρες render.
#     src: domain/dsp_pipeline.rs (run_dsp) · handlers/master.rs
#     (MixLevels)
#
# --- w17_flacenc_bloat.rs ---
# w17_flacenc_bloat_repro
#     Αναπαράγει flacenc 0.3.1 bloat bug (37× το raw PCM μέγεθος σε
#     συγκεκριμένο σήμα) — diagnostic repro, όχι pass/fail assertion.
#     src: sp314-dsp/io/flac_encode.rs (flac_encode)
#
# --- w17_flacenc_narrow.rs ---
# w17_flacenc_narrow
#     Εντοπίζει το ΑΚΡΙΒΕΣ δευτερόλεπτο όπου ξεκινά το flacenc bloat —
#     bisection πάνω στο ίδιο σήμα.
#     src: sp314-dsp/io/flac_encode.rs (flac_encode)
#
# --- glue_full_render.rs ---
# glue_full_render
#     Πλήρες render με το glue-send chain ενεργό (GLUE_SEND_AMOUNT).
#     src: domain/dsp_pipeline.rs (run_dsp) — glue send path
#
# --- ab_render_full.rs ---
# render_ab_full_pipeline
#     A/B render fixture (bodleasons_mid.wav) μέσα από ΟΛΟΚΛΗΡΟ το DSP
#     pipeline — invoked επίσης από render_variants.sh, όχι μόνο εδώ.
#     src: domain/dsp_pipeline.rs (run_dsp) · domain/nodes/
#     render_node.rs · sp314-dsp/spatial/{five_dot_one,renderer}.rs
#
# --- stem_independence.rs ---
# the_five_stems_are_distinct_signals
#     Τα 5 NMFD stems (voice/drums/bass/harmonics/ambience) είναι
#     πράγματι ΔΙΑΚΡΙΤΑ σήματα (χαμηλή cross-correlation), όχι το ίδιο
#     σήμα πολλαπλασιασμένο.
#     src: sp314-dsp/stft/stem_renderer.rs (FiveStemRenderer)
#
# --- spatial_folddown_agrees_with_stereo.rs ---
# spatial_folddown_agrees_with_stereo
#     Το 5.1→stereo folddown συμφωνεί με το native stereo render (ίδια
#     ουσιαστικά ενέργεια/συσχέτιση).
#     src: domain/dsp_pipeline.rs (run_dsp) · sp314-dsp/spatial/
#     {five_dot_one,renderer}.rs
#
# --- e2e_mastering_quality.rs ---
# inv_qa_3_spectral_balance_full
#     Πλήρους-μήκους spectral baseline — πολύ αργό για routine CI,
#     ελέγχεται χειροκίνητα σε alignment verification.
#     src: domain/dsp_pipeline.rs (run_dsp) · sp314-dsp/analysis/
#     spectral.rs
#
# --- e2e_episode_streaming.rs ---
# music_pipeline_heap_is_scale_invariant
#     Το heap usage του Music pipeline είναι scale-invariant (δεν
#     μεγαλώνει γραμμικά με τη διάρκεια) — PASSING μετά το A3 Steps
#     3a/3b (scratch mmaps + F-043 borrow fix), κρατιέται ως gate.
#     src: domain/dsp_pipeline.rs (run_dsp, Music path)
#
# --- streaming_integration.rs ---
# streaming_pipeline_ducking_e2e
#     Ducking e2e πάνω στο streaming (episode) pipeline, όχι το batch
#     Music path.
#     src: sp314-orchestrator/streaming_pipeline.rs
#     (run_streaming_pipeline_with_timeline) · control_bus.rs (Ducker)
#
# --- pass1_integration.rs ---
# test_build_timeline_map
#     Το πρώτο πέρασμα (pass1) χτίζει σωστό timeline map πριν το
#     streaming render.
#     src: sp314-orchestrator/pass1_pipeline.rs (build_timeline_map) ·
#     dsp/file_decoder.rs
#
# --- audition.rs ---
# audition
#     Γράφει αρχεία στον δίσκο για ΑΝΘΡΩΠΙΝΗ ακρόαση — δεν είναι
#     αυτοματοποιημένο assertion gate, είναι εργαλείο ελέγχου με αυτί.
#     src: domain/dsp_pipeline.rs (run_dsp)
#
# --- vad_validation_real.rs ---
# vad_narration_dream
# vad_narration_crossing
# vad_transient_probe
#     VAD classifier πάνω σε ΠΡΑΓΜΑΤΙΚΑ ηχητικά δείγματα κατεβασμένα
#     από GitHub (ensure_downloaded) — ΧΡΕΙΑΖΕΤΑΙ δίκτυο· χωρίς αυτό
#     αυτά τα 3 αναμένεται να αποτύχουν ή να παραλείψουν, όχι bug του
#     script.
#     src: handlers/decode.rs (decode_audio) · sp314-dsp/analysis/
#     {vad_features,vad_model}.rs
#
# --- e2e_streaming_http_hybrid_smoke.rs ---
# test_streaming_http_hybrid_smoke
#     HTTP-level smoke test πάνω στο streaming path — ΧΡΕΙΑΖΕΤΑΙ
#     τοπικό, untracked flight_clips_stereo dataset (βρέθηκε στο
#     repo root στις 2026-08-23, ../../../flight_clips_stereo).
#     src: handlers/* (HTTP router) · streaming_pipeline.rs
#
# --- integration_router_concurrency.rs ---
# test_router_concurrency_limit_applies_http_backpressure
#     ⚠ ΓΝΩΣΤΟ ΣΠΑΣΜΕΝΟ (X0, δηλωμένο στο ίδιο το ignore reason): το
#     /dev/wait endpoint αφαιρέθηκε, το test επιστρέφει 404 αντί 200.
#     ΟΧΙ audio, ΟΧΙ αναμενόμενο PASS — ΑΝΑΜΕΝΟΜΕΝΟ FAIL μέχρι να
#     ξαναγραφτεί ή να αφαιρεθεί. Παραμένει στο μητρώο για πληρότητα
#     (η απογραφή δεν κρύβει σπασμένα gates).
#     src: (κανένα ενεργό — stale test, όχι audio wire)
#
# ═══════════════════════════════════════════════════════════════════
# ΔΗΛΩΣΗ ΔΙΑΡΚΕΙΑΣ
# ═══════════════════════════════════════════════════════════════════
# Γνωστά (δηλωμένα ρητά στα ignore reasons, RELEASE profile):
#   w2_duck_gate.rs: 6 × ~2min  = ~12min
#   inv_det_1:            ~15s
#   inv_det_2:             ~12s
#   inv_persist_1:         ~15s
#   ─────────────────────────────
#   Άθροισμα γνωστών:     ~12min42s (release)
#
# Τα υπόλοιπα 39-9=30 tests: ΑΓΝΩΣΤΗ διάρκεια πριν το πρώτο πλήρες
# πέρασμα — δεν είχαν ποτέ μετρηθεί ρητά. ΜΗΝ υποθέσεις νούμερο που
# δεν μετρήθηκε (Νόμος 4).
#
# ΠΡΟΣΟΧΗ: αυτό το script τρέχει `cargo test` σε DEBUG profile (όπως
# ζητήθηκε ρητά στο task — καμία --release flag). Οι παραπάνω "γνωστές"
# διάρκειες είναι RELEASE-measured· DEBUG audio DSP είναι τυπικά 5-20×
# πιο αργό σε βαρύ αριθμητικό κώδικα. Το ΠΡΑΓΜΑΤΙΚΟ μετρημένο σύνολο
# αυτού του πρώτου πλήρους περάσματος (DEBUG, 2026-08-23) μπαίνει ΕΔΩ
# μετά το τρέξιμο — βλ. .reports/2026-08-23-audio-wire.md για το
# αυτούσιο output.
#
# ΠΡΑΓΜΑΤΙΚΟ ΣΥΝΟΛΟ (2026-08-23, DEBUG, αυτό το πρώτο πέρασμα): βλ.
# αναφορά — συμπληρώνεται μετά το τρέξιμο, όχι μαντεψιά εδώ.
#
# Επίσης: τα tests ΜΕΣΑ στο ίδιο αρχείο τρέχουν από το cargo με τον
# δικό του παραλληλισμό (πολλά νήματα) — το "σειριακά" παρακάτω
# αφορά ΜΕΤΑΞΥ αρχείων/binaries, ΟΧΙ μεταξύ των tests μέσα σε ΕΝΑ
# αρχείο με πάνω από ένα ignored test (π.χ. w2_duck_gate.rs, 6 tests).
# ═══════════════════════════════════════════════════════════════════

set -uo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
M0_DIR="$SCRIPT_DIR/../lineos/m0/m0-daemon"

if [ ! -d "$M0_DIR" ]; then
    echo "FATAL: m0-daemon crate not found at $M0_DIR" >&2
    exit 1
fi

# Ένα binary target ανά αρχείο test — cargo `--ignored` μέσα σε κάθε
# binary τρέχει ΜΟΝΟ τα ignored tests εκείνου του αρχείου, αφήνοντας
# τυχόν #[test] siblings του ίδιου αρχείου εκτός (ήδη καλυμμένα από
# κανονικό `cargo test`).
TESTS=(
    output_acx_delivered_certificate
    external_acx_ffmpeg_agreement
    acx_export_rms_window
    export_mp3_acx
    deliver_acx
    export_flac_real
    inv_persist_1_certificate_survives_restart
    inv_det_1_render_determinism
    inv_det_2_batch_determinism
    oracle_certificate
    w1_vad_trace
    w2_duck_gate
    w6c_nmfd_cost
    w10_duck_dynamics
    w17_bed_sweep
    w17_mix_balance
    w17_flacenc_bloat
    w17_flacenc_narrow
    glue_full_render
    ab_render_full
    stem_independence
    spatial_folddown_agrees_with_stereo
    e2e_mastering_quality
    e2e_episode_streaming
    streaming_integration
    pass1_integration
    audition
    vad_validation_real
    e2e_streaming_http_hybrid_smoke
    integration_router_concurrency
)

RESULTS=()
FAIL_COUNT=0
START_TS=$(date +%s)

cd "$M0_DIR" || exit 1

for t in "${TESTS[@]}"; do
    echo "───────────────────────────────────────────────────────────"
    echo "▶ $t"
    echo "───────────────────────────────────────────────────────────"
    t_start=$(date +%s)
    if cargo test --test "$t" -- --ignored --nocapture; then
        status="PASS"
    else
        status="FAIL"
        FAIL_COUNT=$((FAIL_COUNT + 1))
    fi
    t_end=$(date +%s)
    elapsed=$((t_end - t_start))
    RESULTS+=("$t|$status|${elapsed}s")
done

END_TS=$(date +%s)
TOTAL_ELAPSED=$((END_TS - START_TS))

echo
echo "═══════════════════════════════════════════════════════════"
echo "AUDIO WIRE — PASS/FAIL TABLE"
echo "═══════════════════════════════════════════════════════════"
printf "%-55s %-6s %s\n" "TEST BINARY" "RESULT" "TIME"
printf "%-55s %-6s %s\n" "-----------" "------" "----"
for r in "${RESULTS[@]}"; do
    IFS='|' read -r name status elapsed <<< "$r"
    printf "%-55s %-6s %s\n" "$name" "$status" "$elapsed"
done
echo "───────────────────────────────────────────────────────────"
echo "Total wall-clock: ${TOTAL_ELAPSED}s"
echo "Failures: $FAIL_COUNT / ${#TESTS[@]}"
echo "═══════════════════════════════════════════════════════════"

if [ "$FAIL_COUNT" -gt 0 ]; then
    exit 1
fi
exit 0

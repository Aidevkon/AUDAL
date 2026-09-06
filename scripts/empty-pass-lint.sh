#!/usr/bin/env bash
set -uo pipefail

# empty-pass-lint.sh — ΕΝΑ TEST ΠΟΥ ΔΕΝ ΜΕΤΡΗΣΕ ΤΙΠΟΤΑ ΔΕΝ ΠΕΡΑΣΕ.
#
# ΓΙΑΤΙ ΥΠΑΡΧΕΙ (ΜΕΤΡΗΜΕΝΟ 2026-08-25, sweep του F-071): δεκατρία μπλοκ
# σε tests/ κάνουν
#     println!("SKIPPED: … missing");
#     return;
# ⇒ ο runner διαβάζει `test result: ok` σε 0.00s. Το test **δηλώνει
# επιτυχία χωρίς να εκτελέσει τίποτα**.
#
# ΤΑ ΧΩΡΙΣ #[ignore] ΕΙΝΑΙ ΤΑ ΧΕΙΡΟΤΕΡΑ: τρέχουν σε ΚΑΘΕ
# `cargo test --workspace` — ci.yml:59 · constitutional-gates.yml:61 ·
# red-freeze.yml:37 — και πιστοποιούνται ως επιτυχίες. Το F-071
# (FINDINGS.md:1371) ζήτησε ρητά «sweep ανά ΜΠΛΟΚ συμπεριφοράς, όχι ανά
# αρχείο»· αυτό εδώ είναι ο sweep, μονιμοποιημένος.
#
# ΤΙ ΔΕΝ ΚΑΝΕΙ: δεν διορθώνει και δεν κρύβει. Τέσσερα μπλοκ ΕΠΙΤΗΔΕΣ
# μένουν ως έχουν (λείπουν fixtures που ΔΕΝ αναπαράγονται· ένα μόνιμα
# κόκκινο CI είναι ο ίδιος μηχανισμός με ένα μόνιμα πράσινο ψεύτικο —
# απόφαση Anestis 25/08). Ο φρουρός τα κάνει ΟΡΑΤΑ και ΜΕΤΡΗΣΙΜΑ, και
# εμποδίζει το ΕΠΟΜΕΝΟ.
#
# ΠΩΣ ΤΡΕΧΕΙ: ΠΡΟΕΙΔΟΠΟΙΕΙ, ΔΕΝ ΜΠΛΟΚΑΡΕΙ — εκτός αν το πλήθος ΑΥΞΗΘΕΙ
# πάνω από το παγωμένο baseline. Ίδιο σχήμα με threshold-lint /
# gate-coverage / run-ignored.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

BASELINE_FILE="scripts/.empty-pass-baseline"

TEST_DIRS=(
    "lineos/m0/m0-daemon/tests"
    "lineos/m1/sp314-dsp/tests"
)

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

# ⚠ ΑΥΤΟΕΞΑΙΡΕΣΗ — ΤΟ ΜΑΘΗΜΑ ΤΟΥ gate-coverage.
# Ο gate-coverage.sh μετρούσε ΤΟΝ ΕΑΥΤΟ ΤΟΥ ως καλούντα, επειδή περιείχε
# τη λέξη που έψαχνε· έδωσε ψευδώς πράσινο ΑΚΡΙΒΩΣ για το αρχείο που τον
# γέννησε, και το ίδιο λάθος επανήλθε με το run-ignored.sh. Αυτό εδώ το
# script περιέχει τη λέξη SKIPPED σε κάθε σχόλιο και σε κάθε grep του.
# ΣΑΡΩΝΕΙ ΜΟΝΟ τα TEST_DIRS — ποτέ scripts/. Η γραμμή από κάτω είναι ο
# λόγος που ΔΕΝ υπάρχει «scripts» στα TEST_DIRS, και δεν πρέπει να μπει.

blocks=0
BLOCK_LIST=""
loop_blocks=0
LOOP_LIST=""

for d in "${TEST_DIRS[@]}"; do
    [ -d "$d" ] || continue
    for f in "$d"/*.rs; do
        [ -e "$f" ] || continue
        # Κάθε γραμμή που ΓΡΑΦΕΙ «SKIPPED» — ο αριθμός γραμμής της.
        #
        # ⚠ ΟΧΙ ΜΟΝΟ println!. Η πρώτη μορφή αυτού του φρουρού έψαχνε
        # `println!("SKIPPED` και ΔΕΝ ΕΒΛΕΠΕ ΚΑΘΟΛΟΥ το phi1_vs_dsp_jury.rs,
        # που γράφει `push(format!("… | SKIPPED …"))` — δηλαδή το ΜΟΝΟ
        # παράδειγμα του τρίτου είδους ήταν αόρατο στον φρουρό που
        # υποτίθεται ότι το ξεχωρίζει. Ο κανόνας είναι «γράφει SKIPPED και
        # γυρίζει», όχι «καλεί println!».
        # Εξαιρούνται σχόλια και reason strings: `// …` και `#[ignore = "…"]`
        # μιλάνε ΓΙΑ το μοτίβο, δεν το εκτελούν.
        while IFS= read -r ln; do
            [ -n "$ln" ] || continue

            # ── ΤΟ ΜΟΤΙΒΟ: SKIPPED και μέσα στις 2 επόμενες γραμμές `return`
            # (ενδιάμεσα μπορεί να υπάρχει κλείσιμο παρένθεσης ή σχόλιο).
            tail_lines=$(sed -n "$((ln + 1)),$((ln + 2))p" "$f")
            printf '%s' "$tail_lines" | grep -q 'return' || continue

            # ── ΤΡΙΤΟ ΕΙΔΟΣ: return ΜΕΣΑ ΣΕ CLOSURE/ΒΡΟΧΟ ──
            # Το phi1_vs_dsp_jury.rs:70 παραλείπει ΑΝΑ TRACK μέσα σε
            # `files.par_iter().for_each(...)`: το `return` βγαίνει από το
            # closure, όχι από το test. Ένα test που παραλείπει 9 στα 10
            # tracks και περνάει είναι ΤΡΙΤΟ είδος ψεύδους και θέλει δική
            # του απόφαση — ΜΕΤΡΙΕΤΑΙ ΞΕΧΩΡΙΣΤΑ, ΔΕΝ μπαίνει στο baseline.
            # Αναγνώριση: υπάρχει for_each/par_iter/map/while/for πριν από
            # τη γραμμή, μέσα στην ίδια συνάρτηση.
            fn_start=$(awk -v s="$ln" '/^(pub )?fn |^    fn |^#\[test\]/ {last=NR} NR>=s {print last; exit}' "$f")
            [ -n "$fn_start" ] || fn_start=1
            if sed -n "${fn_start},${ln}p" "$f" \
                 | grep -qE 'for_each\(|par_iter\(|\.iter\(\)|for .* in |while '; then
                loop_blocks=$((loop_blocks + 1))
                LOOP_LIST+="   ΒΡΟΧΟΣ      $f:$ln  (τρίτο είδος — ξεχωριστή απόφαση)"$'\n'
                continue
            fi

            # ── Έχει το αρχείο #[ignore]; ──
            # Δηλωμένα σιωπηλό ≠ πιστοποιημένο ψευδώς στο CI. Και τα δύο
            # μετράνε, αλλά ο επόμενος πρέπει να ξέρει ποιο είναι ποιο.
            if grep -q '#\[ignore' "$f"; then
                mark="#[ignore]=ΝΑΙ"
            else
                mark="#[ignore]=ΟΧΙ  ← ΤΡΕΧΕΙ ΣΤΟ CI"
            fi

            blocks=$((blocks + 1))
            BLOCK_LIST+="   ΚΕΝΟ ΠΕΡΑΣΜΑ $f:$ln  $mark"$'\n'
        done < <(grep -n 'SKIPPED' "$f" \
                 | grep -vE ':[[:space:]]*(//|#\[ignore)' \
                 | cut -d: -f1)
    done
done

printf '%s' "$BLOCK_LIST"
echo
echo "ΚΕΝΑ ΠΕΡΑΣΜΑΤΑ (SKIPPED + return):  $blocks"
if [ "$loop_blocks" -gt 0 ]; then
    printf '%s' "$LOOP_LIST"
    echo "ΕΚΤΟΣ ΒΑΣΗΣ — return σε βρόχο:      $loop_blocks"
fi

if [ ! -f "$BASELINE_FILE" ]; then
    echo "BASELINE: (δεν υπάρχει — γράψε: echo $blocks > $BASELINE_FILE)"
    exit 0
fi
BASELINE=$(cat "$BASELINE_FILE")
echo "BASELINE (παγωμένο):                $BASELINE"

if [ "$blocks" -gt "$BASELINE" ]; then
    echo
    echo "✗ ΑΥΞΗΣΗ: $BASELINE → $blocks (+$((blocks - BASELINE)))."
    echo "  Νέο test που δηλώνει επιτυχία χωρίς να μετρήσει τίποτα."
    echo "  ΔΙΟΡΘΩΣΕ ΤΟ, ΜΗΝ ΣΗΚΩΣΕΙΣ ΤΟ BASELINE."
    exit 1
fi

if [ "$blocks" -lt "$BASELINE" ]; then
    echo
    echo "✓ ΜΕΙΩΣΗ: $BASELINE → $blocks. Σφίξε το baseline:"
    echo "    echo $blocks > $BASELINE_FILE"
    [ "$STRICT" -eq 1 ] && exit 1
fi

exit 0

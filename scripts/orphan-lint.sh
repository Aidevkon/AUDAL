#!/usr/bin/env bash
set -uo pipefail

# orphan-lint.sh — Ο ΑΡΙΘΜΟΣ ΤΟΥ ΕΠΟΜΕΝΟΥ ΟΡΦΑΝΟΥ ΕΙΝΑΙ ΣΩΣΤΟΣ.
#
# ΓΙΑΤΙ ΥΠΑΡΧΕΙ: ίδιο μοτίβο με το allocator-lint.sh (FINDINGS.md), αλλά
# για το docs/ORPHANS.md. Πριν από αυτό το αρχείο, η αρίθμηση των
# ορφανών ζούσε σε πρόζα μέσα σε ευρήματα F-NNN — «δωδέκατο», «δέκατο
# έκτο ορφανό» — ίδια μέρα, δύο σειρές, ελληνικά τακτικά, ΔΕΝ γρεπίζονταν,
# ΔΕΝ ελέγχονταν, σταμάτησαν. Το docs/ORPHANS.md τους δίνει δικό τους
# μητρώο· αυτό το script του δίνει τον ίδιο φρουρό συνέπειας.
#
# ΤΙ ΜΕΤΡΑΕΙ:
#   (1) ΔΙΠΛΑ — ο ίδιος αριθμός τίτλου εμφανίζεται ≥2 φορές ως τίτλος.
#       Μετρητής. Baseline παγωμένο: 0.
#   (2) ALLOCATOR == ΜΕΓΑΛΥΤΕΡΟΣ ΠΙΑΣΜΕΝΟΣ + 1 — ΝΑΙ/ΟΧΙ, ΟΧΙ μετρητής.
#       Δεν «χαλαρώνει» ποτέ· είτε συμφωνεί είτε όχι.
#
# ΤΙΤΛΟΣ vs ΠΑΡΑΠΟΜΠΗ: η αγκύλη `[O-NNN]` εμφανίζεται ΑΠΟΚΛΕΙΣΤΙΚΑ σε
# τίτλους («### [O-NNN] — ...»). Παραπομπές αλλού γράφουν το γυμνό
# O-NNN, χωρίς αγκύλες — ίδιος διαχωρισμός με το allocator-lint.sh,
# αντιγραμμένος εδώ επίτηδες, όχι επανεφευρημένος.
#
# ΔΕΝ ΕΛΕΓΧΕΤΑΙ: κενά στην αρίθμηση, μέγεθος του μητρώου, ή αν κάποιο
# ορφανό «έπρεπε» να έχει συνδεθεί μέχρι τώρα — άλλος φρουρός, αν
# χρειαστεί ποτέ. Το πλήθος επιτρέπεται να ανεβαίνει· σημαίνει ότι
# βρέθηκε κι άλλο ορφανό, όχι ότι κάτι χάλασε.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

FILE="docs/ORPHANS.md"
BASELINE_FILE="scripts/.orphan-baseline"

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

if [ ! -f "$FILE" ]; then
    echo "Σφάλμα: $FILE δεν βρέθηκε." >&2
    exit 1
fi

# ── ΤΙΤΛΟΙ, ΑΠΟΚΛΕΙΣΤΙΚΑ Η ΑΓΚΥΛΩΤΗ ΜΟΡΦΗ ──────────────────────────
mapfile -t NUMS < <(grep -o '\[O-[0-9]\+\]' "$FILE" | grep -o '[0-9]\+')

dupes=0
max=0
declare -A seen
for n in "${NUMS[@]}"; do
    n10=$((10#$n))
    if [ "${seen[$n10]:-0}" -gt 0 ]; then
        dupes=$((dupes + 1))
        echo "   ΔΙΠΛΟ  O-$(printf '%03d' "$n10")  (τίτλος εμφανίζεται ξανά)"
    fi
    seen[$n10]=$(( ${seen[$n10]:-0} + 1 ))
    [ "$n10" -gt "$max" ] && max=$n10
done

allocator_num=$(grep -oE 'ORPHAN-ALLOCATOR: O-[0-9]+' "$FILE" | head -1 | grep -oE '[0-9]+')
if [ -z "${allocator_num:-}" ]; then
    echo "Σφάλμα: δεν βρέθηκε η γραμμή 'ORPHAN-ALLOCATOR: O-NNN' στο $FILE." >&2
    exit 1
fi
allocator_num10=$((10#$allocator_num))
expected=$((max + 1))

if [ "$allocator_num10" -eq "$expected" ]; then
    agree="ΣΥΜΦΩΝΙΑ"
else
    agree="ΑΣΥΜΦΩΝΙΑ"
fi

if [ ! -f "$BASELINE_FILE" ]; then
    echo "0" > "$BASELINE_FILE"
fi
baseline=$(tr -dc '0-9' < "$BASELINE_FILE")

printf "ΔΙΠΛΑ: %d · ALLOCATOR: O-%03d · ΜΕΓΑΛΥΤΕΡΟΣ: O-%03d | BASELINE (παγωμένο): %d διπλά · %s\n" \
    "$dupes" "$allocator_num10" "$max" "$baseline" "$agree"

fail=0

if [ "$dupes" -gt "$baseline" ]; then
    echo
    echo "✗ ΑΥΞΗΣΗ: $baseline → $dupes διπλά τίτλο(ι) στο $FILE."
    echo "  Δύο ορφανά με τον ίδιο αριθμό. ΔΙΟΡΘΩΣΕ, ΜΗΝ ΣΗΚΩΣΕΙΣ ΤΟ BASELINE."
    fail=1
fi

if [ "$agree" = "ΑΣΥΜΦΩΝΙΑ" ]; then
    echo
    echo "✗ ΑΣΥΜΦΩΝΙΑ: ο allocator λέει O-$(printf '%03d' "$allocator_num10"), ο μεγαλύτερος πιασμένος τίτλος είναι O-$(printf '%03d' "$max") — έπρεπε O-$(printf '%03d' "$expected")."
    echo "  Ενημέρωσε τη γραμμή 'ORPHAN-ALLOCATOR' ΣΤΟ ΙΔΙΟ commit που πήρε τον τελευταίο αριθμό."
    fail=1
fi

exit $fail

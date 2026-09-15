#!/usr/bin/env bash
set -uo pipefail

# allocator-lint.sh — Ο ΑΡΙΘΜΟΣ ΤΟΥ ΕΠΟΜΕΝΟΥ ΕΥΡΗΜΑΤΟΣ ΕΙΝΑΙ ΣΩΣΤΟΣ.
#
# ΓΙΑΤΙ ΥΠΑΡΧΕΙ: το FINDINGS.md έχει κανόνα για τον εαυτό του — «Taking
# a number = incrementing this line IN THE SAME COMMIT that introduces
# the finding» — και παραβιάστηκε ΔΥΟ φορές στις 15/09: το F-101
# γράφτηκε, ο allocator έμεινε στο F-101· το F-104 γράφτηκε, ο
# allocator έμεινε στο F-104. Και τις δύο φορές το έπιασε ο επόμενος
# agent με χειροκίνητο grep, ΜΟΝΟ επειδή το prompt το ζήτησε ρητά.
# Χωρίς αυτό το script, δύο ευρήματα θα είχαν πάρει τον ίδιο αριθμό.
#
# ΤΙ ΜΕΤΡΑΕΙ:
#   (1) ΔΙΠΛΑ — ο ίδιος αριθμός τίτλου εμφανίζεται ≥2 φορές ως τίτλος.
#       Μετρητής, όπως οι άλλοι έξι φρουροί. Baseline παγωμένο: 0.
#   (2) ALLOCATOR == ΜΕΓΑΛΥΤΕΡΟΣ ΠΙΑΣΜΕΝΟΣ + 1 — ΝΑΙ/ΟΧΙ, ΟΧΙ
#       μετρητής. Δεν «χαλαρώνει» ποτέ· είτε συμφωνεί είτε όχι.
#
# ΤΙΤΛΟΣ vs ΠΑΡΑΠΟΜΠΗ, ΕΠΑΛΗΘΕΥΜΕΝΟ ΠΡΙΝ ΓΡΑΦΤΕΙ ΑΥΤΟ (2026-09-16):
# η αγκύλη `[F-NNN]` εμφανίζεται ΑΠΟΚΛΕΙΣΤΙΚΑ σε τίτλους («- **[F-105]
# ...»· 42/42 φορές στο μέτρημα εκείνης της μέρας). Καμία εσωτερική
# παραπομπή («ΣΥΓΓΕΝΙΚΟ ΜΕ F-094») δεν τη χρησιμοποιεί — όλες γράφουν
# το γυμνό F-NNN, χωρίς αγκύλες. Το script μετράει ΜΟΝΟ την αγκυλωτή
# μορφή. Αν αυτός ο διαχωρισμός πάψει να ισχύει ποτέ (μια αγκύλη μπει
# σε παραπομπή), το πλήθος τίτλων θα αποκλίνει απότομα από το
# ιστορικό μοτίβο του — που είναι ακριβώς το είδος απόκλισης που ο
# φρουρός δείχνει, όχι κάτι που κρύβεται σιωπηλά.
#
# ΔΕΝ ΕΛΕΓΧΕΤΑΙ: κενά στην αρίθμηση (π.χ. F-100 δεν πάρθηκε ποτέ,
# μετρημένο 2026-09-15) — δηλωμένα αθώα, ΔΕΝ είναι διπλά.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

FILE="FINDINGS.md"
BASELINE_FILE="scripts/.allocator-baseline"

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

if [ ! -f "$FILE" ]; then
    echo "Σφάλμα: $FILE δεν βρέθηκε." >&2
    exit 1
fi

# ── ΤΙΤΛΟΙ, ΑΠΟΚΛΕΙΣΤΙΚΑ Η ΑΓΚΥΛΩΤΗ ΜΟΡΦΗ ──────────────────────────
mapfile -t NUMS < <(grep -o '\[F-[0-9]\+\]' "$FILE" | grep -o '[0-9]\+')

dupes=0
max=0
declare -A seen
for n in "${NUMS[@]}"; do
    n10=$((10#$n))
    if [ "${seen[$n10]:-0}" -gt 0 ]; then
        dupes=$((dupes + 1))
        echo "   ΔΙΠΛΟ  F-$(printf '%03d' "$n10")  (τίτλος εμφανίζεται ξανά)"
    fi
    seen[$n10]=$(( ${seen[$n10]:-0} + 1 ))
    [ "$n10" -gt "$max" ] && max=$n10
done

allocator_num=$(grep -oE 'NEXT FREE: F-[0-9]+' "$FILE" | head -1 | grep -oE '[0-9]+')
if [ -z "${allocator_num:-}" ]; then
    echo "Σφάλμα: δεν βρέθηκε η γραμμή 'NEXT FREE: F-NNN' στο $FILE." >&2
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

printf "ΔΙΠΛΑ: %d · ALLOCATOR: F-%03d · ΜΕΓΑΛΥΤΕΡΟΣ: F-%03d | BASELINE (παγωμένο): %d διπλά · %s\n" \
    "$dupes" "$allocator_num10" "$max" "$baseline" "$agree"

fail=0

if [ "$dupes" -gt "$baseline" ]; then
    echo
    echo "✗ ΑΥΞΗΣΗ: $baseline → $dupes διπλά τίτλο(ι) στο $FILE."
    echo "  Δύο ευρήματα με τον ίδιο αριθμό. ΔΙΟΡΘΩΣΕ, ΜΗΝ ΣΗΚΩΣΕΙΣ ΤΟ BASELINE."
    fail=1
fi

if [ "$agree" = "ΑΣΥΜΦΩΝΙΑ" ]; then
    echo
    echo "✗ ΑΣΥΜΦΩΝΙΑ: ο allocator λέει F-$(printf '%03d' "$allocator_num10"), ο μεγαλύτερος πιασμένος τίτλος είναι F-$(printf '%03d' "$max") — έπρεπε F-$(printf '%03d' "$expected")."
    echo "  Ενημέρωσε τη γραμμή 'NEXT FREE' ΣΤΟ ΙΔΙΟ commit που πήρε τον τελευταίο αριθμό."
    fail=1
fi

exit $fail

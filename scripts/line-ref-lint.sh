#!/usr/bin/env bash
set -uo pipefail

# line-ref-lint.sh — ΜΙΑ ΠΑΡΑΠΟΜΠΗ ΠΟΥ ΗΤΑΝ ΑΛΗΘΕΙΑ ΤΟ ΠΡΩΙ ΜΠΟΡΕΙ ΝΑ
# ΕΙΝΑΙ ΨΕΜΑ ΤΟ ΒΡΑΔΥ.
#
# ΤΟ ΣΧΕΔΙΟ ΕΙΝΑΙ ΤΗΣ 12/08 και δεν χτίστηκε ποτέ. Το παράδειγμά του:
# ένα έγγραφο έλεγε ότι μια συγκεκριμένη γραμμή γράφει r[i] = l[i] —
# ήταν αλήθεια το πρωί, ψέμα μετά το commit της ίδιας μέρας.
#
# ΤΙ ΤΟ ΞΕΧΩΡΙΖΕΙ ΑΠΟ ΤΟΝ ΠΡΟΗΓΟΥΜΕΝΟ ΦΡΟΥΡΟ: ο reference-lint ρωτάει
# «υπάρχει το αρχείο;». Αυτός ρωτάει «λέει ακόμα η ΓΡΑΜΜΗ αυτό που της
# αποδίδεται;».
#
# ΟΙ ΤΕΣΣΕΡΙΣ ΚΑΤΑΣΤΑΣΕΙΣ, ΑΠΟ ΤΟ ΣΧΕΔΙΟ:
#   ΤΑΙΡΙΑΖΕΙ      το απόσπασμα υπάρχει στη δηλωμένη γραμμή
#   ΜΕΤΑΤΟΠΙΣΤΗΚΕ  βρέθηκε σε ±20 γραμμές — δίνεται η ΝΕΑ γραμμή
#   ΧΑΘΗΚΕ         δεν βρέθηκε πουθενά στο ±20
#   ΧΩΡΙΣ ΑΠΟΣΠΑΣΜΑ  ελέγχεται ΜΟΝΟ ότι η γραμμή υπάρχει
#
# ⚠ ΤΟ ΑΠΟΣΠΑΣΜΑ ΕΙΝΑΙ SUBSTRING, ΟΧΙ ΟΛΗ Η ΓΡΑΜΜΗ. Είναι ρητή απαίτηση
# του σχεδίου και μάθημα του πρωτοκόλλου: «ΑΓΚΙΣΤΡΑ: SUBSTRING ΣΕ
# ΓΡΑΜΜΕΣ, ΟΧΙ ΑΥΤΟΥΣΙΑ ΠΡΟΤΑΣΗ — η αναδίπλωση αλλάζει, το substring
# μένει.» Η σύγκριση γίνεται με shell glob (`case`), όχι regex: ένα
# απόσπασμα κώδικα είναι γεμάτο χαρακτήρες που η regex θα ερμήνευε.
#
# ⚠⚠ ΤΑ ΟΡΙΑ — ΓΡΑΜΜΕΝΑ ΕΔΩ, ΟΧΙ ΜΟΝΟ ΣΤΗΝ ΑΝΑΦΟΡΑ:
#   · Απόσπασμα αναγνωρίζεται ΜΟΝΟ όταν ακολουθεί την παραπομπή στην
#     ΙΔΙΑ γραμμή, μετά από κενό, σε backticks. Μια παραπομπή που
#     περιγράφεται σε πρόζα δύο γραμμές παρακάτω ΔΕΝ πιάνεται —
#     μετριέται ως ΧΩΡΙΣ ΑΠΟΣΠΑΣΜΑ, όχι ως πράσινη.
#   · ΧΩΡΙΣ ΑΠΟΣΠΑΣΜΑ σημαίνει «δεν ελέγχθηκε το ΠΕΡΙΕΧΟΜΕΝΟ», μόνο ότι
#     το αρχείο έχει τόσες γραμμές. ΞΕΧΩΡΙΣΤΟ ΝΟΥΜΕΡΟ, ώστε να μη
#     μοιάζει επιτυχία.
#   · Εύρη γραμμών (`x.rs:10-20`) διαβάζονται ως η ΠΡΩΤΗ γραμμή.
#
# ΑΥΤΟΕΞΑΙΡΕΣΗ ΜΕ ΕΥΡΟΣ, ΟΧΙ ΜΕ ΟΝΟΜΑ: σαρώνονται ΜΟΝΟ τα έξι έγγραφα
# παρακάτω. Το ίδιο το script ζει στο scripts/ και δεν σαρώνεται ποτέ —
# γι' αυτό μπορεί να κουβαλάει παραδείγματα στα σχόλιά του χωρίς να
# μολύνει τη μέτρηση. Τρεις προηγούμενοι φρουροί πλήρωσαν το αντίθετο.

cd "$(dirname "${BASH_SOURCE[0]}")/.."

BASELINE_FILE="scripts/.line-ref-baseline"
RADIUS=20

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

# ── ΤΟ ΕΥΡΟΣ: τα ΕΞΙ έγγραφα που κρατάνε παραπομπές ──────────────────
DOCS=(
    FINDINGS.md
    northstar-v2.md
    PRODUCT_MAP.md
    PROJECT_MEMORY_MAP.md
    docs/certificate-schema-v0.md
    ΑΤΖΕΝΤΑ.md
)

# ── ΕΥΡΕΤΗΡΙΟ ΠΗΓΩΝ, ΜΙΑ ΦΟΡΑ ───────────────────────────────────────
# ⚠ 40 ΒΑΣΙΚΑ ΟΝΟΜΑΤΑ ΕΙΝΑΙ ΟΜΩΝΥΜΑ (μετρημένο 25/08): config.rs,
# export.rs, firewall.rs… Μια παραπομπή με σκέτο όνομα ΔΕΝ προσδιορίζει
# αρχείο. Η άρση γίνεται ΔΟΜΙΚΑ: **το απόσπασμα αποφασίζει** — δοκιμάζονται
# όλοι οι υποψήφιοι και κρατιέται αυτός που το περιέχει. Όταν δεν υπάρχει
# απόσπασμα και οι υποψήφιοι είναι πολλοί, δηλώνεται ΑΜΦΙΣΗΜΟ αντί να
# διαλεχτεί ο πρώτος.
SRC_INDEX=$(find . -name '*.rs' -not -path './target/*' \
                 -not -path './*/target/*' -not -path './.git/*' \
                 -printf '%f\t%P\n' 2>/dev/null | sort)

match=0; moved=0; lost=0; nofile=0; nosnip=0; ambig=0
MOVED_LIST=""; LOST_LIST=""; NOFILE_LIST=""; AMBIG_LIST=""

# ── ΕΞΑΓΩΓΗ, ΜΙΑ ΔΙΟΔΟΣ AWK ─────────────────────────────────────────
# Πεδία χωρισμένα με \x01: το κείμενο των εγγράφων περιέχει κάθε ορατό
# χαρακτήρα, αλλά όχι control bytes.
CANDIDATES=$(awk '
{
    line = $0
    while (match(line, /[A-Za-z0-9_.\/-]+\.rs:[0-9]+/)) {
        ref  = substr(line, RSTART, RLENGTH)
        rest = substr(line, RSTART + RLENGTH)
        line = rest

        # Το απόσπασμα πρέπει να ΑΚΟΛΟΥΘΕΙ αμέσως: [κλείσιμο backtick της
        # ίδιας της παραπομπής] κενό `απόσπασμα`
        probe = rest
        sub(/^`/, "", probe)                 # η παραπομπή ήταν σε backticks
        snip = ""
        if (match(probe, /^[ \t]+`[^`]+`/)) {
            snip = substr(probe, RSTART, RLENGTH)
            sub(/^[ \t]+`/, "", snip)
            sub(/`$/, "", snip)
        }
        print FILENAME "\x01" FNR "\x01" ref "\x01" snip
    }
}' "${DOCS[@]}" 2>/dev/null)

while IFS=$'\x01' read -r doc dln ref snip; do
    [ -n "$ref" ] || continue
    path="${ref%:*}"
    num="${ref##*:}"

    # ── Ποιο αρχείο; ──
    cands=""
    if [ -e "$path" ]; then
        cands="$path"
    else
        base="${path##*/}"
        cands=$(printf '%s\n' "$SRC_INDEX" | awk -F'\t' -v b="$base" '$1==b{print $2}')
        # ── Παραπομπή με τμήμα διαδρομής ──
        # ⚠ ΜΗ-ΣΥΝΕΧΟΜΕΝΗ ΑΝΤΙΣΤΟΙΧΙΣΗ. Η πρώτη μορφή απαιτούσε το
        # γραμμένο τμήμα να είναι ΣΥΝΕΧΟΜΕΝΟ κομμάτι της πραγματικής
        # διαδρομής — και έχανε ΚΑΘΕ παραπομπή που παραλείπει το «src».
        # ΜΕΤΡΗΜΕΝΟ 25/08: τρεις από τις έντεκα «σπασμένες» ήταν ΑΚΡΙΒΕΙΣ
        # (επαληθεύτηκαν με το χέρι, γραμμή προς γραμμή)· απλώς τα
        # έγγραφα γράφουν «<crate>/<αρχείο>» ενώ το δέντρο λέει
        # «lineos/m1/<crate>/src/…/<αρχείο>». Το «src» δεν προσθέτει
        # πληροφορία για άνθρωπο, και δεν πρέπει να απαιτείται από μηχανή.
        # ΚΑΝΟΝΑΣ: τα τμήματα εμφανίζονται ΜΕ ΤΗ ΣΕΙΡΑ, όχι διαδοχικά.
        case "$path" in
            */*)
                re=""
                IFS='/' read -r -A segs <<< "$path" 2>/dev/null \
                    || IFS='/' read -r -a segs <<< "$path"
                for seg in "${segs[@]}"; do
                    esc=$(printf '%s' "$seg" | sed 's/[.[\*^$]/\\&/g')
                    if [ -z "$re" ]; then re="(^|/)${esc}"; else re="${re}(/.*)?/${esc}"; fi
                done
                cands=$(printf '%s\n' "$cands" | grep -E "${re}\$" || true)
                ;;
        esac
    fi
    cands=$(printf '%s\n' "$cands" | sed '/^$/d')
    n=$(printf '%s\n' "$cands" | sed '/^$/d' | wc -l)

    if [ "$n" -eq 0 ]; then
        nofile=$((nofile + 1))
        NOFILE_LIST+="   ΑΡΧΕΙΟ ΔΕΝ ΥΠΑΡΧΕΙ  $doc:$dln  →  $ref"$'\n'
        continue
    fi

    # ── ΧΩΡΙΣ ΑΠΟΣΠΑΣΜΑ: μόνο «υπάρχει η γραμμή;» ──
    if [ -z "$snip" ]; then
        if [ "$n" -gt 1 ]; then
            ambig=$((ambig + 1))
            AMBIG_LIST+="   ΑΜΦΙΣΗΜΟ  $doc:$dln  →  $ref  ($n υποψήφιοι, χωρίς απόσπασμα)"$'\n'
            continue
        fi
        if [ "$(wc -l < "$cands")" -ge "$num" ]; then
            nosnip=$((nosnip + 1))
        else
            lost=$((lost + 1))
            LOST_LIST+="   ΧΑΘΗΚΕ  $doc:$dln  →  $ref  (το αρχείο έχει $(wc -l < "$cands") γραμμές)"$'\n'
        fi
        continue
    fi

    # ── ΜΕ ΑΠΟΣΠΑΣΜΑ: το απόσπασμα διαλέγει και το αρχείο ──
    best=""; best_line=""; best_state=""
    while IFS= read -r c; do
        [ -n "$c" ] || continue
        total=$(wc -l < "$c")
        if [ "$num" -le "$total" ]; then
            cur=$(sed -n "${num}p" "$c")
            case "$cur" in *"$snip"*) best="$c"; best_line="$num"; best_state="match"; break ;; esac
        fi
        lo=$((num - RADIUS)); [ "$lo" -lt 1 ] && lo=1
        hi=$((num + RADIUS)); [ "$hi" -gt "$total" ] && hi=$total
        [ "$lo" -gt "$total" ] && continue
        found=$(awk -v lo="$lo" -v hi="$hi" -v s="$snip" \
                    'NR>=lo && NR<=hi && index($0,s){print NR; exit}' "$c")
        if [ -n "$found" ] && [ -z "$best" ]; then
            best="$c"; best_line="$found"; best_state="moved"
        fi
    done <<< "$cands"

    case "$best_state" in
        match) match=$((match + 1)) ;;
        moved) moved=$((moved + 1))
               MOVED_LIST+="   ΜΕΤΑΤΟΠΙΣΤΗΚΕ  $doc:$dln  →  $ref  ⇒  $best:$best_line"$'\n' ;;
        *)     lost=$((lost + 1))
               LOST_LIST+="   ΧΑΘΗΚΕ  $doc:$dln  →  $ref  \`$snip\`"$'\n' ;;
    esac
done <<< "$CANDIDATES"

printf '%s' "$MOVED_LIST"
printf '%s' "$LOST_LIST"
printf '%s' "$NOFILE_LIST"
printf '%s' "$AMBIG_LIST"
echo
echo "ΤΑΙΡΙΑΖΕΙ:            $match"
echo "ΜΕΤΑΤΟΠΙΣΤΗΚΕ:        $moved   (διορθώσιμες αυτόματα — ξέρουμε τη νέα γραμμή)"
echo "ΧΑΘΗΚΕ:               $lost"
echo "ΑΡΧΕΙΟ ΔΕΝ ΥΠΑΡΧΕΙ:   $nofile"
echo "ΧΩΡΙΣ ΑΠΟΣΠΑΣΜΑ:      $nosnip   (ΔΕΝ ελέγχθηκε περιεχόμενο — μόνο ότι υπάρχει η γραμμή)"
echo "ΑΜΦΙΣΗΜΟ:             $ambig   (ομώνυμα αρχεία, χωρίς απόσπασμα να τα ξεχωρίσει)"

BROKEN=$((lost + nofile))
echo
echo "ΣΠΑΣΜΕΝΕΣ (ΧΑΘΗΚΕ + αρχείο λείπει):  $BROKEN"
echo "ΜΕΤΑΤΟΠΙΣΜΕΝΕΣ:                      $moved"

if [ ! -f "$BASELINE_FILE" ]; then
    echo "BASELINE: (δεν υπάρχει — γράψε: echo '$BROKEN $moved' > $BASELINE_FILE)"
    exit 0
fi
read -r B_BROKEN B_MOVED < "$BASELINE_FILE"
echo "BASELINE (παγωμένο):                 $B_BROKEN σπασμένες · $B_MOVED μετατοπισμένες"

rc=0
if [ "$BROKEN" -gt "$B_BROKEN" ]; then
    echo
    echo "✗ ΑΥΞΗΣΗ ΣΠΑΣΜΕΝΩΝ: $B_BROKEN → $BROKEN (+$((BROKEN - B_BROKEN)))."
    echo "  Παραπομπή που δεν λέει πια αυτό που ισχυρίζεται."
    echo "  ΔΙΟΡΘΩΣΕ ΤΟ, ΜΗΝ ΣΗΚΩΣΕΙΣ ΤΟ BASELINE."
    rc=1
fi
if [ "$moved" -gt "$B_MOVED" ]; then
    echo
    echo "✗ ΑΥΞΗΣΗ ΜΕΤΑΤΟΠΙΣΜΕΝΩΝ: $B_MOVED → $moved (+$((moved - B_MOVED)))."
    echo "  Η νέα γραμμή τυπώνεται παραπάνω — η διόρθωση είναι μηχανική."
    rc=1
fi
[ "$rc" -eq 1 ] && exit 1

if [ "$BROKEN" -lt "$B_BROKEN" ] || [ "$moved" -lt "$B_MOVED" ]; then
    echo
    echo "✓ ΜΕΙΩΣΗ. Σφίξε το baseline:"
    echo "    echo '$BROKEN $moved' > $BASELINE_FILE"
    [ "$STRICT" -eq 1 ] && exit 1
fi

exit 0

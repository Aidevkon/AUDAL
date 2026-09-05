#!/usr/bin/env bash
set -euo pipefail

# threshold-lint.sh — ΚΑΘΕ ΚΑΤΩΦΛΙ ΣΤΗ ΔΙΑΔΡΟΜΗ ΣΥΜΜΟΡΦΩΣΗΣ ΦΕΡΕΙ ΣΦΡΑΓΙΔΑ.
#
# ΓΙΑΤΙ ΥΠΑΡΧΕΙ: έξι φορές βρέθηκε καρφωμένο κατώφλι σε μονοπάτι
# συμμόρφωσης, έξι φορές γράφτηκε στο FINDINGS, μηδέν φορές φράχτηκε.
# Το G_MAX_DB=6.0 λέει «placeholder» από τη γέννησή του (dd1122b,
# 2026-07-02) και ζει δύο μήνες. Κώδικας που αλλάζει ΕΝΑ byte ήχου
# κοκκινίζει σε δύο πύλες· κώδικας που καρφώνει κατώφλι περνάει
# καθαρός. ΔΕΝ είναι απροσεξία — είναι ΑΠΟΥΣΙΑ ΦΡΟΥΡΟΥ.
#
# ΤΕΣΣΕΡΙΣ ΑΠΟΔΕΚΤΕΣ ΣΦΡΑΓΙΔΕΣ, ΙΣΟΤΙΜΕΣ:
#   SOURCE: <url> RETRIEVED: <YYYY-MM-DD>
#   MEASURED: n=<N> <YYYY-MM-DD> [ref: .reports/...]
#   HEARD: <YYYY-MM-DD> [ref: F-0XX]
#   PLACEHOLDER: <λόγος> TRIGGER: <τι το ξεκλειδώνει>
#
# ⚠ ΤΟ SOURCE ΕΙΝΑΙ ΙΣΟΤΙΜΟ ΜΕ ΤΟ MEASURED. Ο κανόνας «ποτέ κατώφλι
#   από θεωρία» ΔΕΝ ισχύει για δημοσιευμένες προδιαγραφές: το −23 του
#   ACX δεν μετριέται, ΔΙΑΒΑΖΕΤΑΙ. Φρουρός που δεν το δέχεται
#   παρακάμπτεται την πρώτη μέρα.
#
# ΠΩΣ ΤΡΕΧΕΙ: ΠΡΟΕΙΔΟΠΟΙΕΙ, ΔΕΝ ΜΠΛΟΚΑΡΕΙ — εκτός αν το πλήθος των
# ασφράγιστων ΑΥΞΗΘΕΙ πάνω από το παγωμένο baseline. Ίδιο μάθημα με το
# northstar-lint: «ένα commit που δεν πρέπει να μπλοκάρεται είναι
# ακριβώς αυτό που κάνει τους ανθρώπους να βάζουν --no-verify».
#   ⇒ οι υπάρχουσες καθαρίζονται με τον χρόνο
#   ⇒ ΚΑΜΙΑ ΝΕΑ δεν μπαίνει ασφράγιστη

cd "$(dirname "${BASH_SOURCE[0]}")/.."

BASELINE_FILE="scripts/.threshold-baseline"

# ── ΤΟ ΕΥΡΟΣ ─────────────────────────────────────────────────────────
# ΑΡΧΙΚΟ, ΜΙΚΡΟ ΚΑΙ ΡΗΤΟ: η διαδρομή συμμόρφωσης και μόνο. Επέκταση σε
# άλλα αρχεία είναι ΔΙΚΗ ΤΗΣ απόφαση, όχι παρενέργεια αυτού του φρουρού.
TARGETS=(
    "lineos/m1/sp314-dsp/src/analysis/acx_check.rs"
    "lineos/m1/lineos-types/src/presets.rs"
    "lineos/m0/m0-daemon/src/handlers/export.rs"
    "lineos/m0/m0-daemon/src/handlers/deliver.rs"
    "shared/aether-bridge/src/reference_resolver.rs"
    "shared/aether-bridge/src/lib.rs"
)

STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

scan_file() {
    local f="$1"
    # Ο κώδικας των tests ΔΕΝ σαρώνεται: ένα `assert!(x > 0.5)` δεν είναι
    # κατώφλι συμμόρφωσης — είναι η ίδια η μέτρηση. Κόβουμε στο πρώτο
    # `#[cfg(test)]`.
    local cut
    cut=$(grep -n '#\[cfg(test)\]' "$f" 2>/dev/null | head -1 | cut -d: -f1 || true)
    [ -z "$cut" ] && cut=$(( $(wc -l < "$f") + 1 ))

    awk -v FNAME="$f" -v CUT="$cut" '
    NR >= CUT { exit }

    {
        raw = $0
        line = raw
        sub(/^[ \t]+/, "", line)

        # ── Γραμμή-σχόλιο: κρατιέται ΜΟΝΟ ως φορέας σφραγίδας ──
        if (line ~ /^\/\//) {
            if (in_block == 0) { block = ""; in_block = 1 }
            block = block " " line
            next
        }
        if (line ~ /^[ \t]*$/) { in_block = 0; block = ""; next }

        # ── ΨΕΥΔΩΣ ΘΕΤΙΚΑ — ΔΟΜΙΚΑ, ΟΧΙ ΑΝΑ ΤΙΜΗ ──────────────────
        # (1) bit manipulation: τα << >> ταιριάζουν κατά λάθος με το
        #     regex σύγκρισης, και τα 0x/1023/63 δεν είναι κατώφλια.
        skip = 0
        if (line ~ /<</ || line ~ />>/ || line ~ /0x/) skip = 1
        # (2) FFI/πρόσημο: `if r < 0`, `if written > 0` — κωδικοί
        #     επιστροφής της C, όχι κατώφλια.
        if (line ~ /[<>]=?[ \t]*0[ \t]*[;{)]/ || line ~ /[<>]=?[ \t]*0$/) skip = 1
        # (3) epsilon guards: 1e-10 κ.λπ. — φρουροί διαίρεσης με μηδέν.
        if (line ~ /[0-9]e-[0-9]/) skip = 1
        # (4) μήκη/δομή buffer: `.len() < 12` — όριο δομής αρχείου.
        if (line ~ /\.len\(\)[ \t]*[<>]/) skip = 1
        # (5) κείμενο μέσα σε string: τα μηνύματα του `format!`
        #     περιέχουν «> 4.0 dB» ως ΚΕΙΜΕΝΟ, όχι ως σύγκριση.
        if (line ~ /^"/ || line ~ /^[a-z_]*!\(/ || line ~ /"[^"]*[<>]=?[ \t]*-?[0-9]/) skip = 1

        if (skip) { in_block = 0; block = ""; next }

        # ── ΤΙ ΕΙΝΑΙ ΚΑΤΩΦΛΙ ──────────────────────────────────────
        is_thr = 0
        if (line ~ /^(pub )?const [A-Z_0-9]+:[ \t]*(f32|f64|i32|u32|usize|i64|u64)[ \t]*=/) is_thr = 1
        if (line ~ /[<>]=?[ \t]*-?[0-9]+\.?[0-9_]*/) is_thr = 1

        if (!is_thr) { in_block = 0; block = ""; next }

        # ── Η ΣΦΡΑΓΙΔΑ: στο μπλοκ σχολίων ΑΠΟ ΠΑΝΩ ή στο ίδιο το line ──
        hay = block " " raw
        stamp = "ΑΣΦΡΑΓΙΣΤΟ"
        if (hay ~ /SOURCE:/ && hay ~ /RETRIEVED:[ \t]*[0-9]{4}-[0-9]{2}-[0-9]{2}/) stamp = "SOURCE"
        else if (hay ~ /MEASURED:[ \t]*n=[0-9]+[ \t]+[0-9]{4}-[0-9]{2}-[0-9]{2}/)   stamp = "MEASURED"
        else if (hay ~ /HEARD:[ \t]*[0-9]{4}-[0-9]{2}-[0-9]{2}/)                    stamp = "HEARD"
        else if (hay ~ /PLACEHOLDER:/ && hay ~ /TRIGGER:/)                          stamp = "PLACEHOLDER"

        # Το trailing σχόλιο κόβεται ΓΙΑ ΤΗΝ ΕΜΦΑΝΙΣΗ μόνο — έχει ήδη
        # ελεγχθεί για σφραγίδα παραπάνω (μπαίνει στο `hay`).
        # ΔΕΝ κόβουμε με substr(): τα σχόλια είναι ελληνικά, και μια
        # τομή στη μέση χαρακτήρα UTF-8 βγάζει μισό byte που σπάει τον
        # `read` του καλούντος — το σφάλμα που έκρυβε μια ολόκληρη
        # γραμμή στην πρώτη εκτέλεση.
        code = line
        sub(/[ \t]*\/\/.*$/, "", code)
        printf "%s:%d\t%s\t%s\n", FNAME, NR, stamp, code

        # ΜΙΑ ΣΦΡΑΓΙΔΑ ΑΝΑ ΚΑΤΩΦΛΙ — το μπλοκ ΚΑΘΑΡΙΖΕΤΑΙ.
        #
        # Δοκιμάστηκε το αντίθετο (μια συνεχόμενη ομάδα να μοιράζεται τη
        # σφραγίδα, λογική «ένα ζεύγος min/max έχει μία πηγή») και
        # ΜΕΤΡΗΜΕΝΑ παρήγαγε ΨΕΥΔΗ ΣΦΡΑΓΙΔΑ: το `MEASURED: n=59` του
        # ACX_MARGIN_RMS_FLOOR_DB κάλυψε τις δύο επόμενες γραμμές, που
        # ο ίδιος ο κώδικας σημειώνει `⚠ n=1`. Φρουρός που σφραγίζει
        # σιωπηλά τον γείτονα με ΞΕΝΗ προέλευση είναι χειρότερος από
        # καθόλου φρουρό. Το κόστος είναι μια επανάληψη σχολίου σε
        # ζεύγη min/max — φτηνό, και ρητό.
        in_block = 0; block = ""
    }
    ' "$f"
}

echo "── threshold-lint · διαδρομή συμμόρφωσης ──────────────────────"
echo

total_unsealed=0
tmp=$(mktemp); trap 'rm -f "$tmp"' EXIT

for f in "${TARGETS[@]}"; do
    if [ ! -f "$f" ]; then
        echo "Σφάλμα: στόχος δεν βρέθηκε: $f" >&2
        exit 1
    fi
    scan_file "$f" > "$tmp"
    n_all=$(wc -l < "$tmp")
    n_un=$(grep -c 'ΑΣΦΡΑΓΙΣΤΟ' "$tmp" || true)
    total_unsealed=$(( total_unsealed + n_un ))

    printf "%s  —  %d κατώφλια, %d ασφράγιστα\n" "$f" "$n_all" "$n_un"
    while IFS=$'\t' read -r loc stamp txt; do
        [ -z "$loc" ] && continue
        printf "   %-12s %-28s %s\n" "$stamp" "${loc##*/}" "$txt"
    done < "$tmp"
    echo
done

echo "───────────────────────────────────────────────────────────────"
echo "ΣΥΝΟΛΟ ΑΣΦΡΑΓΙΣΤΩΝ: $total_unsealed"

if [ ! -f "$BASELINE_FILE" ]; then
    echo "$total_unsealed" > "$BASELINE_FILE"
    echo "baseline ΓΡΑΦΤΗΚΕ: $total_unsealed → $BASELINE_FILE"
    exit 0
fi

baseline=$(tr -dc '0-9' < "$BASELINE_FILE")
echo "BASELINE (παγωμένο):  $baseline"

if [ "$total_unsealed" -gt "$baseline" ]; then
    echo
    echo "✗ ΑΥΞΗΣΗ: $baseline → $total_unsealed (+$(( total_unsealed - baseline )))."
    echo "  ΝΕΟ ασφράγιστο κατώφλι στη διαδρομή συμμόρφωσης."
    echo "  Βάλε ΜΙΑ από τις τέσσερις σφραγίδες σε σχόλιο ΑΠΟ ΠΑΝΩ:"
    echo "    SOURCE: <url> RETRIEVED: <YYYY-MM-DD>"
    echo "    MEASURED: n=<N> <YYYY-MM-DD> [ref: .reports/...]"
    echo "    HEARD: <YYYY-MM-DD> [ref: F-0XX]"
    echo "    PLACEHOLDER: <λόγος> TRIGGER: <τι το ξεκλειδώνει>"
    exit 1
fi

if [ "$total_unsealed" -lt "$baseline" ]; then
    echo
    echo "✓ ΜΕΙΩΣΗ: $baseline → $total_unsealed. Σφίξε το baseline:"
    echo "    echo $total_unsealed > $BASELINE_FILE"
    [ "$STRICT" = "1" ] && exit 1
fi

exit 0

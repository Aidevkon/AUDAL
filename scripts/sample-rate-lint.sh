#!/usr/bin/env bash
set -uo pipefail

# sample-rate-lint.sh — Ο ρυθμός δειγματοληψίας είναι ΜΙΑ δήλωση
# (lineos_types::analysis::ANALYSIS_SAMPLE_RATE) — όλα τα άλλα την
# εισάγουν ή δεν υπάρχουν καθόλου.
#
# ΓΙΑΤΙ ΥΠΑΡΧΕΙ: F-103 βρήκε πέντε ανεξάρτητες δηλώσεις της ΙΔΙΑΣ
# παραδοχής και εννιά γυμνά αντίγραφα της τιμής — το F-102 crash ήταν
# η μόνη από αυτές τις θέσεις που έσκασε, γι' αυτό φάνηκε. Οι άλλες
# οκτώ έδιναν λάθος αριθμό σιωπηλά. Το recon της 15/09 απέρριψε ΔΥΟ
# αυτόματους ανιχνευτές (ονομάτων: G_MAX_DB "καθαρό" ενώ έχει 5 θέσεις·
# τιμών: το 6.0 έχει 35 άσχετα αρχεία) — και τα δύο άχρηστα ΓΕΝΙΚΑ.
# Αυτός ο φρουρός ΔΕΝ είναι γενικός· κλειδώνει ΕΝΑ συγκεκριμένο,
# ήδη-γνωστό μέγεθος, όπως ακριβώς κάνουν οι άλλοι πέντε φρουροί για
# τα δικά τους μεγέθη.
#
# ΔΥΟ ΜΕΤΡΗΤΕΣ:
#   ΔΗΛΩΣΕΙΣ — πόσες φορές δηλώνεται το ANALYSIS_SAMPLE_RATE ως const
#     (`use ... as X` ΔΕΝ μετράει — είναι εισαγωγή, όχι δήλωση).
#   ΓΥΜΝΑ    — πόσες φορές εμφανίζεται 48000/48_000/48000.0 ως ΩΜΟ
#     literal, οπουδήποτε στο δέντρο (.rs ΚΑΙ .json, ΚΑΙ WS2 —
#     ΣΚΟΠΙΜΑ, το threshold-lint σαρώνει μόνο .rs σε 6 αρχεία και γι'
#     αυτό το G_MAX_DB έμεινε αόρατο δύο μήνες), εκτός από τη γραμμή
#     της ίδιας της δήλωσης.
#
# ΠΩΣ ΤΡΕΧΕΙ: ΠΡΟΕΙΔΟΠΟΙΕΙ, ΔΕΝ ΜΠΛΟΚΑΡΕΙ — εκτός αν ΚΑΠΟΙΟΣ από τους
# δύο μετρητές ΑΥΞΗΘΕΙ πάνω από το παγωμένο baseline. Ίδιο μάθημα με
# το threshold-lint: «ένα commit που δεν πρέπει να μπλοκάρεται είναι
# ακριβώς αυτό που κάνει τους ανθρώπους να βάζουν --no-verify».

cd "$(dirname "${BASH_SOURCE[0]}")/.."

BASELINE_FILE="scripts/.sample-rate-baseline"
STRICT=0
[ "${1:-}" = "--strict" ] && STRICT=1

# ── ΔΗΛΩΣΕΙΣ ─────────────────────────────────────────────────────────
# `const ANALYSIS_SAMPLE_RATE :` — pub ή ιδιωτικό, οπουδήποτε. ΟΧΙ
# `use ... as ANALYSIS_SAMPLE_RATE` (εκείνο εισάγει, δεν δηλώνει).
decl_hits=$(grep -rnE '^[[:space:]]*(pub[[:space:]]+)?const[[:space:]]+ANALYSIS_SAMPLE_RATE[[:space:]]*:' \
    --include='*.rs' --exclude-dir=target . 2>/dev/null || true)
DECLS=$(echo -n "$decl_hits" | grep -c . || true)

# ── ΓΥΜΝΑ 48000 / 48_000 / 48000.0 ──────────────────────────────────
# Σαρώνει .rs ΚΑΙ .json, όλο το δέντρο εκτός /target — ΚΑΙ το WS2
# (research/), σκόπιμα: εκεί ζουν τα «δύο research binaries» του
# G_MAX_DB, αόρατα σε κάθε προηγούμενο φρουρό.
#
# ΕΞΑΙΡΕΙ: /target · tests/ καταλόγους · #[cfg(test)] μπλοκ μέσα σε
# αρχείο παραγωγής (ίδιο cutoff-ιδίωμα με το threshold-lint.sh) ·
# γραμμές-σχόλιο · την ίδια τη γραμμή δήλωσης του ANALYSIS_SAMPLE_RATE.
# Δοκιμάστηκε ΧΩΡΙΣ αυτές τις εξαιρέσεις: 100+ αχρείαστα ευρήματα, όλα
# helpers τεστ (π.χ. write_wav_6ch(path, 48_000, 3.3)) — το ΙΔΙΟ
# πρόβλημα θορύβου που απέρριψε τον γενικό ανιχνευτή τιμών στο recon.
# Ο κίνδυνος που μας νοιάζει (παραγωγή, .json, WS2 binaries) ζει ΕΞΩ
# από #[cfg(test)] — δεν χάνεται τίποτα από το γνωστό θετικό.
scan_bare_in_file() {
    local f="$1"
    local cut
    cut=$(grep -n '#\[cfg(test)\]' "$f" 2>/dev/null | head -1 | cut -d: -f1 || true)
    [ -z "$cut" ] && cut=$(( $(wc -l < "$f") + 1 ))
    awk -v FNAME="$f" -v CUT="$cut" '
    NR >= CUT { exit }
    {
        raw = $0
        line = raw
        sub(/^[ \t]+/, "", line)
        if (line ~ /^\/\//) next
        if (line ~ /^\*/) next
        if (line ~ /ANALYSIS_SAMPLE_RATE/) next
        if (line ~ /(^|[^0-9_A-Za-z.])48_?000(\.0)?([^0-9_A-Za-z]|$)/) {
            printf "%s:%d:%s\n", FNAME, NR, line
        }
    }' "$f"
}

# ΜΟΝΟ αρχεία ΠΑΡΑΚΟΛΟΥΘΟΥΜΕΝΑ από το git — όχι /target (ήδη εκτός
# git), όχι corpus_data/ ή άλλα παραγόμενα δεδομένα εκτός git (π.χ.
# corpus_data/*.corpus.json είναι .gitignored: 344 «ευρήματα» ήταν
# όλα "start_ms"/"end_ms" σε ηχογραφήσεις χρόνου, τυχαία ίδιος αριθμός
# με το sample rate — άσχετο, ΟΧΙ πηγαίος κώδικας ή ρύθμιση).
bare_hits=""
while IFS= read -r f; do
    [[ "$f" == */tests/* ]] && continue
    hit=$(scan_bare_in_file "$f")
    [ -n "$hit" ] && bare_hits="${bare_hits}${bare_hits:+$'\n'}${hit}"
done < <(git ls-files -- '*.rs' '*.json')
BARE=$(printf '%s' "$bare_hits" | grep -c . || true)

echo "── sample-rate-lint · μία δήλωση, ο ρυθμός κλειδωμένος ─────────"
echo
echo "ΔΗΛΩΣΕΙΣ ΤΟΥ ANALYSIS_SAMPLE_RATE:"
[ -n "$decl_hits" ] && echo "$decl_hits" | sed 's/^/  /'
echo "ΣΥΝΟΛΟ ΔΗΛΩΣΕΩΝ: $DECLS"
echo
echo "ΓΥΜΝΑ 48000/48_000/48000.0 (εκτός δήλωσης):"
[ -n "$bare_hits" ] && echo "$bare_hits" | sed 's/^/  /'
echo "ΣΥΝΟΛΟ ΓΥΜΝΩΝ: $BARE"
echo

if [ ! -f "$BASELINE_FILE" ]; then
    echo "$DECLS $BARE" > "$BASELINE_FILE"
    echo "baseline ΓΡΑΦΤΗΚΕ: $DECLS δηλώσεις · $BARE γυμνά → $BASELINE_FILE"
    exit 0
fi

read -r B_DECLS B_BARE < "$BASELINE_FILE"
echo "BASELINE (παγωμένο): $B_DECLS δηλώσεις · $B_BARE γυμνά"

rc=0
if [ "$DECLS" -gt "$B_DECLS" ]; then
    echo
    echo "✗ ΑΥΞΗΣΗ ΔΗΛΩΣΕΩΝ: $B_DECLS → $DECLS (+$((DECLS - B_DECLS)))."
    echo "  ΝΕΑ ανεξάρτητη δήλωση του ρυθμού — εισήγαγε την υπάρχουσα αντί να δηλώσεις νέα."
    rc=1
fi
if [ "$BARE" -gt "$B_BARE" ]; then
    echo
    echo "✗ ΑΥΞΗΣΗ ΓΥΜΝΩΝ: $B_BARE → $BARE (+$((BARE - B_BARE)))."
    echo "  ΝΕΟ γυμνό 48000 κάπου στο δέντρο — χρησιμοποίησε lineos_types::analysis::ANALYSIS_SAMPLE_RATE."
    rc=1
fi
[ "$rc" -eq 1 ] && exit 1

if [ "$DECLS" -lt "$B_DECLS" ] || [ "$BARE" -lt "$B_BARE" ]; then
    echo
    echo "✓ ΜΕΙΩΣΗ. Σφίξε το baseline:"
    echo "    echo '$DECLS $BARE' > $BASELINE_FILE"
    [ "$STRICT" -eq 1 ] && exit 1
fi

exit 0

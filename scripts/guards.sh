#!/usr/bin/env bash
# Τρέχει τους πέντε γρήγορους φρουρούς και δείχνει τα
# παγωμένα πλήθη. Ο έκτος (run-ignored.sh) θέλει ~28
# λεπτά και τρέχει χωριστά.
#
# Κάθε φρουρός: ΠΡΟΕΙΔΟΠΟΙΕΙ, δεν μπλοκάρει· κοκκινίζει
# ΜΟΝΟ σε αύξηση. Τα υπάρχοντα καθαρίζονται με τον
# χρόνο· κανένα νέο δεν μπαίνει.
set -uo pipefail
cd "$(dirname "$0")/.."
FAIL=0
for s in threshold-lint gate-coverage empty-pass-lint \
         reference-lint line-ref-lint; do
  printf "%-18s " "$s"
  out=$("scripts/$s.sh" 2>&1); rc=$?
  echo "$out" | tail -1
  [ $rc -ne 0 ] && FAIL=1
done
echo
echo "run-ignored        (~28 λεπτά — scripts/run-ignored.sh)"
exit $FAIL

#!/bin/bash
FAIL=0
CHECKED=0
for fixture in lineos/m1/sp314-dsp/tests/fixtures/*.json; do
    lock="${fixture%.json}.lock"
    if [ -f "$lock" ]; then
        expected=$(cat "$lock")
        actual=$(python3 -c "
import hashlib
with open('$fixture', 'rb') as f:
    print(hashlib.sha256(f.read()).hexdigest())
")
        if [ "$expected" != "$actual" ]; then
            echo "TAMPERED: $fixture"
            FAIL=1
        else
            echo "OK: $(basename $fixture)"
            CHECKED=$((CHECKED+1))
        fi
    fi
done
echo "Verified: $CHECKED fixtures"
[ $FAIL -eq 0 ] && echo "All fixtures clean" || echo "INTEGRITY VIOLATION"
exit $FAIL

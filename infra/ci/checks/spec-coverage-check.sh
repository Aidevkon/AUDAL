#!/bin/bash
# G-011: Spec Coverage (warning only — never blocks merge)
WARNINGS=0

for module in chaos semantic mapping intent tuning; do
    if ! ls spec/locked/S-0*"$module"* 2>/dev/null \
       | grep -q .; then
        echo "⚠️  G-011: No locked spec for aether/$module/"
        WARNINGS=$((WARNINGS+1))
    fi
done
echo "ℹ️  G-011: aether/personas/ covered by S-003"
echo "ℹ️  G-011: aether/control/ covered by S-011a"

[ $WARNINGS -gt 0 ] && \
    echo "⚠️  G-011: $WARNINGS warning(s) — non-blocking"
echo "✅ G-011: Spec coverage check complete"
exit 0

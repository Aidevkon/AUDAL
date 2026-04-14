#!/usr/bin/env python3
"""
Validate all JSON schema files in lineos/shared/schema/.
Used by: just validate-schemas
Authority: LineOS Constitution v2.0
"""
import json
import os
import sys

errors = 0
schema_dir = 'lineos/shared/schema'

if not os.path.isdir(schema_dir):
    print(f'  ⚠️  {schema_dir} not found — ok for Phase 1')
    sys.exit(0)

for f in sorted(os.listdir(schema_dir)):
    if not f.endswith('.json'):
        continue
    path = os.path.join(schema_dir, f)
    if os.path.getsize(path) == 0:
        print(f'  ⚠️  {f}  (empty stub — ok)')
        continue
    try:
        json.load(open(path))
        print(f'  ✅ {f}')
    except json.JSONDecodeError as e:
        print(f'  ❌ {f}: {e}')
        errors += 1

sys.exit(errors)

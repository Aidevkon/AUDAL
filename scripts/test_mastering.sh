#!/bin/bash
AUDIO=${1:-"/home/aidevcon/Music/test.wav"}
echo "=== Creator OS Mastering Test ==="
echo -n "1. Health... "
curl -sf http://127.0.0.1:7401/health > /dev/null && echo "OK" || { echo "FAIL — start m0d first"; exit 1; }
echo -n "2. Mastering... "
START=$(date +%s)
RESULT=$(curl -s --max-time 120 -X POST http://127.0.0.1:7402/master \
  -H "Content-Type: application/json" \
  -d "{\"audio_path\":\"$AUDIO\",\"preset_id\":\"spotify\",\"flavour_id\":\"clean\",\"intent_warmth\":0.5,\"intent_punch\":0.5,\"intent_space\":0.5,\"intent_loudness\":0.5}")
END=$(date +%s)
echo "Done in $((END-START))s"
STATUS=$(echo $RESULT | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','?'))" 2>/dev/null)
if [ "$STATUS" = "ok" ]; then
    BLOB=$(echo $RESULT | python3 -c "import sys,json; print(json.load(sys.stdin).get('blob_id',''))" 2>/dev/null)
    echo "✅ SUCCESS — blob_id: $BLOB"
else
    MSG=$(echo $RESULT | python3 -c "import sys,json; print(json.load(sys.stdin).get('message','unknown'))" 2>/dev/null)
    echo "❌ FAILED — $MSG"
fi

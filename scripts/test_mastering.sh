#!/bin/bash
AUDIO=${1:-"/home/aidevcon/Music/test.wav"}
echo "=== Creator OS Mastering Test ==="
echo -n "1. Health... "
curl -sf http://127.0.0.1:7401/health > /dev/null && echo "OK" || { echo "FAIL — start m0d first"; exit 1; }
echo -n "2. Mastering... "
START=$(date +%s)
RESULT=$(curl -s --max-time 300 -X POST http://127.0.0.1:7402/master \
  -H "Content-Type: application/json" \
  -d "{\"audioPath\":\"$AUDIO\",\"presetId\":\"spotify\",\"flavourId\":\"clean\",\"intentWarmth\":0.5,\"intentPunch\":0.5,\"intentSpace\":0.5,\"intentLoudness\":0.5}")
END=$(date +%s)
echo "Done in $((END-START))s"
STATUS=$(echo $RESULT | python3 -c "import sys,json; print(json.load(sys.stdin).get('status','?'))" 2>/dev/null)
if [ "$STATUS" = "ok" ]; then
    BLOB=$(echo $RESULT | python3 -c "import sys,json; print(json.load(sys.stdin).get('blob_id',''))" 2>/dev/null)
    echo "✅ SUCCESS — blob_id: $BLOB"
    curl -s --max-time 300 -X POST http://127.0.0.1:7402/export \
      -H "Content-Type: application/json" \
      -d "{\"blobId\":\"$BLOB\",\"format\":\"wav\",\"outputPath\":\"/home/aidevcon/Music/test_mastered.wav\"}" > /dev/null
else
    MSG=$(echo $RESULT | python3 -c "import sys,json; print(json.load(sys.stdin).get('message','unknown'))" 2>/dev/null)
    echo "❌ FAILED — $MSG"
fi

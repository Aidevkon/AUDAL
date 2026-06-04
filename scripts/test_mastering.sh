#!/bin/bash
AUDIO=${1:-"/home/aidevcon/Music/test.wav"}
echo "=== Creator OS Mastering Test ==="
echo -n "1. Health... "
curl -sf http://127.0.0.1:7401/health > /dev/null && echo "OK" || { echo "FAIL — start m0d first"; exit 1; }
echo -n "2. Mastering... "
START=$(date +%s)
DAEMON_URL="http://127.0.0.1:7402"
RESULT=$(curl -s --max-time 300 -X POST "$DAEMON_URL/master" \
  -H "Content-Type: application/json" \
  -d "{\"audioPath\":\"$AUDIO\",\"presetId\":\"spotify\",\"flavourId\":\"clean\",\"intentWarmth\":0.5,\"intentPunch\":0.5,\"intentSpace\":0.5,\"intentLoudness\":0.5}")

JOB=$(echo $RESULT | python3 -c "import sys,json; print(json.load(sys.stdin).get('job_id',''))" 2>/dev/null)

if [ -z "$JOB" ]; then
    # Fallback: old format with blob_id
    BLOB=$(echo $RESULT | python3 -c "import sys,json; print(json.load(sys.stdin).get('blob_id',''))" 2>/dev/null)
else
    # New format: poll until CERTIFIED
    echo "Got job_id: $JOB, polling..."
    while true; do
        PROG=$(curl -s "$DAEMON_URL/progress/$JOB")
        STAGE=$(echo $PROG | python3 -c "import sys,json; print(json.load(sys.stdin).get('stage',''))" 2>/dev/null)
        echo "  Stage: $STAGE"
        if [ "$STAGE" = "CERTIFIED" ]; then
            BLOB=$(echo $PROG | python3 -c "import sys,json; print(json.load(sys.stdin).get('blob_id',''))" 2>/dev/null)
            break
        elif [ "$STAGE" = "ERROR" ]; then
            echo "❌ FAILED during mastering"
            exit 1
        fi
        sleep 0.5
    done
fi
END=$(date +%s)
echo "Done in $((END-START))s"

echo "✅ SUCCESS — blob_id: $BLOB"
curl -s --max-time 300 -X POST http://127.0.0.1:7402/export \
  -H "Content-Type: application/json" \
  -d "{\"blob_id\":\"$BLOB\",\"format\":\"wav\",\"output_path\":\"/home/aidevcon/Music/no7salt_mastered.wav\"}" > /dev/null

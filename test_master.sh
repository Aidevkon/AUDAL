#!/bin/bash
set -e

# Wait for m0d to start
sleep 2

echo "Triggering master..."
JOB_ID=$(curl -s -X POST http://127.0.0.1:7402/master \
  -H "Content-Type: application/json" \
  -d '{
    "audioPath": "/home/aidevcon/Music/test.wav",
    "presetId": "spotify"
  }' | jq -r .job_id)

echo "Job ID: $JOB_ID"

if [ "$JOB_ID" == "null" ] || [ -z "$JOB_ID" ]; then
    echo "Failed to get Job ID!"
    exit 1
fi

echo "Polling progress..."
while true; do
  RESP=$(curl -s http://127.0.0.1:7402/progress/$JOB_ID)
  STAGE=$(echo "$RESP" | jq -r .stage)
  echo "Stage: $STAGE"
  
  if [ "$STAGE" == "CERTIFIED" ]; then
    BLOB_ID=$(echo "$RESP" | jq -r .blob_id)
    echo "Done! Blob ID: $BLOB_ID"
    
    echo "Fetching blob..."
    BLOB_RESP=$(curl -s http://127.0.0.1:7402/blob/$BLOB_ID)
    BLOB_STATUS=$(echo "$BLOB_RESP" | jq -r '.id')
    if [ "$BLOB_STATUS" == "$BLOB_ID" ]; then
      echo "Blob successfully fetched!"
    else
      echo "Failed to fetch blob!"
      echo "$BLOB_RESP"
      exit 1
    fi
    exit 0
  elif [ "$STAGE" == "ERROR" ]; then
    echo "Failed!"
    exit 1
  fi
  sleep 1
done

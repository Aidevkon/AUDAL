#!/bin/bash
set -u

BASE_DIR="$(pwd)/research/musdb-lab"
EXCERPTS_DIR="${BASE_DIR}/excerpts"
OUT_DIR="/tmp/w5c"
RESULTS_CSV="${OUT_DIR}/results.csv"

# Create output directories
mkdir -p "${OUT_DIR}"

# Initialize CSV header if not exists
if [ ! -f "${RESULTS_CSV}" ]; then
    echo "track,branch,role,sdr,sir,sar" > "${RESULTS_CSV}"
fi

# Find valid tracks
echo "Starting W5.c batch run..."

for track_path in "${EXCERPTS_DIR}"/*/; do
    # Remove trailing slash for basename
    track_path="${track_path%/}"
    track_name="$(basename "$track_path")"
    
    # Check if valid GT
    if [ ! -f "${track_path}/mixture.wav" ] || \
       [ ! -f "${track_path}/vocals.wav" ] || \
       [ ! -f "${track_path}/bass.wav" ] || \
       [ ! -f "${track_path}/drums.wav" ] || \
       [ ! -f "${track_path}/other.wav" ]; then
        echo "Skipping ${track_name} (MISSING GT)"
        continue
    fi
    
    echo "=========================================================="
    echo "Processing track: ${track_name}"
    
    # Run extractor
    track_out="${OUT_DIR}/${track_name}"
    echo "Running oracle_extract..."
    if ! "${BASE_DIR}/oracle_extract/target/release/oracle_extract" --semantic "${track_path}/mixture.wav" "${track_out}"; then
        echo "ERROR: Extractor failed for ${track_name}. Skipping to next."
        echo "${track_name},ERROR,ERROR,NaN,NaN,NaN" >> "${RESULTS_CSV}"
        continue
    fi
    
    # Run eval for nmf5
    echo "Running eval for nmf5..."
    if ! "${BASE_DIR}/venv/bin/python" "${BASE_DIR}/semantic_eval.py" "${track_path}" "${track_out}/nmf5" "${RESULTS_CSV}"; then
        echo "ERROR: Eval failed for ${track_name} (nmf5)."
    fi
    
    # Run eval for nmfd8
    echo "Running eval for nmfd8..."
    if ! "${BASE_DIR}/venv/bin/python" "${BASE_DIR}/semantic_eval.py" "${track_path}" "${track_out}/nmfd8" "${RESULTS_CSV}"; then
        echo "ERROR: Eval failed for ${track_name} (nmfd8)."
    fi
    
    echo "Finished track: ${track_name}"
    echo "=========================================================="
done

echo "Batch run completed. Results saved to ${RESULTS_CSV}"

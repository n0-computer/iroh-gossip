#!/bin/bash
# Runs one cell for several binaries, interleaved, and appends JSON lines.
#
# Usage: run.sh <out.jsonl> <reps> "<bin> <bin> ..." -- <slowbench args>
set -euo pipefail
out=$1; reps=$2; bins=($3); shift 3; [ "${1:-}" = "--" ] && shift
for rep in $(seq 1 "$reps"); do
  n=${#bins[@]}
  for k in $(seq 0 $((n-1))); do
    bin=${bins[$(( (k + rep) % n ))]}
    timeout 600 "$bin" --label "$(basename "$bin")" "$@" >> "$out" 2>> "${out%.jsonl}.err" \
      || echo "{\"label\":\"$(basename "$bin")\",\"failed\":true}" >> "$out"
  done
done

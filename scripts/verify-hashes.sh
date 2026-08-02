#!/usr/bin/env bash
# Judges: run this. Non-zero exit == an original artifact was modified.
set -euo pipefail
cd "$(dirname "$0")/.."
grep -v '^#' tests/HASHES.txt | grep -v '^$' | shasum -a 256 -c -

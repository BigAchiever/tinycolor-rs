#!/usr/bin/env bash
# Regenerates tests/HASHES.txt. Run once at kickoff; never again.
set -euo pipefail
cd "$(dirname "$0")/.."
{
  echo "# Port Mortem — original-artifact hash manifest"
  echo "# upstream: https://github.com/bgrins/TinyColor"
  echo "# pinned commit: $(cat upstream/PINNED_COMMIT)"
  echo "# algorithm: sha256"
  echo "#"
  for f in \
    "tests/original/test.js" \
    "tests/original/package.json" \
    "tests/deno_asserts@0.168.0.mjs" \
    "upstream/tinycolor.reference.js" ; do
    printf "%s  %s\n" "$(shasum -a 256 "$f" | cut -d' ' -f1)" "$f"
  done
} > tests/HASHES.txt
echo "wrote tests/HASHES.txt"

#!/usr/bin/env bash
# Warms everything the demo video needs, so no shot waits on a compile.
set -euo pipefail
cd "$(dirname "$0")/.."

say() { printf "\n\033[1m%s\033[0m\n" "$*"; }

say "building both artifacts"
cargo build --release -p tinycolor-cli 2>&1 | tail -1
wasm-pack build crates/wasm --target nodejs \
  --out-dir ../../tests/original/pkg --out-name tinycolor_wasm 2>&1 | grep -E "✨|error"

say "warming both transports (so the on-camera run is instant)"
node tests/run-all.mjs >/dev/null 2>&1 && echo "  wasm   warm"
TINYCOLOR_TRANSPORT=native node tests/run-all.mjs >/dev/null 2>&1 && echo "  native warm"

say "warming the browser demo bundle"
[ -f web/tinycolor.js ] || echo "  MISSING web/tinycolor.js — run: UPSTREAM_DIR=<path> node scripts/build-demo.mjs"
[ -f web/tinycolor.js ] && echo "  present ($(du -h web/tinycolor.js | cut -f1))"

say "dry run of every on-camera command"
./scripts/verify-hashes.sh >/dev/null && echo "  hashes            OK"
node tests/run-all.mjs 2>&1 | grep -q "45/45" && echo "  suite wasm        45/45"
TINYCOLOR_TRANSPORT=native node tests/run-all.mjs 2>&1 | grep -q "45/45" && echo "  suite native      45/45"
node fuzz/harness.mjs --seconds 5 --seed 1 2>&1 | grep -q "divergences    0" && echo "  fuzz (5s probe)   0 divergences"
grep -q '"publishable": true' bench/results.json && echo "  bench results     publishable"
echo "  decisions         $(grep -c '^## D-0' DECISIONS.md) entries"

say "ready. before you hit record:"
cat <<'TXT'
  T1  this terminal, cleared, font >=18pt, ~100 cols, dark theme
  T2  python3 -m http.server 8099 --directory web
      then load http://localhost:8099 once so segment 5 has no stall
  Editor  DECISIONS.md open at D-012
  Browser tab  github.com/bgrins/TinyColor/issues/280 (for segment 8)

  Script: DEMO-SCRIPT.md
TXT

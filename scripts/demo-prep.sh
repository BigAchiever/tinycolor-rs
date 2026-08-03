#!/usr/bin/env bash
# Warms everything the demo video needs, so no shot waits on a compile, and
# dry-runs every on-camera command so nothing fails live.
#
#   make demo-prep
#
# Note on error handling: this script deliberately does NOT use `set -e`.
# An earlier version did, and piping into `grep` killed it silently the moment
# a cached build printed output the grep did not match — every check after that
# point was skipped and it looked like the script had merely stopped. Each step
# now reports its own pass/fail and the script always reaches the end.

set -uo pipefail
cd "$(dirname "$0")/.."

FAIL=0
say()  { printf "\n\033[1m%s\033[0m\n" "$*"; }
ok()   { printf "  \033[32mok\033[0m   %-30s %s\n" "$1" "${2:-}"; }
bad()  { printf "  \033[31mFAIL\033[0m %-30s %s\n" "$1" "${2:-}"; FAIL=1; }
skip() { printf "  \033[33m--\033[0m   %-30s %s\n" "$1" "${2:-}"; }

# ---------------------------------------------------------------------------
say "1/4  building both artifacts"

if cargo build --release -p tinycolor-cli >/tmp/dp-cli.log 2>&1; then
  ok "native binary" "target/release/tinycolor"
else
  bad "native binary" "see /tmp/dp-cli.log"
fi

if wasm-pack build crates/wasm --target nodejs \
     --out-dir ../../tests/original/pkg --out-name tinycolor_wasm \
     >/tmp/dp-wasm.log 2>&1; then
  ok "wasm module" "$(du -h tests/original/pkg/tinycolor_wasm_bg.wasm 2>/dev/null | cut -f1)"
else
  bad "wasm module" "see /tmp/dp-wasm.log"
fi

# ---------------------------------------------------------------------------
say "2/4  warming both transports"

if node tests/run-all.mjs >/dev/null 2>&1; then ok "wasm transport" "warm"; else bad "wasm transport"; fi
if TINYCOLOR_TRANSPORT=native node tests/run-all.mjs >/dev/null 2>&1; then ok "native transport" "warm"; else bad "native transport"; fi

if [ -f web/tinycolor.js ]; then
  ok "browser demo bundle" "$(du -h web/tinycolor.js | cut -f1)"
else
  bad "browser demo bundle" "UPSTREAM_DIR=<path> node scripts/build-demo.mjs"
fi

# ---------------------------------------------------------------------------
say "3/4  dry-running every on-camera command"

if ./scripts/verify-hashes.sh >/dev/null 2>&1; then ok "hashes" "4 originals unmodified"; else bad "hashes"; fi

if node tests/run-all.mjs 2>&1 | grep -q "45/45"; then ok "upstream suite, wasm" "45/45"; else bad "upstream suite, wasm"; fi

if TINYCOLOR_TRANSPORT=native node tests/run-all.mjs 2>&1 | grep -q "45/45"; then
  ok "upstream suite, native" "45/45"
else
  bad "upstream suite, native"
fi

if node fuzz/harness.mjs --seconds 5 --seed 1 2>&1 | grep -q "divergences    0"; then
  ok "fuzz (5s probe)" "0 divergences"
else
  bad "fuzz (5s probe)"
fi

if grep -q '"publishable": true' bench/results.json 2>/dev/null; then
  ok "benchmark results" "publishable"
else
  bad "benchmark results"
fi

ok "decision log" "$(grep -c '^## D-0' DECISIONS.md) entries"

if command -v docker >/dev/null 2>&1 && docker image inspect tinycolor-port-mortem >/dev/null 2>&1; then
  ok "docker image" "built"
else
  skip "docker image" "not built (optional for the video)"
fi

# ---------------------------------------------------------------------------
say "4/4  set up before you hit record"
cat <<'TXT'
  T1  this terminal — cleared, font >=18pt, ~100 cols, dark theme
  T2  python3 -m http.server 8099 --directory web
      then load http://localhost:8099 once so segment 5 has no stall
  Editor       DECISIONS.md open at D-012
  Browser tab  github.com/bgrins/TinyColor/issues/280   (segment 8)

  Script       DEMO-SCRIPT.md
  Record       Cmd+Shift+5 -> Record Selected Portion -> Options -> Microphone
TXT

if [ "$FAIL" -eq 0 ]; then
  printf "\n\033[32m  ready to record.\033[0m\n\n"
else
  printf "\n\033[31m  NOT ready — fix the FAIL rows above first.\033[0m\n\n"
  exit 1
fi

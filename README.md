# TinyColor → Rust

**Port Mortem 2026 · Track F (JavaScript → Rust)**
Source: [bgrins/TinyColor](https://github.com/bgrins/TinyColor), 1,187 lines of JavaScript colour maths, pinned at commit `b49018c9f2dbca313d80d7a4dad25e26143cfe01`.

| | |
|---|---|
| **Upstream test suite, byte-for-byte unmodified** | **45/45 (100%)**, through two independent transports |
| **Differential fuzz vs upstream on V8, exact float-bit equality** | **689,345 generated inputs × 45 operations = 31,020,525 comparisons, 0 divergences** (300 s, seed 1) |
| **`unsafe` blocks** | **0** — `unsafe_code = "forbid"` in all three crates, so it is a compile error, not a claim |

```bash
make verify      # the one command: build, hash-check the originals,
                 # Rust unit tests, upstream suite on BOTH transports
make docker      # the same proof in a container, no local toolchain
```

`make verify` builds both artifacts first, so it needs Rust 1.96+, Node 20+ and wasm-pack 0.15. `make docker` needs none of them — the image compiles everything itself and the build context excludes every host-built artifact (see `.dockerignore`).

**The port is slower than the original at per-operation colour maths — 4.8×–9.6×.** That is the honest headline, it is [in the table below](#performance-the-regression-and-the-win), and it has a precise cause: the JSON-RPC bridge (D-001) that makes the *unmodified* upstream suite able to drive the port. The genuine win is runtime elimination: the native binary starts in **3.2 ms vs 18.4 ms** and peaks at **4.4 MB vs 34.3 MB**.

---

## Evidence index

Every claim, and the exact command or file that settles it.

| Claim | Result | Check |
|---|---|---|
| Upstream suite, unmodified, via wasm | 45/45 (100.0%) | `node tests/run-all.mjs` |
| Same suite, via native binary over stdio JSON-RPC | 45/45 (100.0%) | `TINYCOLOR_TRANSPORT=native node tests/run-all.mjs` |
| Upstream's own runner (rethrows on first failure) | exit 0 | `node tests/original/test.js` |
| Original artifacts unmodified | 4/4 sha256 OK | `./scripts/verify-hashes.sh` |
| Differential fuzz, wasm | 689,345 cases · 31,020,525 comparisons · **0 divergences** | `node fuzz/harness.mjs --seconds 300 --seed 1` |
| Differential fuzz, native | 74,303 cases · 3,343,635 comparisons · **0 divergences** | `TINYCOLOR_TRANSPORT=native node fuzz/harness.mjs --seconds 120 --seed 11` |
| Zero `unsafe` | compiler-enforced | `unsafe_code = "forbid"` in `crates/*/Cargo.toml` |
| Rust unit tests | 19/19 | `cargo test --locked -p tinycolor-core` |
| Behavioural checksums agree | all 9 rows, at equal *n* | `bench/results.json` → `equivalence_all_agree: true` |
| Per-op throughput | **4.8×–9.6× slower** than upstream JS | `node bench/run.mjs --reps 5` |
| Startup / RSS | 3.2 ms / 4.4 MB native; wasm loses on both | `bench/results.json` |
| Documented divergences | 22 (`D-001`…`D-022`) | `DECISIONS.md` |

`tests/original/test.js` contains 432 assertion call sites (79 `assert`, 350 `assertEquals`, 3 `assertThrows`); several run inside loops, so roughly 432 assertion call sites execute per run.

| **Live demo** | [bigachiever.github.io/tinycolor-rs](https://bigachiever.github.io/tinycolor-rs/) — upstream's own demo page, unmodified, every swatch computed in Rust | `web/` |
| Upstream bug found and filed | [bgrins/TinyColor#280](https://github.com/bgrins/TinyColor/issues/280) — unbounded loop in `analogous()`/`monochromatic()` | `UPSTREAM-BUG-REPORT.md`, D-022 |

Fuzz comparison is **bit-level exact**, not epsilon-tolerant: every returned number is canonicalised to its raw IEEE-754 bit pattern (`fuzz/harness.mjs`, `canon()`), so a 1-ULP difference cannot hide, and `NaN`, `±Infinity` and `-0` are each encoded distinctly rather than being flattened together by JSON (D-016).

---

## What is upstream's, and what is ours

Judges get burned by ports that quietly edit the tests. Three categories, kept separate.

**Byte-identical to upstream and hash-pinned.** `./scripts/verify-hashes.sh` exits non-zero if any of these changed:

| File | Role |
|---|---|
| `tests/original/test.js` | the upstream test suite — 0 bytes changed |
| `tests/original/package.json` | upstream's manifest for that suite |
| `tests/deno_asserts@0.168.0.mjs` | the assertion library the suite imports |
| `upstream/tinycolor.reference.js` | the original library — **fuzz oracle only**, never linked by the port |

The oracle is in the manifest too, so the reference cannot have been quietly bent toward the port either.

**Copied verbatim but not hash-pinned.** `web/index.html` and `web/demo/demo.css` are copied from an upstream checkout by `scripts/build-demo.mjs`. They are *not* in `tests/HASHES.txt`; diff them against upstream yourself rather than take this file's word for it.

**Ours — do not mistake these for the original:**

- `tests/original/tinycolor.js` — a 110-line **shim**, neither upstream nor the port. `test.js` line 5 is `require("./tinycolor.js")`, so placing the shim at exactly that path substitutes the library under test without editing the suite (D-017). `TINYCOLOR_TRANSPORT` selects `wasm` (default) or `native`.
- `tests/run-all.mjs` — a pass-*rate* runner. Upstream's CJS runner rethrows on the first failing assertion, which is fine for CI and useless for reporting a rate. This pre-loads the same `@deno/shim-deno-test` module the tests import and wraps each registered function in try/catch — **without touching the test file**.
- `tests/original/pkg/` — `wasm-pack --target nodejs` output, a build artifact (gitignored; `make verify` produces it).
- `shim/api.js`, `crates/`, `fuzz/`, `bench/`, `scripts/`, `web/tinycolor.js`.

---

## Performance: the regression, and the win

Apple M4 (fanless MacBook Air), macOS 26.5.2 / arm64, node v20.19.5 / V8 11.3.244.8, rustc 1.96.0. 512-entry corpus, 200k-op warmup per config, median of 5 reps. **Run-to-run spread reaches 39%, so read these as two significant figures.** RPC cost is included, not subtracted.

| Row | upstream JS | port (wasm) | |
|---|---:|---:|---|
| parse-only | 464 ns | 4,475 ns | **9.6× slower** |
| to-rgb-string | 570 ns | 4,973 ns | **8.7× slower** |
| to-hex-string *(headline)* | 645 ns | 4,991 ns | **7.7× slower** |
| to-hsl-string | 645 ns | 4,860 ns | **7.5× slower** |
| to-name | 645 ns | 4,807 ns | **7.5× slower** |
| luminance | 625 ns | 4,203 ns | **6.7× slower** |
| round-trip | 1,695 ns | 9,423 ns | **5.6× slower** |
| lighten | 1,664 ns | 8,535 ns | **5.1× slower** |
| mix | 1,465 ns | 7,102 ns | **4.8× slower** |

**Read the shape, not the magnitudes.** Upstream separates its cheapest and dearest row by ~1.2 µs; the port's rows all sit between 4.2 and 9.4 µs regardless of how much colour maths each does. That flat ~4 µs floor is the answer: the cost is the JSON-RPC round trip, not the arithmetic. `luminance` costs 4.2 µs through the bridge; the Rust that does the work is 4.0 ns of it.

### Where the 8× actually goes

The natural reading of the table above is "the Rust is slow". It isn't. Peeling
the stack apart one layer at a time, same operation, medians of 5 reps:

| layer | ns/op | added by this layer |
|---|---|---|
| **A** upstream JS on V8 | 1,033 | — |
| **D** the ported Rust, native, no bridge | 1,325 | +292 |
| **C** + wasm boundary + `serde_json` | 4,456 | +3,131 |
| **B** + JS shim (`JSON.stringify`/`parse`, handle table, `FinalizationRegistry`) | 8,374 | +3,918 |

**The ported colour logic runs at 1.28× upstream. The shipped artifact runs at
8.11×. Of the 7,341 ns gap, 96% is bridge** — split roughly evenly between the
wasm/serde boundary and the JS shim, and constant per call regardless of how
much work the operation does.

Reproduce it:

```bash
node bench/js-bench.cjs --config upstream     --row to-hex-string --n 100000  # A
./target/release/tinycolor bench parse-hex 1000000                            # D
node bench/js-bench.cjs --config wasm-raw     --row rpc-new      --n 50000    # C
node bench/js-bench.cjs --config wasm-shipped --row to-hex-string --n 50000   # B
```

Config **C** is the useful one: it calls the wasm `dispatch()` export directly
with a literal request string, so no JSON work happens in JavaScript. The gap
between C and B is therefore the shim alone; the gap between D and C is the
wasm boundary plus `serde_json` parsing inside Rust.

That is the bill for **D-001**, and it is the same decision that bought the strongest correctness evidence available — the upstream suite running with zero bytes changed. Both facts come from one trade, made with open eyes.

### Startup and memory

p50 of 20 spawns, and max RSS:

| Configuration | startup p50 | max RSS |
|---|---:|---:|
| **native binary** | **3.2 ms** | **4.4 MB** |
| node + upstream JS | 18.4 ms | 34.3 MB |
| node + port (native IPC) | 22.7 ms | 40.2 MB |
| node + port (wasm) | 71.1 ms | 86.3 MB |

**The shipped wasm artifact loses on every axis, including the two where "it's Rust" is supposed to win** — instantiating a 1.1 MB wasm module costs more than booting V8. The runtime-elimination win is real and lives only in the native binary: **5.8× faster to start, 7.8× lighter**, no V8 at all. It carries the same correctness evidence as the wasm build — the same 45/45 on the same unmodified suite, plus its own clean fuzz run — which is what makes it a claim rather than a demo (D-020).

### The native throughput column is not comparable to the JS column

`target/release/tinycolor bench <workload> <n>` reports, in-process: parse-hex 694 ns, to-hex 199 ns, to-rgb-string 159 ns, to-hsl-string 315 ns, lighten 952 ns, mix 402 ns, luminance 4.0 ns, round-trip 2,188 ns, noop 0.75 ns.

**These must not be placed beside the JS column.** The CLI pre-parses its corpus *outside* the timed region while the JS harness parses *inside* it, and the Rust timed region includes deallocation while the JS baseline defers it to a GC running outside the window. Putting them side by side would produce a flattering table and a false claim, so they are reported separately and no ratio is computed from them. `bench/METHODOLOGY.md` records this and the other confounds that could not be eliminated.

### Benchmark honesty controls

`bench/METHODOLOGY.md` was written **before any number was measured** and says so in its header, with the then-failing preconditions recorded verbatim. `bench/run.mjs` refuses to publish if the machine is on battery below 80%, in Low Power Mode, low on disk,; the gate results ship next to the numbers in `bench/results.json` (`publishable: true`).

There is **no per-op p99** for the sub-microsecond rows. `Instant::now()` on Apple Silicon costs ~20–25 ns against a ~40 ns mach timebase quantum, so timing a 150 ns operation per-iteration would inflate each side differently and charge the larger penalty to JS. Those rows are 1,000-op batch means; the percentiles in `bench/results.json` are batch-level and labelled `"mode": "batch"`. Genuine per-op percentiles are reported only for startup, which is millisecond-scale.

---

## Behavioural equivalence

`fuzz/harness.mjs` runs the pinned reference and the port in the same process on identical generated inputs across 21 generators — hex3/4/6/8, names, name casing, `transparent`, `rgb`/`rgba`/`hsl`/`hsla`/`hsv`/`hsva` strings, five object shapes, whitespace abuse, malformed strings, non-string types. Per-generator counts are in every log.

| Run | Transport | Budget | Cases | Comparisons | Divergences | Log |
|---|---|---|---:|---:|---:|---|
| seed 1 | wasm | 300 s | 689,345 | 31,020,525 | **0** | `fuzz/logs/run-wasm-300s-seed1.json` |
| seed 7 | wasm | 120 s | 129,250 | 5,557,750 | **0** | `fuzz/logs/post-perffix-seed7.json` |
| seed 11 | native | 120 s | 74,303 | 3,343,635 | **0** | `fuzz/logs/run-native-120s-seed11.json` |

**34,364,160 comparisons, zero divergences.** That is 763,648 generated inputs put through a 45-operation battery each, not 34 million distinct inputs. The transport is chosen by `TINYCOLOR_TRANSPORT` on the command line; each log records the transport it actually ran under, in its `transport` field.

Each log also carries `peak_live_handles` (13,200 — one batch). That field exists because the port once leaked handles until the wasm heap died at ~4.6M colours (D-018); the regression is now a number rather than a mysterious crash forty minutes in.

Nine equivalence checksums, independent of timing, agree between upstream and port at equal *n* (`bench/results.json`), so no row is fast because it is doing less work. `luminance` agrees on a full 32-bit fold over 20,000 colours — `2499936779` — which is the evidence that the D-012 lookup table is bit-identical to V8's `Math.pow`.

---

## Architecture

**D-001: three crates behind one JSON-RPC bridge, not ~45 FFI signatures.**

```
crates/core   2,823 lines — jsnum, names, luminance, convert, parse, color, rpc.
              All colour logic, no I/O, no transport dependency.
crates/wasm      14 lines — wasm-bindgen transport
crates/cli      420 lines — binary `tinycolor`: rpc | bench <workload> <n> | version
```

A per-method wasm-bindgen surface would have put ~45 signatures across the FFI boundary, each a place for a marshalling bug that is indistinguishable from a porting bug when it fires. A napi-rs addon would have coupled the port to Node's ABI and made "this runs standalone" much harder to argue. Instead the boundary is one function taking and returning a JSON string, with colours living in a Rust-side handle table so JavaScript object identity (`tinycolor(x) === x`) and the mutating chain methods behave as upstream's do.

Three consequences that were not predicted:

- **It is why D-012 was findable.** The wasm and native builds disagreed on floating point, and a disagreement between two builds of one source only surfaces if both run the same cases through the same protocol.
- **It is why the suite runs unmodified** — the shim sits at the `require()` path (D-017).
- **It cost the entire performance story**, and every defect the port introduced (D-016, D-018, D-021) lives in it, not in the colour maths.

`crates/core/src/jsnum.rs` reproduces JavaScript's numeric semantics rather than approximating them: `Math.round` breaking ties toward +∞ (D-003), `parseFloat` as a prefix parse (D-005), `Number→String` exponential thresholds (D-006), `parseInt`-on-number reading the mantissa (D-011). Three dependencies total — `regex`, `once_cell`, `serde_json`. No JS runtime is linked.

**Upstream's own demo page runs on the port.** `web/index.html` is upstream's demo, unmodified. Because it loads the library with a classic `<script>` tag and calls `tinycolor(...)` synchronously, the wasm is inlined as base64 and instantiated with `initSync`, so the page needed no changes at all:

```bash
make demo        # http://localhost:8099/
```

---

## Decisions worth a judge's five minutes

`DECISIONS.md` holds 21 entries (`D-001`…`D-021`). Each records what upstream does, what a direct translation would have produced, what the port does instead, and how it is verified — tagged **unit**, **suite**, **fuzz**, or **exhaustive**. Fifteen carry a **fuzz** tag, meaning differential fuzzing found or confirmed them; D-013, D-016 and D-018 were found by it outright. The entries found by reading are the ones we are least confident are complete, and that asymmetry is the honest summary of what differential fuzzing is for.

**D-012 — `Math.pow` is not the same function on two machines.** Upstream's readability test asserts exact float equality, and the port failed it by one ULP — differently depending on the build target. Rust `std::f64::powf` matched V8 on 207/256 channel values; wasm32's MUSL-derived `pow` on 225/256. `pow` is not correctly-rounded in any mainstream libm and nobody promises it is. But `getLuminance()` takes its input from `toRgb()`, which rounds, so only the integers 0–255 can reach that call. The domain is finite, so it is tabulated: `crates/core/src/luminance.rs` holds V8's exact f64 bit patterns for all 256 values, generated by `scripts/gen-luminance-table.mjs`. `srgb_linear` returns `None` outside the domain, so the finiteness argument is enforced rather than assumed. Bit-exact on every target — and the entry that reframed the rest of the port: *equivalent has to mean equivalent on the machine the judge runs it on.*

**D-002 — `"1.0"` and `1.0` are different colours.** `{r: "1.0"}` gives `#ff0000`; `{r: 1.0}` gives `#010000`. A factor of 255, decided by `typeof`: upstream's `isOnePointZero` returns true only for a *string*. Parsing every component to `f64` at the boundary destroys this on line one and still passes most of the suite — the dangerous kind of wrong. The port carries a `convert::Unit` enum of `Num(f64) | Str(String)` down to `bound01`, making the type itself a runtime value.

**D-013 — upstream is inconsistent about alpha, and the port has to be inconsistent the same way.** The first real fuzz run reported ~34,000 divergences, nearly all alpha vanishing from modification methods. `lighten`/`darken`/`saturate`/`desaturate`/`greyscale`/`spin`/`complement`/`analogous` rebuild from the mutated `toHsl()` object, which carries `a`; `polyad`/`splitcomplement`/`monochromatic` build from a fresh `{h, s, l}` literal, which drops it; `brighten` threads `toRgb()` and survives by a third route. There is no design to recover — it is an artefact of which line got written when. Two constructors, applied per function to match. Tidying it would have been the natural instinct and would have made the port wrong.

**D-016 and D-018 — the defects the port introduced are in the bridge, not the colour maths.** `JSON.stringify(Infinity)` is `"null"` and `-0` stringifies to `"0"`, so `lighten(Infinity)` arrived as `lighten(undefined)` and defaulted to 10 — presenting identically to a porting bug and not being one. Separately, the handle table leaked: a 300-second run went clean for 4.2M comparisons then emitted 7.3M divergences, which is the signature of state corruption, with `memory access out of bounds` underneath. JS garbage collection knows nothing about a Rust-side table, so every colour ever constructed stayed live; at ~4.6M handles linear memory was exhausted. Fixed with a `FinalizationRegistry` in `shim/api.js` plus an explicit `__reset` between fuzz batches. **The part worth stealing:** short runs reported zero divergences and were not lying — they were too short to reach the failure. A 60-second run, this event's minimum, would never have found it.

**D-011 — a real defect, proven unobservable, deliberately not reported.** `bound01` does `parseInt(n * max, 10)`, and the argument is a *number*, so it is stringified first: `parseInt(2.55e-7, 10)` is `2`, not `0`. `bound01("0.000000001%", 255)` returns `0.0000784` where the mathematically correct value is `~1e-11`. Then we tried to make it observable through the public API and could not — the constructor applies `if (this._r < 1) this._r = Math.round(this._r)`, and `Number#toString` normalises exponential form to one leading digit, so `parseInt` returns at most `9` and the defect can produce a channel of at most `0.09`, which rounds to `0` — the correct answer, for every reachable input. The port reproduces it exactly (`jsnum::parse_int_from_number`) rather than "fixing" it: a port that silently corrects its source is no longer equivalent, and `trunc()` — the natural Rust translation — would have been wrong here.

**D-021 — the native transport hung every process that used it.** The spawned child and its two pipes are live libuv handles, and an active handle keeps Node's event loop alive; a consumer that returned from `main` never exited. Neither harness could see it, because `tests/run-all.mjs` and `fuzz/harness.mjs` both call `process.exit()` — so 45/45 and 3,343,635 fuzz comparisons ran green over a transport that hung. Fixed with `child.unref()`. It is the third defect in a row located in the bridge rather than in ported logic.

---

## Code quality

**Zero `unsafe`, machine-checked.** `unsafe_code = "forbid"` is present in all three crate manifests — core, wasm and cli. An `unsafe` block anywhere, including at the transport boundaries, is a compile error, so the claim cannot drift from the code.

**Layering.** `crates/core/src/lib.rs` documents the stack bottom-up: `jsnum` → `names`/`luminance` → `convert` → `parse` → `color` → `rpc`. Colour logic never touches I/O; the transports contain no colour logic (`crates/wasm/src/lib.rs` is 14 lines).

**Idiomatic where Rust is better, faithful where JavaScript is weird.**

- `names::HEX_NAME_MAP` is a `once_cell` `HashMap` built from a generated 149-entry slice **in source order**, so later entries overwrite earlier ones — JavaScript's last-wins object flip is a property the map preserves, not a side effect of a scan direction (D-004, D-019).
- `jsnum::math_round` uses floor/fract, *not* the `floor(x + 0.5)` shorthand, which is wrong just below a half (D-003).
- `color::to_int32` exists because `>>` in `analogous` is an int32 coercion, not a halving: `(30.5 * 6) >> 1` is `91`, not `91.5` (D-010).

**19 Rust unit tests**, none of them duplicates of the suite. Each pins a documented divergence — `math_round_matches_js_on_ties`, `parse_float_takes_prefix`, `hex_names_flip_is_last_wins`, `parse_int_from_number_reads_mantissa_only`, `maps_agree_with_linear_scan` — so a regression names its own decision entry.

**Build reproducibility.** `Cargo.lock` committed and `--locked` used everywhere; release profile pinned in the workspace root (`opt-level=3, lto=true, codegen-units=1, panic="abort"`); upstream commit pinned in `upstream/PINNED_COMMIT`; the luminance table regenerable with `node scripts/gen-luminance-table.mjs`, whose output should diff clean against the committed file.

---

## What this port does not do

| Penalised pattern | Status |
|---|---|
| **Shelling out to the original** | No. `grep -rn "Command::" crates/*/src` returns nothing — the port spawns no process. The only `std::process` use in the workspace is `exit()` in the CLI's own `main`. The three dependencies are `regex`, `once_cell`, `serde_json`. The native binary runs with no Node present. |
| **FFI into the source-language runtime** | No. No napi-rs, no V8 embedding, no JS engine linked. The wasm transport is wasm-bindgen; the native transport is a stdio pipe. `crates/core` has no transport dependency at all. |
| **Silently edited tests** | No. `test.js`, its `package.json`, and the assertion module are hash-verified byte-identical. `./scripts/verify-hashes.sh` exits non-zero if any changed. |
| **Cherry-picked happy paths** | No. 45/45 of the suite, not a chosen subset, plus 21 fuzz generators explicitly including malformed strings, whitespace abuse and non-string types. The bench corpus hash is recorded in `bench/results.json`. |
| **Hello-world translation** | No. 2,823 lines of Rust in core against 1,187 of JavaScript; the complete upstream API behind one dispatch function; 149 CSS names; all combination functions. |
| **LLM dump without decision documentation** | 21 decision entries with alternatives considered and rejection reasons; a benchmark methodology written before measurement; three self-inflicted bugs documented as such (D-016, D-018, D-021). |

**One clarification, because it looks like the thing that is disallowed.** `upstream/tinycolor.reference.js` is the pinned original JavaScript. It exists **only** as the differential-fuzz oracle and as the benchmark's `--config upstream` baseline. It is not linked, imported or executed by the port, by either transport, or by the test path — `tests/original/tinycolor.js` never references it. Confirm with `grep -rn "reference.js" crates shim tests web`, which returns exactly one line, in the hash manifest; or delete the file and re-run `node tests/run-all.mjs`, which still reports 45/45.

### The upstream bug, and the two that were not filed

**Filed: [bgrins/TinyColor#280](https://github.com/bgrins/TinyColor/issues/280)** —
`analogous()` and `monochromatic()` loop unboundedly on negative or fractional
counts and exhaust the process heap. Both decrement a counter and test it for
truthiness, so a counter that never lands exactly on `0` never terminates, and
each pass allocates a colour. Six cases confirmed (two functions × `-1`, `1.5`,
`0.5`), all exit 134. Verified against current `main` before filing, not just
the pinned commit.

Upstream already guards the same input shape in `polyad()`
(`if (isNaN(number) || number <= 0) throw`), so the hazard is recognised in one
of the three combination functions and not the other two. Full report in
`UPSTREAM-BUG-REPORT.md`; the port's deliberate divergence is **D-022**.

Two further candidates were investigated to conclusion and **deliberately not
filed**, because neither is a defect:

- **D-011** — the `parseInt`-on-number quirk in `bound01`: real, and **proven
  unobservable** through the public API. The constructor's `< 1` rounding
  absorbs it, and `Number#toString` normalisation caps the error below the
  rounding threshold, so it cannot be pushed into view.
- **D-007** — `[\s|\(]+` in upstream's matcher means a literal `|` is an
  accepted CSS separator, so `tinycolor("rgb|255|0|0")` parses to `#ff0000` and
  reports `isValid() === true`. Almost certainly an escaping slip, but
  TinyColor's stated design goal is that input be as permissive as possible, so
  reporting it would be filing a bug against a documented intention. The port
  reproduces it and the fuzzer emits pipe separators on purpose.

One filed, two withheld. The bar was "is this a defect a maintainer should act
on", not "is this worth three points".

---

## Repo map

```
crates/core/       the port — jsnum, names, luminance, convert, parse, color, rpc
crates/wasm/       wasm-bindgen transport
crates/cli/        native binary `tinycolor`: rpc | bench <workload> <n> | version
shim/api.js        shared API layer — used by the node shim AND the browser demo
tests/original/    test.js + package.json BYTE-IDENTICAL to upstream npm/cjs/
                   tinycolor.js is OUR shim; pkg/ is wasm-pack build output
tests/run-all.mjs  pass-rate runner (upstream's own runner rethrows on first failure)
tests/HASHES.txt   sha256 manifest of the four unmodified artifacts
fuzz/              harness.mjs, generators.mjs, logs/ (raw run output, kept)
bench/             METHODOLOGY.md (pre-registered), run.mjs, js-bench.cjs,
                   gen-corpus.mjs, corpus.json, results.json, CLI-NOTES.md
upstream/          tinycolor.reference.js (fuzz oracle), LICENSE, PINNED_COMMIT
web/               index.html (upstream's demo page) + tinycolor.js (wasm inlined)
scripts/           hash-originals.sh, verify-hashes.sh, gen-luminance-table.mjs,
                   build-demo.mjs
DECISIONS.md       D-001 … D-021
Makefile           make verify | fuzz | fuzz-long | bench | demo | docker | help
Dockerfile         hermetic verification image; builds everything from source
```

### Building from source

```bash
make build                                     # both artifacts
cargo build --locked --release -p tinycolor-cli # native binary only
wasm-pack build crates/wasm --target nodejs \
  --out-dir ../../tests/original/pkg --out-name tinycolor_wasm
UPSTREAM_DIR=/path/to/TinyColor node scripts/build-demo.mjs   # regenerates web/tinycolor.js
node scripts/gen-luminance-table.mjs           # regenerates the D-012 table from V8
```

Overridable make variables: `SECONDS`, `LONG_SECONDS`, `SEED`, `TRANSPORT`, `REPS`, `PORT` — e.g. `make fuzz SECONDS=120 SEED=7 TRANSPORT=native`.

---

## What this evidence does not cover

- **One machine, one session.** Everything above is macOS 26.5.2 / arm64 on a fanless M4, no core pinning, other processes running. The between-rep spread is measured *within* one session, which understates true variance. D-012 already proved this port is target-sensitive; no performance number generalises to Linux/x86_64 without being re-measured there.
- **Fuzz coverage is generator-shaped.** Zero divergences over 34.4M comparisons is strong evidence about the space those 21 generators reach and says nothing about the space they do not.
- **The suite passing 45/45 is a statement about upstream's coverage**, not proof of total equivalence. The fuzz runs are the wider claim, and they are bounded by the same generators.
- **`polyad` is excluded from fuzzing**, deliberately: upstream comments the prototype method out pending [bgrins/TinyColor#254](https://github.com/bgrins/TinyColor/issues/254), so the reference throws for every input and fuzzing it produced ~11,000 meaningless divergences (D-015). The port implements it anyway, because the disabled test expects it.
- **No fuzz run is a proof.** All three are reproducible from their seeds; none is exhaustive.

## Live demo

<https://bigachiever.github.io/tinycolor-rs/>

This is upstream's own `index.html` from the pinned commit, byte-identical, with
one substitution: the `tinycolor.js` it loads is the Rust port compiled to
WebAssembly and inlined as base64, so a plain `<script src>` works with no async
init and the page needed no edit at all.

Published from the `gh-pages` branch, built by `scripts/build-demo.mjs`. Rebuild
it with:

```bash
wasm-pack build crates/wasm --target no-modules \
  --out-dir ../../web/pkg --out-name tinycolor_wasm
node scripts/build-demo.mjs
```

Worth checking in the console: `tinycolor("#111").getLuminance()` returns
`0.005605391624202723` — V8's exact bit pattern, from the D-012 lookup table,
served from WebAssembly.

---

## Provenance and licence

Upstream TinyColor is © Brian Grinstead, MIT — `upstream/LICENSE`, preserved verbatim. This port is MIT, same terms. Pinned commit `b49018c9f2dbca313d80d7a4dad25e26143cfe01` (`upstream/PINNED_COMMIT`). `tests/original/test.js`, `tests/original/package.json`, `tests/deno_asserts@0.168.0.mjs` and `upstream/tinycolor.reference.js` are upstream's work, unmodified and hash-pinned. `web/index.html` is upstream's demo page, unmodified. Everything else is the port.

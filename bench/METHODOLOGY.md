<!--
NOTE: figures inline below marked (probe) are planning-time estimates taken
under throttled conditions and are NOT results. The measured results are in
bench/results.json. Where this document cited fuzz counts as fact, they have
been refreshed to the current archived runs.

Benchmark methodology, written BEFORE any number was measured, so the plan
cannot be retrofitted to flatter the results.

STATUS: NOT YET EXECUTED. Gate P1 (AC power, Low Power Mode off) is failing —
the machine was on battery at 10% with LPM=1 when this was written. Any figure
measured under those conditions is CPU-throttled and unpublishable.

Probe numbers appearing inline below were taken under exactly those bad
conditions during planning. They indicate direction and magnitude only and
MUST NOT be published.
-->

# TinyColor Port — Benchmark Plan (executable, final)

Machine of record: **Apple M4, MacBook Air (fanless), macOS 26.5.2, arm64**, node v20.19.5 / V8 11.3.244.8-node.30, rustc 1.96.0, workspace release profile `opt-level=3, lto=true, codegen-units=1, panic="abort"`.

All numbers quoted below as *(probe)* were measured by me during planning **on battery at 12% with Low Power Mode ON**. They are indicative of direction and magnitude only and **must not be published**. They exist here so the executor knows what to expect and can detect a broken harness.

---

## 0. Answers to the two decision questions, up front

### 0.1 Headline: shipped **wasm+RPC vs upstream JS in Node**. RPC overhead **included**.

The headline row is **`tinycolor(x).toHexString()` through `/Users/danishalisiddiqui/Dev-Setups/1aug/tests/original/tinycolor.js`** (shim → wasm → `rpc::dispatch`) against `/Users/danishalisiddiqui/Dev-Setups/1aug/upstream/tinycolor.reference.js`, both in Node, RPC cost **included, not subtracted**.

Three reasons, in priority order:

1. **It is the only artifact with correctness evidence.** `tests/original/test.js` loads `./pkg/tinycolor_wasm.js`; the archived run logs record which transport produced them. The 45/45 and the fuzz comparisons are statements about a specific binary. Quoting native throughput under that banner reproduces exactly the native/wasm split D-012 already documented.
2. **It is what a consumer gets.** No consumer calls `TinyColor::from_str` today. `web/tinycolor.js` and the test shim both go through the bridge.
3. **The honest number is a loss, and the FAQ requires disclosing it.** *(probe: shipped path 7,555 ns/op vs upstream 783 ns/op — the port is ~9.7× SLOWER on the headline operation.)* A plan that leads with native here would be selecting the flattering measurement; that is the specific thing judges have been burned by.

**Native is a second column, always labelled**, and gated on §3.6. It is not suppressed — but note that native also loses on the common paths *(probe: native `parse-hex` 1,219 ns/op vs upstream JS 774 ns/op; native `to-hex` 391 ns vs JS 152 ns)*, so there is no flattering-column temptation to manage. The port's genuine wins are **process startup** and **`getLuminance`/`readability`**, and both must be reported with their causes named (V8 boot elimination; a lookup table, not Rust).

RPC overhead is additionally **decomposed** into its own line item (§4, configs B/C/D/E) and attributed to **D-001** by name. Presenting the bridge as a deliberate, measured architectural trade-off is stronger than hiding it and stronger than letting a reader mistake it for slow colour code.

### 0.2 The one-sentence headline the report should carry

> Eliminating the Node runtime saves **~31 ms of V8/libuv/ICU bootstrap per process invocation**; per-operation throughput of the shipped wasm+JSON-RPC artifact is a **regression** against upstream JavaScript, by roughly an order of magnitude on the headline operation, and the native core is also slower than V8 on parsing and string formatting. See §11 for the full disclosure list.

---

## 1. Preconditions (hard gates — the driver refuses to run if any fail)

| # | Gate | Check |
|---|---|---|
| P1 | **AC power, Low Power Mode OFF** | `pmset -g ps` must report `AC Power`; `pmset -g \| grep lowpowermode` must report `0`. **Currently battery 12% + LPM=1 — this is a blocker.** |
| P2 | Binary freshness | `mtime(/Users/danishalisiddiqui/Dev-Setups/1aug/target/release/tinycolor)` ≥ newest mtime under `/Users/danishalisiddiqui/Dev-Setups/1aug/crates/` |
| P3 | Disk | ≥ 4 GB free (`df -h /`; currently 6.5 GB). Do **not** add criterion — its default features pull plotters + rayon and can add hundreds of MB. Hand-rolled `std::time` only. |
| P4 | No rebuild mid-run | Build once, before any timing. Rebuilding relinks the binary and triggers a one-time macOS `syspolicyd`/XProtect scan (100–500 ms) charged entirely to the Rust side. |
| P5 | Foreground interactive shell, no other heavy processes, screen awake (`caffeinate -dims` wrapping the driver). |
| P6 | Native validation gate passed (§3.6) before *any* native number is published. |

---

## 2. Configurations (the columns of every table)

| ID | Name | What it is | Driver |
|---|---|---|---|
| **A** | `js-upstream` | `require('/Users/.../upstream/tinycolor.reference.js')` | `bench/js-bench.cjs --config upstream` |
| **B** | `wasm-shipped` ← **headline** | `require('/Users/.../tests/original/tinycolor.js')` (shim + wasm + RPC, incl. `FinalizationRegistry`) | `bench/js-bench.cjs --config wasm-shipped` |
| **C** | `wasm-raw-dispatch` | `require('/Users/.../tests/original/pkg/tinycolor_wasm.js').dispatch(literalJsonString)` — no shim, no `JSON.parse`/`stringify` in JS | `bench/js-bench.cjs --config wasm-raw` |
| **D** | `native-core` | `target/release/tinycolor bench <row> <n>` — direct `TinyColor` calls | native CLI |
| **E** | `native-rpc` | `target/release/tinycolor bench rpc-<row> <n>` — `rpc::dispatch(&str)` in-process, no wasm, no pipe | native CLI |

The decomposition this buys:

- **B − C** = shim JS cost (two `JSON.stringify`/`JSON.parse` pairs, `encode`, `FinalizationRegistry.register`, handle bookkeeping)
- **C − E** = wasm32 codegen + wasm-bindgen string marshalling tax
- **E − D** = `serde_json` + slab insert + `TinyColor::clone` (rpc.rs:37) — the D-001 bridge tax on the Rust side
- **D vs A** = the ported colour logic itself vs V8

**Rejected: R1's proposed bench-only `#[wasm_bindgen] pub fn bench_hex_string`.** It adds an export to the shipped crate that is not in the artifact, and the layer it isolates (wasm codegen without serde) is not a configuration anyone ships. Config **C** obtains the same decomposition using only the existing `dispatch` export.

---

## 3. Code changes required before measuring (minimal, and none to `crates/core`)

### 3.1 `crates/cli/src/main.rs` — additions

Add to `WORKLOADS` and `dispatch_workload`:

- `timer-floor` — times `iterations` back-to-back `Instant::now()` pairs, prints the per-pair cost. Publish next to `noop`; **never silently subtract it.**
- `to-name-hit` — `parsed_corpus()` filtered to colours that have names; `to-name-miss` — `["#1a2b3c","#c0ffee","#123457","#8a2be3"]` parsed, then `.to_name()`.
- `parse-invalid` — `["not a color","#gggggg","rgb(","","hsl(999","chartrooze"]`. **Exempt these from the `every_corpus_entry_parses` test** (add a separate assertion that they do *not* parse — the point is to time the failure path deliberately).
- `parse-object` — `TinyColor::new(Input::Obj(...))` over `{r,g,b}`, `{r,g,b,a}`, `{h,s,l}`, `{h,s,v}` literals, skipping `string_input_to_object` entirely.
- `parse-name-early` (`aliceblue, antiquewhite, aqua, aquamarine`) and `parse-name-late` (`yellowgreen, whitesmoke, yellow, wheat`) — `parse.rs:111` scans `NAMES` front-to-back, so alphabetical position is a real cost axis.
- `readability` — `ops::readability(a,b)` over corpus pairs.
- `rpc-new-hex` — `timed(HEX_REQ_JSON, iters, |s| rpc::dispatch(s))` where `HEX_REQ_JSON` holds the literal strings the shim emits, e.g. `{"op":"new","input":"#ff0000","opts":{}}`.
- `rpc-round-trip` — the exact two-request sequence `shim/api.js` issues for `tinycolor(x).toHexString()`: `{"op":"new",…}` then `{"op":"call","id":N,"method":"toHexString","args":[]}`, **plus** `{"op":"free","id":N}`, counted in the measured cost. Without the free this becomes a `HashMap`-growth benchmark with rehash spikes in the tail (and at 1e6 it is hundreds of MB — each `TinyColor` owns `format: Option<String>` and `original_input: Input`).
- `scenario-palette`, `scenario-harmony` — see §4.

**Remove `most-readable` from the native column.** `crates/cli/src/main.rs:265` is a *second implementation* of the algorithm that lives in `rpc.rs::statics("mostReadable")`; its own doc comment concedes this. Benchmarking a reimplementation is not benchmarking the port, and it additionally pre-parses `READABILITY_CANDIDATES` outside the timed region while the shipped path re-materialises them from JSON per call. `most-readable` is measured **only** on configs A/B/C/E, where the tested code runs. Native gets `readability` (a real `core` function) instead.

### 3.2 Checksums (both sides)

`timed()` gains a `u64` checksum accumulator: XOR-fold the bytes of `String` results, bit-cast-and-XOR `f64` results, XOR the `r/g/b/a` bit patterns of `TinyColor` results. Print as `"checksum"` in the JSON line. `bench/js-bench.cjs` computes the identical fold. **Checksums for a given row must match across configs A/B/C/D/E** — this is simultaneously the anti-dead-code-elimination guard and a cheap proof both harnesses did the same work.

### 3.3 Linearity guard

Rep 1 runs every row at both `N` and `2N`. If `nanos_per_op(2N) < 0.85 × nanos_per_op(N)`, **abort the whole run with a non-zero exit and a loud message**: the loop was folded. `black_box` is already on both input and result (main.rs:191) and inputs come from non-`const`-visible corpus slices, but the guard is cheap and it is the single most likely route to an absurd published number.

### 3.4 `bench/js-bench.cjs` (new, ~180 lines, CommonJS)

- `--config {upstream|wasm-shipped|wasm-raw}`, `--row <name>`, `--n <iters>`, `--warmup <iters>`, `--mode {batch|per-op}`, `--emit-curve`
- Corpus loaded at runtime from `bench/corpus.json` (§4); **no string literals in the timed loop.**
- Clock: `process.hrtime.bigint()`.
- `--mode batch`: one clock pair per 1,000 corpus operations, collect batch means → report distribution of **batch means**, no percentile claim.
- `--mode per-op`: one clock pair per operation into a preallocated `Float64Array` → real p50/p90/p99/p99.9/max.
- Emits exactly one JSON line on stdout.
- Prints `process.execArgv`, `process.version`, `process.versions.v8` into that line so the absence of V8 flags is verifiable, not merely asserted.

### 3.5 `bench/js-lut-luminance.cjs` (new, ~30 lines)

Emit the same 256-entry table `scripts/gen-luminance-table.mjs` produces as a JS array; monkey-patch `getLuminance` on a **copy** of the reference module. This gives the third column for the luminance/readability family. It is ten lines of work and it converts the project's most vulnerable number into its most credible one. *(probe: JS 81.4 ns vs native Rust 7.5 ns = 10.9×; expect most of that gap to close with the LUT.)*

### 3.6 Native validation gate — `crates/cli` must be exercised by the real evidence

Right now the fast thing and the verified thing are different binaries. Before publishing **any** number from configs D or E:

1. Add an env switch to `/Users/danishalisiddiqui/Dev-Setups/1aug/tests/original/tinycolor.js` (that file is explicitly *not* part of the port and *not* part of the original — its own header says so, and D-017 covers it). `TINYCOLOR_TRANSPORT=native` builds `buildTinyColorApi(dispatch)` over a persistent child process instead of wasm. `tests/original/test.js` stays byte-identical; `tests/HASHES.txt` still verifies.
2. The adapter (`shim/api.js` needs `dispatch` to be **synchronous**, so this is the required shape):

```js
const { spawn } = require("node:child_process");
const fs = require("node:fs");
const child = spawn("/Users/danishalisiddiqui/Dev-Setups/1aug/target/release/tinycolor",
                    ["rpc"], { stdio: ["pipe", "pipe", "inherit"] });
const buf = Buffer.alloc(1 << 20);
let pending = "";
function dispatch(req) {
  fs.writeSync(child.stdin.fd, req + "\n");
  for (;;) {
    const nl = pending.indexOf("\n");
    if (nl >= 0) { const line = pending.slice(0, nl); pending = pending.slice(nl + 1); return line; }
    let n = 0;
    try { n = fs.readSync(child.stdout.fd, buf, 0, buf.length, null); }
    catch (e) { if (e.code === "EAGAIN") continue; throw e; }
    pending += buf.toString("utf8", 0, n);
  }
}
```

3. Run `node /Users/.../tests/original/test.js` with `TINYCOLOR_TRANSPORT=native`. **Required result: 45/45.**
4. Run a **60-second** fuzz session with the same switch (`fuzz/harness.mjs` picks up the same env var). At an estimated ~2 ms/case over a pipe that is roughly 30k cases / >1M comparisons. **Required result: 0 divergences.** Write to `fuzz/logs/run-60s-native.json` with `port: "target/release/tinycolor rpc (Rust -> aarch64 native)"`.
5. Cite both next to every native number.

**If the adapter cannot be made to work:** the native column ships with the literal label *"not differentially validated; correctness evidence covers the wasm build only"* on every native row, and the report says so in the Disclosures. Do not quietly publish native numbers without one of these two outcomes.

> Note `panic = "abort"` is inherited from the workspace profile, so a panic inside `dispatch` closes the pipe rather than producing an error response. A fuzz session that ends early with a pipe error is a **crash**, not a clean stop — treat it as a divergence and investigate.

---

## 4. Corpus — `/Users/danishalisiddiqui/Dev-Setups/1aug/bench/corpus.json`

**Do NOT use `fuzz/generators.mjs`.** *(Explicit conflict resolution: R1 recommended it, R2 recommended against.)* Siding with R2/R3: the fuzz generators are deliberately boundary-biased (NaN, `Infinity`, `1e-7%`, pipe separators, malformed) — they exist to find divergences, not to represent usage. Using them would over-weight failure paths, which is a *different question* with a known direction. Instead the invalid path is measured **as its own named row** (`parse-invalid`), never blended in.

**512 entries**, generated once by `/Users/.../bench/gen-corpus.mjs` with a seeded `mulberry32` (reuse the one in `fuzz/generators.mjs`), **committed to the repo**, hashed (sha256) into every results file, read byte-identically by both harnesses in **file order** (no reshuffling per side).

Stated weights (an assumption about design-system / CSS-tooling usage, **not** a measurement of any real codebase — say so in the report):

| class | weight | notes |
|---|---|---|
| hex6 | 40% | most common real format |
| hex3 | 10% | |
| hex8 / hex4 | 8% | |
| named | 12% | **sampled uniformly across the alphabet**, explicitly including `aliceblue` *and* `yellowgreen`; `parse.rs:111` is a front-to-back scan and `color.rs:237` is a back-to-front scan — the two are inverted, a ~100× swing decided by corpus choice |
| `rgb()` / `rgba()` | 10% | |
| `hsl()` / `hsla()` | 8% | |
| object literals | 4% | the only path that skips `string_input_to_object` |
| invalid | 5% | worst parse case on both sides; a colour input field produces these constantly |
| whitespace-padded / mixed case | 3% | |

~20% of valid entries must be **achromatic** (`r==g==b`) — `rgb_to_hsl` early-returns on `max == min` and `hsl_to_rgb` short-circuits on `s == 0`, so a corpus of saturated hues is measurably more expensive than a neutral ramp on both sides.

**512 entries ≈ 20 KB stays L2-resident**, so the array itself is not what is being measured.

**Clone semantics trap — pin it:** neither harness may call Rust's derived `Clone`. `TinyColor::clone_color()` (color.rs:302) is `TinyColor::from_str(&self.to_string(None))` — a full serialise + reparse, matching upstream's `tinycolor(this.toString())`. The derived `Clone` is a struct copy. They differ by ~an order of magnitude and both are spelled "clone". **Both harnesses reconstruct from the corpus string each iteration** (`TinyColor::from_str(s)` / `tinycolor(s)`); if a clone is genuinely needed, it is `clone_color()`. The matching checksums (§3.2) verify this.

### Two scenario rows (and only two)

Per-operation rows are the primary table. Two scenarios are added because they are the **only** place the GC-tail claim can honestly be made:

- **`scenario-palette`** — 10 shades per seed via `lighten`/`darken`, 64 seeds, reconstructing from the corpus string each time.
- **`scenario-harmony`** — `triad` + `tetrad` + `splitcomplement` + `analogous()` + `monochromatic(6)`, each `.map(toHexString)`. Each allocates 3–6 colours per call; on the shipped path that is N slab inserts + N `FinalizationRegistry` registrations. Run for a **fixed 10 s wall-clock budget per config** rather than a fixed iteration count, so V8 experiences a representative number of scavenges. Do **not** pass `--expose-gc` or force collection — that would hide the effect being measured.

---

## 5. Warmup and timing policy (answers Q4 — how the JS reference is kept fair)

### 5.1 Warmup: **fixed 200,000 operations per side, identical policy, published curve**

*(Explicit conflict resolution: R1 wanted an adaptive stability criterion — 5 consecutive 1k-batch medians within 2%. R2 wanted a fixed 100k discard. Siding with R2, at 200k, plus R1's curve.)*

Reason for rejecting the adaptive criterion: this is a **fanless M4 MacBook Air**. Batch medians drift with DVFS and thermal ramp by more than 2% over a multi-minute run, so a 2%-convergence criterion may never converge and can hang the run, and it makes warmup length non-deterministic between reps — which is itself a confound. A fixed count that is demonstrably past the plateau is reproducible and provable.

Proof it is past the plateau, published as artifact: `--emit-curve` prints the **first 40 batch medians (1k ops each) for config A**, so the Ignition → Sparkplug → TurboFan tier-up is visible in the results file. Node v20.19.5 runs V8 11.3.244.8 with **no Maglev**; `stringInputToObject`/`inputToRGB` need ~1e4–1e5 invocations to reach TurboFan. 200k clears it with margin; at ~800 ns/op it costs ~160 ms. If the curve shows the plateau is not reached by batch 200, raise the warmup and re-run — do not publish.

**Rust gets the same 200k warmup**, not zero. This matters in both directions: it forces `once_cell::Lazy<Matchers>` (parse.rs:76, eleven `Regex` objects) and warms caches/branch predictors, and skipping it would silently delete the port's largest fixed cost — which is exactly the cost the startup story turns on.

**First-call cost is reported separately, never folded in.** `parse.rs:76` regex construction is *(probe: `cli bench parse-hex 1` − `cli bench noop 0` = 6.44 − 3.60 = ~2.84 ms)*. The same binary can honestly report ~milliseconds-per-colour or ~nanoseconds-per-colour depending only on iteration count. Every number states which bucket it is in.

### 5.2 Timing mode: **hybrid, by operation cost**

*(Explicit conflict resolution: R1 said batch everything and never quote per-op p99; R2 said per-iteration sample everything and compute real percentiles. Siding with R3's hybrid, which resolves them.)*

| operation cost | mode | what is reported |
|---|---|---|
| **< 1 µs/op** (all of config A, all of config D, config E) | 1,000-op batches, one clock pair per batch | median / p90 / p99 of the **distribution of per-batch means**, explicitly labelled as such, with N and batch size in the caption. **No per-op p99 claim.** |
| **≥ 1 µs/op** (config B, config C, `scenario-harmony`, `scenario-palette`) | one clock pair per operation into a preallocated array | genuine **p50 / p90 / p99 / p99.9 / max** |
| **milliseconds** (startup) | one clock pair per process spawn | genuine p50 / p99 / min / max |

Justification for the split: `Instant::now()` on Apple Silicon is ~20–25 ns against a ~40 ns mach-timebase quantum, and `process.hrtime.bigint()` costs more still — timing a 150 ns operation per-iteration inflates each side by a *different* 15–40% and charges the larger penalty to JS. But the shipped path is *(probe)* 7.5 µs/op, which has three orders of magnitude of clock headroom, so the headline path gets a real p99 — and it is the only place where the D-018 `FinalizationRegistry` tail can appear at all.

`timer-floor` and `noop` are printed in every results file next to the numbers, **never silently subtracted**.

### 5.3 Throughput

`throughput = total_operations / total_wall_time` over a fixed window. **Never `1/mean(latency)`** (Jensen's inequality biases it high). **Never `mean ± stddev`** — the shipped path's distribution is right-skewed by FinalizationRegistry reclamation bursts, and symmetric error bars would conceal a tail that is a genuine property of the design.

---

## 6. Throughput protocol — exact commands, N, reps

### 6.1 Interleaving

*(Explicit conflict resolution: R2 wanted upstream and the shim driven in the **same** Node process, same loop. Siding against: **one Node process per (config, row)**.)* Reason: sharing a process lets config A's monomorphic ICs at `stringInputToObject` be polluted by config B's traffic (and vice versa), which is a confound neither reviewer priced in. Process isolation gives both implementations a clean V8 under an identical harness file and identical flags, which is the fairness property R2 actually wanted. Corpus/loop/warmup identity is preserved because both configs are the *same file* with a flag.

Interleaving therefore happens at **row granularity inside one wall-clock window**, driven by `/Users/danishalisiddiqui/Dev-Setups/1aug/bench/run.py`:

```
for rep in 1..3:
    order = shuffle([(cfg, row) for cfg in [A,B,C,D,E] for row in ROWS])   # seeded, recorded
    for (cfg, row) in order:
        run one subprocess, capture the single JSON line, append to results
```

Discard the first 10 s of each rep (a fixed "burn-in" row list run and thrown away) so thermal ramp is not encoded into the first measurements. Rep 2 additionally runs with the order **reversed** relative to rep 1; report both.

### 6.2 Iteration counts

| config | N per row | rationale |
|---|---|---|
| A `js-upstream` | **1,000,000** | ~800 ns/op → ~0.8 s |
| D `native-core` | **1,000,000** | ~1.2 µs/op worst → ~1.2 s |
| E `native-rpc` | **200,000** | ~µs-scale |
| C `wasm-raw` | **200,000** | *(probe 4.3 µs/op)* → ~0.9 s |
| B `wasm-shipped` | **200,000** | *(probe 7.6 µs/op)* → ~1.5 s |
| scenario rows | **10 s fixed wall-clock budget** | so V8 reaches a representative GC cadence |

Warmup 200,000 ops for every config and row. Rep 1 additionally runs every row at `2N` for the linearity guard.

**Reps: 3 full runs**, back to back within one session, separated by a 60 s idle gap. Publish the **between-rep spread** alongside the within-rep spread and state plainly: *"between-rep spread is measured within a single session and therefore understates true run-to-run variance."*

### 6.3 Exact invocations

```bash
# native (configs D, E)
/Users/danishalisiddiqui/Dev-Setups/1aug/target/release/tinycolor bench parse-hex 1000000
/Users/danishalisiddiqui/Dev-Setups/1aug/target/release/tinycolor bench rpc-round-trip 200000
/Users/danishalisiddiqui/Dev-Setups/1aug/target/release/tinycolor bench timer-floor 1000000
/Users/danishalisiddiqui/Dev-Setups/1aug/target/release/tinycolor bench noop 1000000

# JS (configs A, B, C) — default flags, no V8 tuning, ever
node /Users/danishalisiddiqui/Dev-Setups/1aug/bench/js-bench.cjs \
     --config upstream --row parse-hex --n 1000000 --warmup 200000 --mode batch
node /Users/danishalisiddiqui/Dev-Setups/1aug/bench/js-bench.cjs \
     --config wasm-shipped --row round-trip --n 200000 --warmup 200000 --mode per-op
node /Users/danishalisiddiqui/Dev-Setups/1aug/bench/js-bench.cjs \
     --config upstream --row parse-hex --n 200000 --warmup 0 --mode batch --emit-curve

# luminance third column
node /Users/danishalisiddiqui/Dev-Setups/1aug/bench/js-lut-luminance.cjs \
     --row luminance --n 1000000 --warmup 200000 --mode batch
```

### 6.4 Row list (final)

`parse-hex` · `parse-rgb` · `parse-hsl` · `parse-name-early` · `parse-name-late` · `parse-invalid` · `parse-object` · `to-hex` · `to-rgb-string` · `to-hsl-string` · `to-name-hit` · `to-name-miss` · `lighten` · `mix` · `luminance` · `readability` · `most-readable` (A/B/C/E only) · `round-trip` · `scenario-palette` · `scenario-harmony`

**No aggregate. No geometric mean. No "Nx faster" headline number across rows.** All three reviewers converge here and they are right: a geomean over 20 rows where the luminance LUT wins 11× would drown the 5–20× `to-name` loss. If a single number is ever quoted it is the `round-trip` row on config B, labelled *"parse + format round trip, shipped path"* — never "TinyColor".

---

## 7. Startup — exact protocol

No hyperfine. Python driver `/Users/danishalisiddiqui/Dev-Setups/1aug/bench/startup.py`, `time.perf_counter_ns()` around `subprocess.run(..., capture_output=True)`.

**Nine cells, 50 timed runs each, one untimed run first, first 5 timed samples kept and reported as the "cold" row (not deleted).** Cells interleaved round-robin, not run in blocks.

```python
NODE = "/Users/danishalisiddiqui/.nvm/versions/node/v20.19.5/bin/node"
R    = "/Users/danishalisiddiqui/Dev-Setups/1aug"
CELLS = {
 "spawn-floor":            ["/usr/bin/true"],
 "node-bare":              [NODE, "-e", ""],
 "node-require-js":        [NODE, "-e", f'require("{R}/upstream/tinycolor.reference.js")'],
 "node-js-first-colour":   [NODE, "-e", f'require("{R}/upstream/tinycolor.reference.js")("#ff0000").toHexString()'],
 "node-require-wasm":      [NODE, "-e", f'require("{R}/tests/original/tinycolor.js")'],
 "node-wasm-first-colour": [NODE, "-e", f'require("{R}/tests/original/tinycolor.js")("#ff0000").toHexString()'],
 "cli-version":            [f"{R}/target/release/tinycolor", "version"],
 "cli-noop-0":             [f"{R}/target/release/tinycolor", "bench", "noop", "0"],
 "cli-parse-hex-1":        [f"{R}/target/release/tinycolor", "bench", "parse-hex", "1"],
 "cli-parse-hex-1000":     [f"{R}/target/release/tinycolor", "bench", "parse-hex", "1000"],
}
```

Derived quantities, each published as its own line:

- **`spawn-floor`** — the Python driver's own fork/exec cost, **subtracted only where explicitly stated, and always shown** *(probe: 2.91 ms median)*.
- **V8 bootstrap eliminated** = `node-bare` − `spawn-floor` *(probe: ~28 ms)* — this is the runtime-elimination number, and it is **not** attributable to TinyColor.
- **Library-attributable JS startup** = `node-js-first-colour` − `node-bare` *(probe: ~6.4 ms)*.
- **Rust lazy regex build** = `cli-parse-hex-1` − `cli-noop-0` *(probe: ~2.84 ms)* — the port's largest fixed cost.
- **Amortisation** = `cli-parse-hex-1000` − `cli-parse-hex-1`, so amortisation is *shown*, not chosen.
- **Shipped-wasm startup regression** = `node-wasm-first-colour` − `node-js-first-colour`. *(probe: 145.9 ms vs 37.4 ms — the shipped artifact is ~4× SLOWER to start than plain JS on Node.)* **This must be in the headline startup table.**

Report **min / median / p99 / max** per cell. Phrase every startup claim in **absolute milliseconds of bootstrap eliminated per invocation**, decomposed into V8 boot / module eval / first-colour work. **Never as a speed multiple over TinyColor without the `node-bare` row visible on the same screen.**

Never rebuild between startup runs (P4). Every binary gets one untimed execution first, so page-cache state is symmetric.

---

## 8. RSS — exact protocol

`/usr/bin/time -l` (verified working on macOS 26.5.2), field **`maximum resident set size`**. Also record `peak memory footprint`, which macOS reports separately and is the more comparable figure.

```bash
/usr/bin/time -l /usr/bin/true
/usr/bin/time -l node -e ""
/usr/bin/time -l node bench/rss.cjs --config upstream    --n 1000000 --mode transient
/usr/bin/time -l node bench/rss.cjs --config wasm-shipped --n 1000000 --mode transient --no-reset
/usr/bin/time -l node bench/rss.cjs --config wasm-shipped --n 1000000 --mode transient --reset-every 200
/usr/bin/time -l node bench/rss.cjs --config upstream    --n 1000000 --mode retained
/usr/bin/time -l node bench/rss.cjs --config wasm-shipped --n 1000000 --mode retained --no-reset
/usr/bin/time -l ./target/release/tinycolor bench parse-hex 1000000
/usr/bin/time -l ./target/release/tinycolor bench rpc-round-trip 1000000
```

`bench/rss.cjs` additionally samples, every 10k constructions, into a JSONL sidecar:
`process.memoryUsage().rss`, `wasmMemory.buffer.byteLength` (so the linear-memory contribution is isolable), and `tinycolor.__liveHandles()`.

**The default-GC, no-`__reset` row is the headline memory row.** *(probe: a 240,000-op run with no `__reset` ended with `__liveHandles() === 240002` — handles accumulate exactly as D-018 describes.)* A `__reset` regime produces a flat line describing a harness, not a consumer; `fuzz/logs/latest.json`'s `peak_live_handles: 13200` exists **only** because `fuzz/harness.mjs:216` calls `port.__reset()` every 200 cases — without it the same run reached ~4.6M handles and trapped with "memory access out of bounds".

Additionally run a **growth-to-failure** cell: construct colours with no `__reset` until either a wasm trap or 5M handles or 120 s, and report peak RSS, live-handle count and whether it trapped. That is a genuine property of the shipped design and belongs in the report.

State explicitly: **macOS "maximum resident set size" counts pages mapped from the dyld shared cache and V8's mapped regions, and is not comparable to Linux RSS.** *(probe: `node -e ""` = 34.9 MB max RSS / 9.55 MB peak footprint.)*

---

## 9. Size — three separate claims, not one

Framed as R1 proposed, with R2's correction on wasm-opt. **Measured, not estimated:**

| claim | JS | port | direction |
|---|---|---|---|
| **library bytes shipped to a browser** | `upstream/tinycolor.reference.js` **37,584 B** (9,836 B gzip -9) | `web/pkg/tinycolor_wasm_bg.wasm` **1,160,040 B** (432,849 B gzip) — and `web/tinycolor.js`, the base64-inlined bundle, **1,559,420 B** (632,507 B gzip) | **port loses ~31× raw / ~41× as shipped bundle** |
| **runtime required** | V8 | V8 for the wasm path; **none** for native | wasm path is **not** runtime elimination |
| **total bytes for a standalone colour tool** | node **88,452,192 B** + 37,584 B | `target/release/tinycolor` **1,742,048 B** | **port wins ~50×** |

**Correction to reviewer input:** R1 asserted the wasm had been through `wasm-opt` (1,418,333 → 1,160,040). **Verified false — `wasm-opt` is not installed on this machine** (`which wasm-opt` → not found; `wasm-pack build --release` ran without it). R2 is correct. The report must say the wasm has **not** been `wasm-opt`'d and that `-Oz` would likely reduce it materially.

Name the cause: the bulk is the **`regex` crate's DFA engines** (ten `Regex` objects, parse.rs:76, present because D-007's deliberately permissive matchers must be reproduced exactly) plus **`serde_json`** (D-001) plus f64 formatting. Explained beats conceded.

Also note `tests/original/pkg/tinycolor_wasm_bg.wasm` is **1,160,676 B** — a *different* build (nodejs target) from `web/pkg`'s 1,160,040 B (no-modules target). Hash both into the results file so it is unambiguous which one produced which number.

---

## 10. `bench/results.json` — exact schema

One file per full run: `/Users/danishalisiddiqui/Dev-Setups/1aug/bench/results/run-<utc-iso>.json`, plus a symlink `bench/results/latest.json`. **Raw per-batch and per-op samples are emitted to sidecar JSONL** (`run-<utc-iso>.samples.jsonl`) so percentiles can be recomputed independently — that is the actual credibility win.

```jsonc
{
  "schema_version": 1,
  "run_id": "2026-08-02T20:14:07Z",
  "run_index": 1,                       // 1..3
  "aborted": false,
  "abort_reason": null,                 // e.g. "linearity guard failed: parse-hex"

  "environment": {
    "cpu": "Apple M4",
    "machine": "Danishs-MacBook-Air.local",
    "uname": "Darwin ... arm64",
    "macos": "26.5.2",
    "power_source": "AC Power",
    "low_power_mode": 0,
    "battery_percent": 100,
    "node_version": "v20.19.5",
    "v8_version": "11.3.244.8-node.30",
    "node_exec_argv": [],               // MUST be empty — proves no V8 flags
    "node_binary_bytes": 88452192,
    "rustc_version": "rustc 1.96.0 (ac68faa20 2026-05-25)",
    "cargo_version": "cargo 1.96.0 (30a34c682 2026-05-25)",
    "cargo_profile": { "opt_level": 3, "lto": true, "codegen_units": 1, "panic": "abort" },
    "wasm_opt_applied": false,
    "disk_free_bytes": 6979321856,
    "hyperfine_available": false,
    "docker_used": false
  },

  "provenance": {
    "cli_binary_path": "/Users/.../target/release/tinycolor",
    "cli_binary_bytes": 1742048,
    "cli_binary_mtime": "2026-08-02T13:17:44Z",
    "cli_binary_sha256": "…",
    "newest_source_mtime_under_crates": "2026-08-02T13:16:02Z",
    "freshness_ok": true,
    "wasm_node_sha256": "…",            // tests/original/pkg/tinycolor_wasm_bg.wasm
    "wasm_node_bytes": 1160676,
    "wasm_web_sha256": "…",             // web/pkg/tinycolor_wasm_bg.wasm
    "wasm_web_bytes": 1160040,
    "upstream_pinned_commit": "b49018c9…",
    "upstream_js_sha256": "…",
    "corpus_path": "bench/corpus.json",
    "corpus_sha256": "…",
    "corpus_entries": 512,
    "corpus_class_mix": { "hex6": 205, "hex3": 51, "hex8_hex4": 41, "named": 61,
                          "rgb": 51, "hsl": 41, "object": 20, "invalid": 26,
                          "whitespace_case": 16, "achromatic_share": 0.20 },
    "order_seed": 8123,
    "row_order": ["…"]                  // the actual randomized interleave order
  },

  "validation": {
    "wasm": { "suite": "45/45", "suite_cmd": "node tests/original/test.js",
              "fuzz_log": "fuzz/logs/run-wasm-300s-seed1.json",
              "fuzz_cases": 689345, "fuzz_comparisons": 31020525, "fuzz_divergences": 0 },
    "native": { "suite": "45/45", "suite_cmd": "TINYCOLOR_TRANSPORT=native node tests/original/test.js",
                "fuzz_log": "fuzz/logs/run-native-120s-seed11.json",
                "fuzz_cases": 74303, "fuzz_comparisons": 3343635, "fuzz_divergences": 0,
                "validated": true }     // if false, every native row is labelled unvalidated
  },

  "timer": {
    "rust_instant_pair_ns": 0.0,        // `bench timer-floor`
    "rust_noop_ns_per_op": 1.64,        // `bench noop`
    "js_hrtime_pair_ns": 0.0,
    "js_noop_ns_per_op": 0.0
  },

  "warmup": {
    "policy": "fixed 200000 operations per side, identical for every config",
    "ops": 200000,
    "js_tier_up_curve_ns_per_op": [ /* first 40 batch medians, config A, 1k ops each */ ],
    "curve_plateau_reached_at_batch": 0
  },

  "throughput": [
    {
      "row": "parse-hex",
      "config": "wasm-shipped",         // upstream | wasm-shipped | wasm-raw | native-core | native-rpc | upstream-lut
      "mode": "per-op",                 // per-op | batch
      "batch_size": null,               // 1000 when mode == "batch"
      "n": 200000,
      "warmup_ops": 200000,
      "wall_ns": 1512000000,
      "throughput_ops_per_sec": 132275.1,   // n / wall, never 1/mean
      "ns_per_op": { "p50": 0, "p90": 0, "p99": 0, "p999": 0, "max": 0, "mean": 0 },
      "percentiles_are_per_op": true,   // false ⇒ they describe per-batch MEANS
      "checksum": "0x…",
      "samples_file": "run-….samples.jsonl",
      "samples_offset": 0,
      "linearity": { "n": 200000, "n2": 400000,
                     "ns_per_op_n": 0, "ns_per_op_n2": 0, "ratio": 1.0, "ok": true },
      "notes": "includes JSON.stringify/parse ×2, wasm string crossing ×4, slab insert, FinalizationRegistry.register — D-001"
    }
  ],

  "startup_ms": [
    { "cell": "node-bare", "cmd": ["node","-e",""], "runs": 50, "cold_samples": [ /* first 5 */ ],
      "min": 0, "p50": 0, "p99": 0, "max": 0, "spawn_floor_subtracted": false }
  ],

  "startup_derived_ms": {
    "spawn_floor": 0, "v8_bootstrap_eliminated": 0, "js_library_attributable": 0,
    "rust_lazy_regex_build": 0, "rust_amortisation_1_to_1000": 0,
    "wasm_startup_regression_vs_js": 0
  },

  "rss": [
    { "cell": "node+wasm-shipped transient 1e6, no __reset",
      "cmd": "…", "max_rss_bytes": 0, "peak_footprint_bytes": 0,
      "final_live_handles": 0, "wasm_linear_memory_bytes": 0,
      "reset_hook_used": false, "trapped": false,
      "note": "macOS max RSS counts dyld shared cache + V8 mapped regions; not comparable to Linux RSS" }
  ],

  "rss_growth_to_failure": {
    "config": "wasm-shipped", "reset_hook_used": false,
    "handles_at_trap": null, "trapped": false, "peak_rss_bytes": 0,
    "elapsed_s": 0, "note": "D-018; __reset() is a harness hook, not part of the API"
  },

  "size_bytes": {
    "upstream_js_raw": 37584, "upstream_js_gzip9": 9836,
    "wasm_web_raw": 1160040, "wasm_web_gzip9": 432849,
    "web_bundle_raw": 1559420, "web_bundle_gzip9": 632507,
    "native_binary": 1742048, "node_binary": 88452192,
    "wasm_opt_applied": false
  },

  "between_run_spread": {               // populated by bench/aggregate.py after all 3 reps
    "note": "within a single session; understates true run-to-run variance",
    "per_row": { "parse-hex": { "wasm-shipped": { "min": 0, "max": 0, "spread_pct": 0 } } }
  },

  "disclosures": [ /* the §11 strings, verbatim, embedded in the artifact */ ]
}
```

---

## 11. Disclosures — must appear in the report, in the same font size as the wins

1. **The shipped artifact is slower than the original at almost everything.** Per-operation throughput through `shim/api.js` → wasm → `rpc::dispatch` is roughly an order of magnitude worse than calling `upstream/tinycolor.reference.js` directly *(planning probe: 7,555 ns/op vs 783 ns/op for `tinycolor(x).toHexString()`)*. The cause is D-001: two full JSON round trips per user-visible call (`op:"new"` then `op:"call"`), four wasm string crossings, `serde_json::from_str` into a `Value`, a full `TinyColor::clone` at `crates/core/src/rpc.rs:37`, a slab insert, and a `FinalizationRegistry.register`. D-001 chose one narrow JSON boundary over ~45 hand-written FFI signatures so the *unmodified* upstream suite could drive the port. Report the number, name the decision, say why the trade was made.
2. **The native core is also slower than V8 on the common paths.** *(probe: native `parse-hex` 1,219 ns/op vs upstream JS 774 ns/op; native `to-hex` 391 ns vs 152 ns; native `to-hsl-string` 611 ns vs 152 ns.)* The port does not win on colour maths throughput. It wins on process startup and on the luminance family.
3. **`getLuminance` / `readability` / `isReadable` / `mostReadable` are fast because of a lookup table, not because of Rust.** D-012 replaced three `Math.pow(x, 2.4)` calls with three indexes into a generated 256-entry f64 table (`crates/core/src/luminance.rs`). The table exists for **bit-exactness across libm implementations**, not for speed. The `upstream-lut` column quantifies exactly how much of the gap survives when the algorithm is held constant; publish that fraction. *(probe: JS 81 ns vs Rust 7.5 ns before the LUT column exists.)*
4. **`toName()` and named-colour parsing are algorithmically worse in the port.** `crates/core/src/color.rs:237` is `HEX_NAMES.iter().filter(|(h,_)| *h == hex).next_back()` — a 149-entry linear scan with **no early exit**, and a miss is the common case. `crates/core/src/parse.rs:111` is `NAMES.iter().find(...)`, also linear and **front-to-back**, while the `HEX_NAMES` scan is back-to-front — so per-input cost varies by alphabetical position in opposite directions. Upstream uses `names[color]` and `hexNames[hex]`, both O(1). Published as `to-name-hit` / `to-name-miss` / `parse-name-early` / `parse-name-late` rows, **not** absorbed into an average. Not fixed before benchmarking (see #12).
5. **Parsing runs each functional-notation regex 4–5 times.** `crates/core/src/parse.rs:140-190` guards with `is_match` and then calls `captures` once per captured field, where upstream does `if (match = matchers.rgb.exec(color))` — one exec, reused. Additionally the if-chain tries rgb, rgba, hsl, hsla, hsv, hsva **before** any hex branch, so hex6 — the most common real format — pays six wasted DFA passes first. This is a defect the **port introduced**, not one inherited. Any `parse-rgb`/`parse-hsl` gap is measuring this, not "Rust vs V8".
6. **Regex construction is lazy.** `crates/core/src/parse.rs:76` builds eleven `Regex` objects inside a `once_cell::sync::Lazy` on the first parse *(probe: ~2.84 ms)*. Every number states whether that cost is included or amortised — the same binary can honestly report milliseconds-per-colour or nanoseconds-per-colour depending only on iteration count.
7. **The wasm path is not runtime elimination.** It is 1,160,040 bytes replacing 37,584 bytes (~31× raw; ~41× as the base64-inlined `web/tinycolor.js` at 1,559,420 bytes) and it **still requires V8**. It has **not** been through `wasm-opt` (not installed here); `-Oz` would likely reduce it materially. The bulk is the `regex` crate's DFA engines (present to reproduce D-007's deliberately permissive matchers) plus `serde_json`. Only the native binary (1,742,048 B replacing an 88,452,192 B node install) supports the runtime-elimination claim.
8. **The wasm path is also ~4× slower to start than plain JS on Node.** *(probe: 145.9 ms vs 37.4 ms to first colour.)* wasm module compilation plus the module-load-time `{"op":"static","method":"names"}` RPC are the cause.
9. **Startup wins are mostly V8, not TinyColor.** `node -e ""` alone is ~31 ms *(probe)* of the ~37 ms JS first-colour figure. Every startup claim is phrased in **absolute milliseconds of bootstrap eliminated per invocation**, decomposed, with the `node-bare` baseline row adjacent. No speed multiple is attached to TinyColor.
10. **`__reset()` is a harness hook, not part of the API.** D-018: colours live in a Rust `HashMap` invisible to V8's GC, freed only when `FinalizationRegistry` fires. `fuzz/logs/latest.json`'s `peak_live_handles: 13200` exists only because `fuzz/harness.mjs:216` calls `port.__reset()` every 200 cases; without it the same run reached ~4.6M handles and trapped with "memory access out of bounds". *(probe: a 240,000-op run with no reset ended at 240,002 live handles.)* The headline memory row is default-GC with **no** `__reset`; the `__reset` row is labelled harness-only.
11. **The shipped configuration's memory footprint is strictly worse than plain JS.** It is node's RSS **plus** 1.16 MB of module **plus** compiled code **plus** wasm linear memory **plus** the slab. Any "Rust uses less memory" statement refers to the **native** binary and credits the port with V8's absence. macOS "maximum resident set size" counts dyld-shared-cache and V8 mapped pages and is **not comparable to Linux RSS**.
12. **Two known regressions were measured, not fixed.** The regex double-execution (#5) and the linear name scans (#4) are both fixable (single `captures`; `phf` or sorted+binary-search). They were deliberately **not** fixed before benchmarking, because `crates/core` carries all 45/45 and 15.4M-comparison evidence and changing it would invalidate that evidence within this run's time and disk budget. File:line references and the expected direction of each fix are published. Any post-hoc "fixed" numbers appear only in a labelled appendix, and only after the suite and a fuzz session have been re-run.
13. **Which binary produced which number, on every row.** Correctness evidence for configs B and C is `tests/original/pkg/tinycolor_wasm_bg.wasm` (45/45, 31,020,525 comparisons, 0 divergences -- see `fuzz/logs/run-wasm-300s-seed1.json`). Configs D and E rest on the §3.6 native validation run — cite its case/comparison counts. If §3.6 could not be completed, every native row carries *"not differentially validated"*. **D-012 already proved the native and wasm builds of this identical source are different programs** (macOS libm `powf` matched V8 for 207/256 channel values; the wasm32 MUSL-derived `pow` for 225/256). Assume and state that they differ in speed too — wasm32 is 32-bit, so the u64 xorshift in `rpc.rs` and the f64→i64 paths in `to_int32` lower to multi-instruction sequences, and the allocator differs.
14. **Latency percentiles for sub-microsecond rows are over 1,000-op batches, not individual operations** — the Apple Silicon mach timebase gives `Instant::now()` ~40 ns granularity and ~20–25 ns of call overhead against a 150–1,200 ns operation. Those rows carry `percentiles_are_per_op: false`. Genuine per-op p50/p90/p99/p99.9/max are reported only for the shipped path (µs-scale), the scenario rows, and startup (ms-scale). The `timer-floor` and `noop` figures are printed alongside and never silently subtracted.
15. **Rust timed regions include deallocation** of returned `String`/`TinyColor` values (forced by `black_box`); the JS baseline defers that work to a GC that runs outside the measured window. The comparison is not clean in either direction and the size of the effect is unmeasured. This is why the JS side's p99/max are reported next to its mean.
16. **Corpus weights are an assumption, not a measurement** of any real codebase. `bench/corpus.json` and the full per-class breakdown are published so a reader who disagrees can re-weight without rerunning anything. The fuzz generators were **deliberately not used** for perf because they are boundary-biased and would over-weight failure paths.
17. **The `most-readable` row is not measured natively**, because `crates/cli/src/main.rs:265` is a duplicate implementation of the algorithm that ships in `rpc.rs::statics("mostReadable")`, and it pre-parses candidates outside the timed region while the shipped path re-materialises them from JSON per call. It is measured only on configs A/B/C/E.
18. **`crates/cli` is new code written for this benchmark**, not covered by the 45/45 suite or the 15.4M fuzz run except through §3.6. The matching checksums across configs are the only cross-check that all harnesses computed the same results; they are published per row.
19. **No aggregate, no geomean, no single "Nx faster".** If one number is quoted it is the `round-trip` row on config `wasm-shipped`, labelled *"parse + format round trip, shipped path"*.
20. **Platform scope and error bars.** All figures are macOS 26.5.2 / arm64 (Apple M4, **fanless MacBook Air**) with node v20.19.5, no core pinning available, other processes running. Every number carries N, batch size, warmup ops consumed, power state, versions, cargo profile, and the between-rep spread across 3 reps — **measured within one session, which understates true variance**. D-012 already established this port is target-sensitive; no performance claim generalises to Linux/x86_64 without measurement.
21. **hyperfine is not installed**; the timing wrappers are hand-rolled and shipped in the repo (`bench/run.py`, `bench/startup.py`, `bench/js-bench.cjs`). Their measured overhead (`spawn-floor`, `timer-floor`, `noop`) is published. **Node ran with default flags and empty `process.execArgv`** — recorded in the results file, not merely asserted.
22. **No number in this report was taken inside Docker.** Docker Desktop on Apple Silicon is a Linux VM; the Dockerfile ships as a reproducibility recipe only and is labelled as producing different absolute numbers.

---

## 12. Explicitly SKIPPED (and why)

| Skipped | Why |
|---|---|
| Any geometric mean / blended "Nx faster" | Would let the 11× luminance LUT win pay for the 5–20× `to-name` loss. All three reviewers agree. |
| Criterion | Pulls plotters + rayon + ~100 crates onto a volume with 6.5 GB free and `target/` already at 307 MB; its default output is mean-centric anyway. ~150 lines of `std::time` is smaller and more honest. |
| Any number from Docker | See disclosure #22. |
| V8 flags (`--jitless`, `--no-opt`, `--allow-natives-syntax`, `--expose-gc` on the headline) | Every one makes the published multiple a property of the flag. `--expose-gc` specifically would hide the D-018 tail that `scenario-harmony` exists to expose. |
| A bench-only `#[wasm_bindgen]` export | Config C obtains the same decomposition from the shipped export. |
| Per-op p99 on sub-µs rows | Below the hardware's resolution; would be a quantisation bucket or a scheduler preemption reported as a property of the port. |
| Native `most-readable` | Duplicate implementation (§3.1). |
| `polyad` | D-015: commented out upstream, excluded from fuzzing. |
| `fuzz/generators.mjs` as the bench corpus | Adversarial by design; measures the failure path, which is a different question with a known direction (§4). |
| Fixing the regex double-exec / name scans before measuring | Would invalidate the correctness evidence within this budget. Disclosed instead (#12). |
| Deleting `target/release/tinycolor` as "stale" | **Resolved:** `crates/cli/src/main.rs` now exists, `crates/cli` is in the workspace `members`, and `target/release/tinycolor.d` correctly references existing files. R1's finding was accurate at the time it was written and is now obsolete. The freshness gate (P2) stays. |

---

## 13. Reviewer conflicts — resolutions, in one table

| Conflict | Reviewers | Decision | Reason |
|---|---|---|---|
| Headline = native or shipped? | R3 (native primary) vs R2 (shipped primary) vs R1 (three-layer, no headline) | **Shipped wasm+RPC is the headline; RPC included; native is a labelled second column; decomposition published alongside** | Correctness evidence covers wasm only; it is what consumers get; the FAQ requires disclosing the regression, and it *is* a regression. |
| Warmup: adaptive vs fixed | R1 (adaptive 2% stability) vs R2 (fixed 100k) vs R3 (10k) | **Fixed 200k both sides + publish the JS tier-up curve** | Fanless M4 Air drifts >2% thermally; adaptive may never converge and is non-deterministic between reps. The published curve delivers R1's evidentiary value with a reproducible runtime. |
| Percentiles: batch vs per-op | R1 (batch everything) vs R2 (per-op everything) | **Hybrid by operation cost (R3's split)** | Native micro-ops are at the clock floor → batch means, no p99 claim. The shipped path is µs-scale → real per-op p99, which is also the only place D-018's tail is visible. |
| Corpus source | R1 (fuzz generators) vs R2/R3 (hand-weighted, explicitly *not* fuzz) | **Hand-weighted 512-entry `bench/corpus.json`, invalid slice as its own row** | Fuzz generators are boundary-biased by design; using them silently would be a choice with a known direction. R1's mechanism (one file, both sides, hashed) is adopted. |
| Same Node process for A and B? | R2 (same process, same loop) vs implied isolation | **One Node process per (config,row); interleave at row granularity** | Sharing a process lets config B's traffic make config A's `stringInputToObject` ICs megamorphic. Corpus/loop/warmup identity is preserved via one harness file + a flag. |
| Was the wasm `wasm-opt`'d? | R1 (yes) vs R2 (no) | **No — verified, `wasm-opt` is not installed** | R1's figure was incorrect; report says not optimised and that `-Oz` would help. |
| Stale binary / missing workspace member | R1 | **Obsolete** — `crates/cli` exists and is in `members`; keep freshness gate P2 | Verified against the current tree. |
| Fix regex double-exec first? | R2 (fix then measure) vs R1 (fix or disclose) | **Disclose, do not fix** | Touching `crates/core` invalidates 45/45 + 15.4M comparisons within this budget. A named, file:line'd port defect is worth more in a Port Mortem than a marginally better number. |
| JS-with-LUT third column | R3 only | **Adopt** | ~10 lines; converts the project's most attackable number into its most credible one. |

---

## 14. Execution order and time budget

```
0.  Plug in AC, disable Low Power Mode, close other apps.          (P1)
1.  cargo build --release                                          (~30 s, once; then P4 applies)
2.  node bench/gen-corpus.mjs  →  bench/corpus.json  (commit it)    (~5 s)
3.  cargo test -p tinycolor-cli --release                           (~10 s)
4.  §3.6 native validation gate: suite + 60 s fuzz                  (~3 min)
5.  Size table (ls + gzip -9 + shasum)                              (~10 s)
6.  Startup matrix, 9 cells × 50 runs, interleaved                  (~5 min)
7.  RSS matrix + growth-to-failure                                  (~8 min)
8.  Throughput rep 1 (with 2N linearity guard)                      (~12 min)
9.  60 s idle; rep 2 (reversed order)                               (~9 min)
10. 60 s idle; rep 3                                                (~9 min)
11. bench/aggregate.py → between-run spread → bench/results/latest.json
12. Write the report. Disclosures §11 go in verbatim, before the tables.
```

Total ≈ 50 minutes. Abort and restart from step 8 if the linearity guard fires, if `pmset` shows the machine dropped to battery mid-run, or if any checksum mismatches across configs for the same row.

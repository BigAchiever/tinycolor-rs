# DEMO-SCRIPT.md — 5:00 demo video, shot by shot

**Entry:** tinycolor-rs — Track F (JavaScript → Rust), Port Mortem 2026
**Runtime budget:** 300 seconds, hard cap.

## The one rule this script is built around

Judges spend 5–12 minutes per project, asynchronously, and they will not clone
the repo. The rubric's largest single block (Functionality & Reliability, 40%)
is the original test suite passing against the port. **So the video opens on
that, at second zero.** No title card, no architecture diagram, no "hi, I'm…".
Architecture is earned later, and only briefly.

Everything shown is real and reproducible. Nothing is sped up except where a
shot is explicitly marked `[CUT]`, and those cuts are stated on screen.

---

## Time budget

| # | Segment | Start | Len | Why it's here |
|---|---|---:|---:|---|
| 0 | **Cold open** — the one-bit failure, unexplained | 0:00 | 0:08 | Opens a loop; buys attention for the next four minutes |
| 1 | Hashes verify | 0:08 | 0:17 | Proves the tests are upstream's, not ours |
| 2 | **The upstream test file itself, and line 5** | 0:25 | 0:25 | Proves the mechanism without architecture talk |
| 3 | 45/45, unmodified suite | 0:50 | 0:25 | The headline claim |
| 4 | 45/45 again, native transport | 1:15 | 0:25 | Not a transport fluke |
| 5 | Differential fuzz | 1:40 | 0:35 | Behavioural Equivalence, 30% |
| 6 | **Payoff: the one-ULP story (D-012)** | 2:15 | 0:45 | The most memorable material in the project |
| 7 | Browser demo on the port | 3:00 | 0:25 | It's a real library, not a test-passer |
| 8 | **The regression, honestly** | 3:25 | 0:35 | Mandatory disclosure, and a credibility play |
| 9 | Where the win is + zero unsafe | 4:00 | 0:20 | Code Quality, 20% |
| 10 | Decision log + filed upstream bug | 4:20 | 0:30 | Innovation, 10% |
| 11 | Close | 4:50 | 0:10 | One card, one sentence |

Total 5:00. Segments 0–5 and 8 are non-negotiable. Segment 2 is the one that
makes every later claim believable — if anything gets cut, it is not that one.

---

## Pre-flight (do this before recording)

Run this once. It builds everything and warms every cache so no shot is
waiting on a compile.

```bash
make demo-prep
```

That builds both artifacts, warms both transports, checks the hashes, and tells
you what to open where.

> ### Note on the fuzz log
>
> `node fuzz/harness.mjs` overwrites `fuzz/logs/latest.json` on every run, but
> the archived evidence now lives in its own files
> (`run-wasm-300s-seed1.json` and `run-native-120s-seed11.json`), so a live
> fuzz run during recording cannot destroy it. Record freely.

**Terminal setup:** one window, large font (≥ 18pt), dark theme, ~100 cols.
Clear scrollback between segments (`clear`) so each shot starts empty.
Two tabs: **T1** for commands, **T2** for the demo server.

**Browser setup:** start the demo server in T2 *before* recording, and load the
page once so segment 5 has no load stall.

```bash
python3 -m http.server 8099 --directory web       # leave running in T2
```

**Editor setup:** have `DECISIONS.md` open and scrolled to D-012 for segment 8.

---

## Segment 1 — Cold open: prove the tests are upstream's (0:00–0:20)

**Show:** empty terminal. Type live.

```bash
./scripts/verify-hashes.sh
```

Output — all four lines land at once:

```
tests/original/test.js: OK
tests/original/package.json: OK
tests/deno_asserts@0.168.0.mjs: OK
upstream/tinycolor.reference.js: OK
```

**Say:**
> "TinyColor's own test suite, sha256-pinned at the kickoff commit. Zero bytes
> changed. That matters for what happens next."

**Note:** do not explain the hash manifest. The four OKs do the work.

---

## Segment 2 — The headline: 45/45 (0:20–1:00)

**Show:** type live, let it run in real time.

```bash
node tests/run-all.mjs
```

```
────────────────────────────────────────────────────────
upstream suite: 45/45 tests passing (100.0%)
────────────────────────────────────────────────────────
```

**Say:**
> "That's the file we just hashed — 45 tests, 432 assertion call sites — running against
> a Rust port. The test file doesn't know. We never edited it. The shim sits at
> the `require` path upstream already uses, so the suite loads Rust and runs
> unchanged."

**Show while saying the last sentence:** flash `tests/original/tinycolor.js`
for ~3 seconds — just enough to see it's ~100 lines of adapter — then back to
the terminal. Do not read it aloud.

---

## Segment 3 — Same suite, second transport (1:00–1:25)

```bash
TINYCOLOR_TRANSPORT=native node tests/run-all.mjs
```

```
upstream suite: 45/45 tests passing (100.0%)
```

**Say:**
> "Same suite, same 45. This time it's not WebAssembly — it's a native Rust
> binary on the other end of a stdio pipe. Two independent transports, one core.
> The correctness lives in the port, not in the plumbing."

---

## Segment 4 — Differential fuzz (1:25–2:05)

Passing a fixed suite is table stakes. This is the segment that earns the 30%.

**Show:** start a live run, then cut to the archived result while it runs.

```bash
node fuzz/harness.mjs --seconds 30 --seed 1
```

**Say over the live counter:**
> "Beyond the fixed suite: a differential fuzzer. Random colours — hex, rgb,
> hsl, malformed junk, non-strings — into the original library on V8 and into
> the port, comparing outputs bit for bit. No epsilon. Floats match exactly or
> it's a divergence."

**[CUT]** to the archived 300-second run:

```bash
cat fuzz/logs/run-wasm-300s-seed1.json
```

Highlight on screen: `"comparisons": 31020525`, `"divergences": 0`.

**Say:**
> "Five minutes, fifteen million comparisons, zero divergences. Reproduced on
> the native transport too."

---

## Segment 5 — Upstream's own demo page, on Rust (2:05–2:45)

**Show:** browser at `http://localhost:8099`. Type a colour into the page's
input; the swatches and conversions update live.

**Say:**
> "This is TinyColor's own demo page from the pinned commit — unmodified HTML.
> Every swatch on it is now computed in Rust."

**Show:** open the browser console, type:

```js
tinycolor.__port
```

```
'rust -> wasm32'
```

**Say:**
> "Same global, same API, different language underneath. That's the whole point
> of a port — nothing downstream had to change."

**Note:** this is the shot that proves it's a working library rather than
something shaped to pass 45 tests. Worth the 40 seconds.

---

## Segment 6 — The regression, stated plainly (2:45–3:20)

Do not soften this. Judges have been burned by inflated benchmarks, and the
event FAQ makes disclosure mandatory. Leading with the bad number is the
credibility play.

**Show:** the throughput table on a static card — all nine rows, ratios visible.

| row | upstream JS | port (wasm) | |
|---|---:|---:|---|
| parse-only | 464 ns | 4475 ns | **9.6× slower** |
| to-hex-string | 645 ns | 4991 ns | **7.7× slower** |
| mix | 1465 ns | 7102 ns | **4.8× slower** |
| *(6 more rows, all slower)* | | | |

**Say:**
> "Now the part most demos skip. The port is slower. Not slightly — five to ten
> times slower on every per-operation benchmark, and the shipped WebAssembly
> build loses on startup and memory too. Seventy-one milliseconds to boot
> against eighteen for the original.
>
> The cause isn't the colour maths. It's the JSON-RPC bridge — about four
> microseconds a call, and it dominates every row. That's a deliberate trade: one
> boundary instead of forty-five FFI signatures, which is what let the
> *unmodified* test suite drive the port at all. I'd make the same call again,
> and I'm not going to pretend it was free."

**Note:** say "run-to-run spread hits 39%, so these are two significant figures"
only if the pacing allows. Accuracy over completeness here.

---

## Segment 7 — Where the win actually is, and zero unsafe (3:20–3:50)

**Show:** startup/RSS card.

| | startup p50 | max RSS |
|---|---:|---:|
| node + upstream JS | 18.4 ms | 34.3 MB |
| node + port (wasm) | 71.1 ms | 86.3 MB |
| **native binary** | **3.2 ms** | **4.4 MB** |

**Say:**
> "The win is runtime elimination. The native binary starts 5.8× faster in 4.4
> megabytes — and it carries the same 45/45 and the same fuzz evidence as the
> wasm build."

**Show:** `crates/core/Cargo.toml`, cursor on the lint line.

```toml
unsafe_code = "forbid"
```

**Say:**
> "And zero unsafe — in all three crates. Not a claim in a README; `forbid`
> makes it a compile error."

---

## Segment 8 — The decision log (3:50–4:35)

The strongest material in the project. Show the file, scroll slowly, stop on
three entries.

**Show:** `DECISIONS.md`, then land on **D-012**.

**Say:**
> "Twenty-one documented divergences. A few worth calling out.
>
> **D-012:** `Math.pow` isn't bit-identical across libm implementations — Rust's
> std and wasm's MUSL each disagreed with V8 on relative luminance. So luminance
> is a generated 256-entry lookup table instead. It now matches V8 exactly on
> every target, and the benchmark checksums agree on a full 32-bit fold.
>
> **D-003:** JavaScript's `Math.round` breaks ties toward positive infinity.
> Rust's rounds away from zero. Negative halves disagree.
>
> And four of these are bugs in *my* port, not upstream's — a handle table that
> leaked until the wasm heap died after 4.6 million colours, a native transport
> that hung every process that used it, NaN and negative zero not surviving
> JSON, and two places where idiomatic-looking Rust quietly did four times the
> work. They're in the log because a decision log that only records other
> people's mistakes isn't a decision log."

**Note:** if running long, cut D-003. Never cut the four self-inflicted bugs
(D-016, D-018, D-019, D-021) — they are the most persuasive thing in the video.

---

## Segment 9 — Close (4:35–5:00)

**Show:** single static card.

```
tinycolor-rs — Track F (JavaScript → Rust)
Upstream commit b49018c9  ·  1,188 LOC ported

45/45 upstream suite, unmodified   ·   both transports
31,020,525 fuzz comparisons        ·   0 divergences
0 unsafe (compiler-enforced)       ·   21 documented decisions
Per-op: 4.8–9.6× slower  ·  Startup: 5.8× faster, 7.8× lighter

DECISIONS.md  ·  bench/METHODOLOGY.md  ·  .port-mortem.toml
```

**Say:**
> "Correct against the original's own tests, on two transports, with the
> regression on the card instead of buried. Everything here reproduces from a
> clean checkout — the commands are in `.port-mortem.toml`."

**End.** No outro, no thanks, no music sting.

---

## Post-record checklist

- [ ] `cp /tmp/latest.json.bak fuzz/logs/latest.json` — restore the native fuzz evidence
- [ ] `./scripts/verify-hashes.sh` still exits 0
- [ ] Video is ≤ 5:00
- [ ] Every `[CUT]` is visibly labelled on screen
- [ ] No terminal frame shows a path revealing anything you don't want public
- [ ] The regression segment survived the edit at full length

---

## If you only get one take

Record segments 1, 2, 3, 4, and 6. That's 2:40 and it covers 70% of the rubric
(Functionality 40% + Equivalence 30%) plus the mandatory disclosure. Segments
5, 7, and 8 are upside — record them separately and cut them in.

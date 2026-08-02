# DECISIONS.md

Architectural divergences and behavioural subtleties encountered porting
[TinyColor](https://github.com/bgrins/TinyColor) (JavaScript, 1,187 LOC) to
Rust, pinned at upstream commit `b49018c9f2dbca313d80d7a4dad25e26143cfe01`.

Each entry records what upstream does, what a direct translation would have
produced, what the port does instead, and how the choice is verified. Entries
marked **fuzz** were found by `fuzz/harness.mjs` rather than by reading code.

Verification vocabulary:
- **unit** — a Rust `#[cfg(test)]` case pins the behaviour
- **suite** — covered by the unmodified upstream test suite
- **fuzz** — found and/or confirmed by differential fuzzing against V8
- **exhaustive** — checked over the complete input domain

---

## D-001 — Three crates behind one RPC bridge, not a direct FFI

**Decision.** The port is `crates/core` (pure logic, no I/O), with two thin
transports: `crates/wasm` (wasm-bindgen, drives the test suite in Node) and
`crates/cli` (native binary, drives benchmarks). Both speak one JSON protocol
defined in `core/src/rpc.rs`.

**Alternatives rejected.** A napi-rs native addon would have coupled the port
to Node's ABI and made the "runs standalone" requirement harder to argue. A
per-method wasm-bindgen surface would have put ~45 method signatures across
the FFI boundary, where each one is a place for a marshalling bug that looks
like a porting bug.

**Consequence.** The boundary is a single function taking and returning a
JSON string. Colours live in a handle table (`SLAB`) rather than being
serialised in and out, which preserves JS object identity (`tinycolor(x) === x`)
and makes the mutating chain methods behave as they do upstream.

The same protocol serving both transports is what made **D-012** detectable:
the native and wasm builds disagreed, and that only surfaces when both run the
same cases.

*Verified: suite, fuzz.*

---

## D-002 — Components carry their JS type, because `"1.0"` ≠ `1.0`

**Upstream.** `isOnePointZero(n)` returns true only for a *string* containing
`"."` whose value is exactly 1, and `bound01` then rewrites it to `"100%"`.

**Naive port.** Parse every component into `f64` on the way in.

**Consequence of the naive port.** `{r: "1.0"}` and `{r: 1.0}` would become
the same value. Upstream they differ by a factor of 255:

```
tinycolor({r: "1.0", g: 0, b: 0}).toHexString()  // "#ff0000"
tinycolor({r:  1.0,  g: 0, b: 0}).toHexString()  // "#010000"
```

**Port.** `convert::Unit` is an enum of `Num(f64)` / `Str(String)`, threaded
from parsing all the way to `bound01`. `is_percentage` and `is_one_point_zero`
return `false` for `Num` regardless of value, mirroring the `typeof` guard.

*Verified: unit (`bound01_percentage_and_plain`), suite.*

---

## D-003 — `Math.round` breaks ties toward +Infinity

**Upstream.** JS `Math.round(-1.5)` is `-1`; `Math.round(-0.5)` is `-0`.

**Naive port.** Rust's `f64::round` rounds ties *away from zero*: `-1.5` → `-2.0`.

**Port.** `jsnum::math_round`, implemented via floor/fract. Deliberately *not*
the `floor(x + 0.5)` shorthand, which is wrong just below a half —
`0.49999999999999994 + 0.5` rounds up to `1.0` before the floor, yielding `1`
where JS gives `0`.

Reached through `spin()` with negative hues, `convertDecimalToHex` on
out-of-range alpha, and every `toRgb()`.

*Verified: unit (`math_round_matches_js_on_ties`), fuzz.*

---

## D-004 — The hex→name map resolves collisions last-wins

**Upstream.** `hexNames = flip(names)`, and several names share a hex value
(`aqua`/`cyan` both `0ff`, `fuchsia`/`magenta` both `f0f`). Object assignment
means the later key wins.

**Naive port.** A `HashMap` built by inserting pairs — same result by luck, or
the opposite result depending on iteration order.

**Port.** `names::HEX_NAMES` is a generated slice in source order, and
`to_name()` takes the **last** match via `.next_back()`. `tinycolor("#0ff").toName()`
is `"cyan"`, not `"aqua"`.

*Verified: unit (`hex_names_flip_is_last_wins`), suite.*

---

## D-005 — `parseFloat` is a prefix parse, not a validating parse

**Upstream.** `parseFloat("50%")` is `50`. Every percentage unit depends on it.

**Naive port.** `str::parse::<f64>()` is all-or-nothing and returns `Err` for
any trailing character.

**Port.** `jsnum::parse_float` scans the longest valid numeric prefix and
returns `NaN` when there isn't one. Handles the JS whitespace set, `Infinity`,
and the case where a malformed exponent (`"1e"`) is simply not consumed.

*Verified: unit (`parse_float_takes_prefix`), fuzz.*

---

## D-006 — Number→String has exponential thresholds Rust does not

**Upstream.** JS switches to exponential notation at `|x| >= 1e21` or
`|x| < 1e-6`, and writes a `+` on positive exponents (`"1e+21"`).

**Naive port.** Rust's `{}` never emits exponential form; its `{:e}` always
does and omits the `+`.

**Port.** `jsnum::num_to_string` applies JS's thresholds and patches the sign.
This is load-bearing wherever a number reaches a string — `toRgbString`,
`toHslString`, the `"%"` suffixes — and inside **D-011**.

*Verified: unit (`num_to_string_thresholds`), fuzz.*

---

## D-007 — A literal `|` is an accepted CSS separator

**Upstream.** The matcher separators are written `[\s|\(]+` and `[,|\s]+`.
Inside a character class, `|` is a literal, not alternation. So:

```
tinycolor("rgb|255|0|0").toHexString()  // "#ff0000"
tinycolor("rgb|255|0|0").isValid()      // true
```

Almost certainly an escaping slip — but TinyColor's stated design goal is
"input is meant to be as permissive as possible", so it is not clearly a
defect, and it is not being reported upstream on that basis.

**Port.** Reproduced exactly in `parse::permissive_match`. The fuzzer's
generators emit pipe separators deliberately.

*Verified: unit (`pipe_separator_quirk_is_reproduced`), fuzz.*

---

## D-008 — `switch (max)` resolves channel ties in r, g, b order

**Upstream.** `rgbToHsl`/`rgbToHsv` select the hue formula with
`switch (max) { case r: ... case g: ... case b: }`. When two channels tie for
the maximum, the first matching case wins.

**Naive port.** A `match` on which channel *is* the max, written in whatever
order feels natural, or an `if max == g` checked first.

**Port.** `if max == r { … } else if max == g { … } else { … }`, preserving
source order. For `rgb(200, 200, 10)` this yields hue 60°, from the `r` branch.

*Verified: unit (`max_channel_tie_prefers_r`), fuzz.*

---

## D-009 — `toName()` returns `false`, and `toString()` falls back on falsy

**Upstream.** `toName()` returns the string `"transparent"`, a colour name, or
the boolean `false`. `toString()` ends with `return formattedString || this.toHexString()`,
so **both** `false` and `""` fall through to hex.

**Naive port.** Return `String`, using `""` for "no name" — which then silently
changes `toString("name")` behaviour, or return `Result` and propagate an error
that upstream never raises.

**Port.** `to_name() -> Option<String>`; the RPC layer maps `None` to JSON
`false`. `to_string()` treats both `None` and `Some("")` as falsy before
falling back to `to_hex_string(false)`.

*Verified: suite, fuzz.*

---

## D-010 — `>>` in `analogous` is an int32 coercion, not a halving

**Upstream.** `hsl.h = (hsl.h - ((part * results) >> 1) + 720) % 360`.
JS `>>` coerces its left operand through `ToInt32` first, so
`(30.5 * 6) >> 1` is `91`, not `91.5`.

**Naive port.** `(part * results) / 2.0`, which keeps the fraction and shifts
every analogous hue.

**Port.** `color::to_int32` implements the `ToInt32` abstract operation
(truncate, mod 2³², wrap to signed) and the result is then shifted as an `i32`.

*Verified: suite, fuzz.*

---

## D-011 — `parseInt(number)` in `bound01`: reproduced, and proven unobservable

**Upstream.** `bound01` contains `n = parseInt(n * max, 10) / 100`. The
argument is a *number*, so it is stringified before parsing. When the value is
small enough to stringify exponentially, `parseInt` reads only the mantissa:

```
(2.55e-7).toString()   // "2.55e-7"
parseInt(2.55e-7, 10)  // 2      <- not 0
```

This does fire. `bound01("0.000000001%", 255)` returns `0.0000784` where the
mathematically correct value is `~1e-11`.

**Initially recorded as a candidate upstream bug. It is not one.** The
constructor immediately applies `if (this._r < 1) this._r = Math.round(this._r)`,
and the defect's output is `0.02`, which rounds to `0` — the correct answer.
It cannot be pushed higher: `Number#toString` normalises exponential form to a
single leading digit (`255e-9` prints as `"2.55e-7"`), so `parseInt` returns at
most `9`, giving a channel of at most `0.09`, which still rounds to `0`.

**Port.** Reproduced exactly via `jsnum::parse_int_from_number` rather than
"fixed", because a port that silently corrects upstream is no longer
equivalent. Using `trunc()` here would be the natural Rust translation and
would be wrong.

*Verified: unit (`parse_int_from_number_reads_mantissa_only`); unobservability
confirmed by exhaustive reasoning over the mantissa range plus direct API
probing at magnitudes from 1e-9 to 1e-11.*

---

## D-012 — `Math.pow` is not bit-identical across libm, so luminance is tabulated

**fuzz / exhaustive.** The upstream `readability` test asserts exact float
equality:

```js
assertEquals(tinycolor.readability("#000", "#111"), 1.1121078324840545)
```

The port failed it by one ULP — and the failure depended on the *build target*:

| implementation | exact matches vs V8, over all 256 channel values |
|---|---|
| Rust std `powf` (macOS, Apple libm) | 207 / 256 |
| wasm32 `pow` (MUSL-derived) | 225 / 256 |
| V8 (fdlibm) | reference |

Neither matches. The same Rust source produced different luminance natively
and in WebAssembly.

**Port.** `getLuminance()` reads the sRGB linearisation from
`crates/core/src/luminance.rs`, a generated table of V8's exact f64 bit
patterns for all 256 integer channel values.

This is sound because the domain is genuinely finite: `getLuminance()` takes
its input from `toRgb()`, which rounds, so only integers 0–255 can reach the
`pow` call. `srgb_linear` returns `None` outside that domain and the caller
falls back to computing directly — unreachable in practice, but not a silent
assumption.

Regenerate with `node scripts/gen-luminance-table.mjs`.

**Side benefits.** Bit-exact on every target, no libm dependency, and a table
lookup instead of a transcendental call.

*Verified: exhaustive (all 256 inputs), suite, fuzz.*

---

## D-013 — Alpha preservation is inconsistent upstream, per function

**fuzz.** The first fuzz run reported ~34,000 divergences, nearly all of them
alpha being dropped by the modification methods.

The cause is that upstream uses two different construction forms, and which
one it uses is not consistent:

| form | alpha | used by |
|---|---|---|
| `tinycolor(hsl)` — the mutated `toHsl()` object, which carries `a` | preserved | `lighten`, `darken`, `saturate`, `desaturate`, `greyscale`, `spin`, `complement`, `analogous` |
| `tinycolor({h, s, l})` — a fresh object literal | **dropped** | `polyad`, `splitcomplement`, `monochromatic` |

`brighten` is a third case: it passes the `toRgb()` object through, which also
carries `a`, so alpha survives.

**Port.** Two constructors, `hsl_color_a` (carries alpha) and `hsl_color`
(does not), applied per function to match. This is not a defensible design and
would be worth raising upstream as an API-consistency issue, but it is
observable behaviour and the port matches it rather than tidying it.

*Verified: fuzz (divergences 33,897 → 14,626 on this fix alone).*

---

## D-014 — `convertToPercentage` compares with `ToNumber`, not `parseFloat`

**fuzz.** `convertToPercentage(n)` is `if (n <= 1) n = n * 100 + "%"`. The
`<=` operator coerces via the `ToNumber` abstract operation, which requires the
*entire* string to be numeric — unlike `parseFloat`, which takes a prefix:

```
Number("1%")      // NaN   -> NaN <= 1 is false -> value passes through
parseFloat("1%")  // 1     -> 1 <= 1 is true    -> rewritten to "100%"
```

**Naive port.** Reuse the `parse_float` already written for `bound01` — the
two look interchangeable and are not. Taking the wrong branch on `"1%"` turns
1% saturation into 100%.

**Port.** `jsnum::to_number` implements `ToNumber` (empty string → 0, hex/octal/
binary prefixes, exact `"Infinity"` spelling, whole-string numeric match), and
`convert_to_percentage` takes a `&Unit` rather than an `f64` so the original
value is still available at the comparison.

*Verified: fuzz.*

---

## D-015 — `polyad` is commented out upstream and excluded from fuzzing

**Upstream.** `tinycolor.prototype.polyad` is commented out (`mod.js:265`), and
its test is registered with `ignore: true` pending
[bgrins/TinyColor#254](https://github.com/bgrins/TinyColor/issues/254). The
internal `polyad()` function still exists and backs `triad`/`tetrad`.

**Port.** Implements `polyad` (the disabled test expects it, and it would be
needed the moment upstream re-enables the method), but the differential fuzzer
excludes it — the reference throws `"polyad is not a function"` for every
input, so there is nothing to compare against. Fuzzing it produced ~11,000
meaningless divergences before exclusion.

*Verified: suite (test is skipped, matching upstream).*

---

## D-016 — Values JSON cannot represent travel tagged across the RPC boundary

**fuzz.** `JSON.stringify(Infinity)` is the string `"null"`, and `-0`
stringifies to `"0"`. Untagged, `lighten(Infinity)` arrived at the port as
`lighten(undefined)` and defaulted to `10`.

This was a bug in the harness's own transport, not in the port — but it
presents identically to a porting bug, which is exactly why it is recorded.

**Port.** `shim/api.js` encodes non-finite numbers and negative zero as
`{"__num": "Infinity"}` etc.; `rpc::decode_tagged_num` reverses it. Results
travelling the other way use string sentinels revived by the shim.

*Verified: fuzz (divergences 3,258 → 0 on this fix).*

---

## D-018 — Handle-table lifetime: the port leaked until the wasm heap died

**fuzz.** The first 300-second run was clean for 4.2M comparisons and then
produced 7.3M divergences. The pattern — zero, then *everything* failing — is
the signature of state corruption rather than an input-dependent bug. The
actual error was:

```
port: "memory access out of bounds"
```

**Cause.** D-001 puts colours in a Rust-side `HashMap` keyed by handle. JS
garbage collection knows nothing about that table, so every colour ever
constructed stayed live forever. At roughly 4.6M handles the wasm linear
memory was exhausted and every subsequent call trapped.

This is a defect the port introduced, not one inherited from upstream: the
original has no handle table because JS objects are simply collected.

**Fix, in two parts.**

1. `shim/api.js` registers every wrapper with a `FinalizationRegistry` that
   issues an RPC `free` when the JS object is collected. This is the correct
   general fix and makes the library safe for ordinary consumers.
2. `FinalizationRegistry` reclamation is tied to GC timing, and the fuzz loop
   allocates far faster than the collector runs, so the harness additionally
   calls an explicit `__reset` between batches. No colour survives a case, so
   this is sound.

Peak live handles went from unbounded to **13,200** — one batch — and the
harness now reports that figure in `fuzz/logs/latest.json` so a regression
would be visible rather than silent.

**Why this matters beyond the leak.** Short fuzz runs reported zero
divergences and were not wrong; they were just too short to reach the failure.
A 60-second run — the minimum the event asks for — would not have found this.

*Verified: fuzz (300s run, peak handles bounded), suite.*

---

## D-022 — `analogous` and `monochromatic` loop unboundedly upstream; the port does not

**fuzz-adjacent.** Found while auditing argument paths neither harness reached,
then reproduced directly.

**Upstream.** Both functions decrement a counter and test it for truthiness:

```js
for (hsl.h = (hsl.h - ((part * results) >> 1) + 720) % 360; --results; ) { … }  // analogous
while (results--) { … }                                                        // monochromatic
```

A counter that never lands exactly on `0` never terminates. From `results = -1`
the sequence is `-2, -3, …`; from `1.5` it is `0.5, -0.5, -1.5, …`. Each
iteration pushes a colour, so the process exhausts its heap:

```
$ node --max-old-space-size=256 -e "require('./upstream/tinycolor.reference.js')('red').analogous(-1)"
FATAL ERROR: Reached heap limit Allocation failed
$ echo $?
134
```

Confirmed for `analogous` and `monochromatic` at `-1`, `1.5` and `0.5`.
`0`, `2` and `null` terminate normally.

**Port.** Returns a finite list — `analogous(-1)` gives 1 entry, `1.5` gives 2 —
because the loop is `while results > 0.0`, which is the natural Rust spelling and
cannot run away.

**This is the one deliberate behavioural divergence in the port.** Everywhere
else, reproducing upstream exactly is the goal, including reproducing its
oddities (D-007, D-011). Here it is not, for two reasons. Reproducing an
unbounded allocation loop would mean shipping a denial-of-service on purpose.
And there is no observable behaviour to match: the reference does not *return*
anything for these inputs, it dies. Equivalence with a crash is not a
meaningful target.

It also cannot be differentially fuzzed, for the same reason `polyad` cannot
(D-015) — the reference never returns. Excluded from the battery deliberately,
not by omission.

**Upstream already guards the same hazard elsewhere.** `polyad()` validates its
count (`if (isNaN(number) || number <= 0) throw`), so the risk is recognised in
one of the three combination functions and not the other two. Written up as a
filable report in `UPSTREAM-BUG-REPORT.md`; not filed, because filing should
happen under the team's own GitHub identity rather than be fabricated here.

*Verified: reproduced directly against upstream at the pinned commit, exit 134,
and against the port, exit 0.*

---

## D-017 — The upstream test suite is executed unmodified, via shim placement

**Decision.** `tests/original/test.js` is byte-for-byte identical to upstream's
`npm/cjs/test.js` (sha256 in `tests/HASHES.txt`; verify with
`./scripts/verify-hashes.sh`).

Its only entry point to the library is `require("./tinycolor.js")` on line 5.
Rather than edit that line or translate the assertions into `#[test]` blocks,
the port's shim is placed at exactly that path, so the test file resolves to
the Rust port with zero bytes changed.

`tests/run-all.mjs` reports a pass *rate* without touching the file either: it
pre-loads the same `@deno/shim-deno-test` module the tests import and wraps
each registered function in a try/catch, because upstream's CJS runner
rethrows on the first failure.

**Consequence.** 45/45 tests, 432 assertion call sites, against the genuine upstream
suite.

*Verified: `diff` against upstream is empty; hash manifest verifies clean.*

---

## D-019 — Two performance defects the port introduced, found by review

Both are places where idiomatic-looking Rust did strictly more work than the
JavaScript it replaced. Neither changes output; both are the port's own fault.

**(a) The matcher chain ran each regex up to four times.**

**Upstream.** `stringInputToObject` runs a matcher once and reads the capture
groups off the single array it returns.

**Naive port.** The `regex` crate separates "does it match" from "what did it
capture", so the natural translation is `is_match()` to select the branch and
then `captures()` to read the groups — and, since each group read looks
self-contained, a fresh `captures()` per group.

For a successful `rgb()` parse that is **four** executions of the rgb regex
(one `is_match`, three `captures`), plus two executions of every matcher
earlier in the chain.

**Port.** One `captures()` per matcher; `parse::permissive_match` returns the
`Captures` value and the groups are read from it.

**(b) Name lookups were linear scans, running in opposite directions.**

**Upstream.** `names[color]` and `hexNames[hex]` — plain object indexing, O(1).

**Naive port.** A generated slice plus `.iter().find(...)`. It reads cleanly
and it is a 149-entry scan per lookup. Worse, the two scans ran in *opposite*
directions — `parse.rs` front-to-back via `.find()`, `color.rs` back-to-front
via `.next_back()`, the latter forced by D-004's last-wins requirement — so
lookup cost varied with alphabetical position, in opposite ways depending on
which direction you were converting.

**Port.** `names::NAME_MAP` and `names::HEX_NAME_MAP`, `once_cell::sync::Lazy`
`HashMap`s. `HEX_NAME_MAP` inserts `HEX_NAMES` **in source order** so later
entries overwrite earlier ones: D-004's last-wins behaviour is a property the
map has to preserve, not a side effect of the old scan direction. A test
asserts the maps are indistinguishable from the scans they replace.

**Measured effect.** Directional only — the measuring machine was throttled and
these sit inside the run-to-run spread recorded in `bench/METHODOLOGY.md`:

| row | change |
|---|---|
| round-trip | −26.9% |
| to-rgb-string | −6.3% |
| mix | −4.7% |
| parse-hex | −4.3% |

round-trip gains most because it parses an `hsl()` string — precisely the path
that ran its regex four times.

Behaviour is unchanged: 5,557,750 fuzz comparisons, 0 divergences, seed 7.

*Verified: unit (`maps_agree_with_linear_scan`), suite, fuzz (5,557,750
comparisons, seed 7).*

---

## D-020 — Two transports, because the fast artifact and the proven artifact have to be the same artifact

**Decision.** `TINYCOLOR_TRANSPORT` selects `wasm` (default, in-process
wasm-bindgen) or `native` (the release binary over a stdio JSON-RPC pipe). Both
drive the same `rpc::dispatch` in `crates/core`, and both run the byte-identical
upstream suite.

**Why.** The benchmark says the two builds are not interchangeable:

| | startup p50 | max RSS |
|---|---|---|
| node + upstream JS | 18.4 ms | 34.3 MB |
| node + port (wasm) | 71.1 ms | 86.3 MB |
| node + port (native IPC) | 22.7 ms | 40.2 MB |
| native binary | 3.2 ms | 4.4 MB |

The native binary starts 5.8× faster than Node+upstream and uses 7.8× less
memory. The wasm build is *worse* than upstream on both — instantiating a
1.1 MB wasm module costs more than booting V8. Correctness evidence had only
ever been collected against wasm. Quoting the native numbers under the wasm
build's 45/45 and its fuzz record would have been exactly the sleight of hand
the methodology review warns about: two artifacts, one set of claims.

D-012 is the sharper version of the same point — the two builds genuinely
disagreed on floating point until the luminance table removed libm from the
picture. "It is the same Rust source" is not an argument.

**Consequence.** Both transports pass 45/45 on the unmodified suite, and both
survive differential fuzzing against upstream-on-V8 with zero divergences
(wasm: 689,345 cases; native: 74,303 cases).

**Implementation note.** `shim/api.js` requires `dispatch(json)` to be
*synchronous* — upstream's tests are synchronous, and object identity
(`tinycolor(x) === x`) depends on the call completing in place — so the native
transport cannot use Node's stream API. Node does not expose `.fd` on pipe
streams, so the shim takes the descriptor from the underlying libuv handle and
does its I/O with `fs.readSync`/`fs.writeSync`:

```js
const inFd  = child.stdin._handle.fd;
const outFd = child.stdout._handle.fd;
```

*Verified: suite (45/45 through both transports), fuzz (both transports).*

---

## D-021 — The native transport hung every process that used it

**Symptom.** A program that used the library over the native transport finished
its work and then never exited. About three hours of a benchmark run sat at
0.0% CPU before this was noticed.

**Cause.** The spawned child and its two pipes are live libuv handles, and an
active handle keeps Node's event loop alive. Nothing was pending; the process
simply had no permission to leave.

**Why the harnesses could not see it.** Both `tests/run-all.mjs` and
`fuzz/harness.mjs` call `process.exit()` explicitly, which tears the process
down regardless of outstanding handles. 45/45 tests and 3,343,635 fuzz
comparisons therefore ran green over a transport that hung. The defect was
reachable only by a plain consumer that returns from `main` and expects to
exit — the one shape neither harness has.

**Fix.** `child.unref()`, `child.stdin.unref()`, `child.stdout.unref()`. All
I/O is on raw descriptors (D-020), so nothing needs those handles referenced;
unreferencing them only removes them from the loop's liveness count.

**The pattern D-016 and D-018 were already pointing at.** Every genuine defect
this project has introduced has been in the JS↔Rust bridge — tagged numbers,
handle lifetime, process lifetime — and none has been in the ported colour
logic. The colour maths carries 15.4M differential comparisons behind it; the
bridge is where the untested surface actually lives.

*Verified: by observation — a consumer that returns normally now exits; suite
and fuzz, by construction, cannot see this class of defect.*

---

## Summary

| ID | Found by | Class |
|---|---|---|
| D-001 | design | architecture |
| D-002 | reading | JS type semantics |
| D-003 | reading | numeric semantics |
| D-004 | reading | data-structure semantics |
| D-005 | reading | numeric semantics |
| D-006 | reading | string coercion |
| D-007 | reading | upstream quirk (not reported — permissive by design) |
| D-008 | reading | evaluation order |
| D-009 | reading | falsy semantics |
| D-010 | reading | operator coercion |
| D-011 | reading | upstream quirk (investigated, proven unobservable) |
| D-012 | **suite** | cross-target float determinism |
| D-013 | **fuzz** | upstream inconsistency |
| D-014 | **fuzz** | abstract-operation confusion |
| D-015 | **fuzz** | upstream dead code |
| D-016 | **fuzz** | harness transport |
| D-017 | design | verification strategy |
| D-018 | **fuzz** | port-introduced resource leak |
| D-022 | audit | upstream unbounded loop; deliberate divergence |
| D-019 | review | port-introduced performance defect |
| D-020 | design | transport parity / benchmark honesty |
| D-021 | benchmark run | port-introduced process-lifetime leak |

No `unsafe` appears in the port. This is enforced at compile time by
`unsafe_code = "forbid"` in both crate manifests, so it is a build failure
rather than a claim.

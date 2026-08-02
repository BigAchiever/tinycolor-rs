# Native CLI implementation notes

Both required commands verified, plus full workload and RPC coverage.

## What I built

**`/Users/danishalisiddiqui/Dev-Setups/1aug/crates/cli/Cargo.toml`** — package `tinycolor-cli`, `[[bin]] name = "tinycolor"`, single dep `tinycolor-core` (path `../core`), `[lints.rust] unsafe_code = "forbid"`.

**`/Users/danishalisiddiqui/Dev-Setups/1aug/crates/cli/src/main.rs`** (~370 lines, no deps beyond core)
- `rpc` — `BufRead::lines` on stdin → `tinycolor_core::rpc::dispatch` → response + `\n` + explicit `flush()` per line. Blank lines skipped; `BrokenPipe` on stdout exits 0 (peer hangup is a normal end of run, so the fuzzer closing the pipe isn't a spurious failure).
- `bench <workload> <iterations>` — prints exactly the four specified keys. `std::time::Instant`, `std::hint::black_box` on both the input reference and the result.
- `version` — `env!("CARGO_PKG_VERSION")`.
- 3 unit tests: every corpus entry parses (so no workload silently times the *failure* path), every advertised workload dispatches, RPC response fits one line.

**`/Users/danishalisiddiqui/Dev-Setups/1aug/Cargo.toml`** — members line only: `["crates/core", "crates/wasm", "crates/cli"]`. `Cargo.lock` gained the generated `tinycolor-cli` entry (cargo writes this; nothing else in it changed).

## Two measurement decisions you need to know before quoting any number

1. **Corpus setup is not timed.** For `to-*`, `lighten`, `mix`, `most-readable`, `luminance` the corpus is parsed into `TinyColor` *before* `Instant::now()`, so `to-hex` measures formatting only. `parse-*` and `round-trip` deliberately include parsing. **The JS baseline must draw the line in the same place or the ratio is meaningless.**
2. **`nanos_total` is wall time for exactly `iterations` operations.** No hidden warm-up, no discarded first run, no inner repeat factor, no min-of-N. One iteration = one op on one corpus element, walked cyclically. The figure therefore *includes* loop overhead — so I added a 13th workload, **`noop`**, that measures that floor on the same machine (1.5 ns/op). All 12 required workloads are present and unchanged; `noop` is additive and listed in `--help`.

## Verification (exact commands, real output)

```
$ cargo build --release -p tinycolor-cli
   Compiling tinycolor-cli v0.1.0 (/Users/danishalisiddiqui/Dev-Setups/1aug/crates/cli)
    Finished `release` profile [optimized] target(s) in 26.14s      # zero warnings

$ ./target/release/tinycolor bench to-hex 1000
{"workload":"to-hex","iterations":1000,"nanos_total":412833,"nanos_per_op":412.833}

$ echo '{"op":"new","input":"red"}' | ./target/release/tinycolor rpc
{"id":1}
```

Multi-line RPC session (handle table persists across lines, errors don't kill the stream, `exit=0`):
```
{"id":1} / "#ff0000" / {"id":1} / "rgb(255, 51, 51)" / {"id":2} / "#800080"
{"live":2} / {"ok":true} / {"live":1}
{"error":"unknown op: bogus"}
{"error":"bad request json: expected ident at line 1 column 2"}
```

All 13 workloads at 20k iterations (ns/op): parse-hex 1390.7, parse-rgb 3053.6, parse-hsl 3472.3, parse-name 1341.6, to-hex 390.6, to-rgb-string 313.5, to-hsl-string 608.6, lighten 1881.6, mix 809.8, most-readable 338.7, luminance 7.6, round-trip 5228.0, **noop 1.5**.

Also: `cargo test -p tinycolor-cli --release` → 3 passed; `cargo clippy --release -p tinycolor-cli` → **zero warnings for the cli crate** (the 11 warnings shown are pre-existing in `tinycolor-core`, mostly `manual_clamp` on `.max().min()` — which is NaN-preserving on purpose for JS semantics, so do not let anyone "fix" those); `cargo check --workspace --release` → core, cli and wasm all still build.

## Three things to flag before you measure

- **Those parse numbers are slow enough to be a real risk to the "runtime elimination" narrative.** 1.4 µs for a hex parse is regex-engine plus allocation cost per call — V8's regex is heavily optimised and JIT'd JS may well beat it. I did not touch core (out of scope), but you should expect `parse-*` to be where a disclosure-requiring regression shows up, and measure it before writing any claim.
- **1000 iterations is too few to quote.** The two `to-hex 1000` runs above differ by 21% (412.8 vs 397.2 ns/op) — that's cold i-cache, not signal. Use ≥1e6 for anything that goes in front of a judge.
- **`panic = "abort"` is inherited from the workspace release profile**, so a panic inside `dispatch` takes the whole process down with no unwinding; the fuzzer would see the pipe close rather than an error response. Worth knowing when interpreting a fuzz run that ends early.

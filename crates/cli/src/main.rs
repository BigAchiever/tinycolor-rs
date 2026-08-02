//! Native transport for the TinyColor port.
//!
//! Three subcommands, one job each:
//!
//!   `rpc`      — the JSON-RPC protocol from `tinycolor_core::rpc` spoken over
//!                stdio, one request per line, one response per line. Same
//!                protocol the wasm crate exposes to Node, so the differential
//!                fuzzer can drive either transport unchanged.
//!   `bench`    — a fixed workload run N times, timed with `Instant`.
//!   `version`  — the crate version.
//!
//! # Benchmark honesty
//!
//! Two things about `bench` are deliberate and must be understood before any
//! number it prints is compared against the JS baseline:
//!
//! 1. **Corpus setup is not timed.** For the conversion, modification and
//!    combination workloads the corpus is parsed into `TinyColor` values
//!    *before* `Instant::now()` is taken, so `to-hex` measures formatting and
//!    nothing else. The `parse-*` and `round-trip` workloads deliberately
//!    include parsing, because parsing is what they exist to measure. The JS
//!    baseline must draw the line in the same place or the comparison is
//!    meaningless.
//!
//! 2. **`nanos_total` is wall time for exactly `iterations` operations.**
//!    There is no hidden warm-up, no discarded first run, no inner repeat
//!    factor, and no minimum-of-N selection. One iteration is one operation on
//!    one corpus element; the corpus is walked cyclically. Consequently the
//!    reported figure *includes* loop and dispatch overhead. The `noop`
//!    workload measures that floor on the same machine so it can be subtracted
//!    or, better, quoted alongside.
//!
//! Corpora are fixed literals, never random, so two runs of the same command
//! do the same work.

use std::hint::black_box;
use std::io::{self, BufRead, Write};
use std::time::{Duration, Instant};

use tinycolor_core::color::{self as ops, TinyColor};
use tinycolor_core::rpc;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let code = match args.first().map(String::as_str) {
        Some("rpc") => run_rpc(),
        Some("bench") => run_bench(args.get(1), args.get(2)),
        Some("version") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("help" | "-h" | "--help") => {
            usage(&mut io::stdout());
            0
        }
        Some(other) => {
            eprintln!("tinycolor: unknown subcommand: {other}");
            usage(&mut io::stderr());
            2
        }
        None => {
            usage(&mut io::stderr());
            2
        }
    };
    std::process::exit(code);
}

fn usage(w: &mut impl Write) {
    let _ = writeln!(
        w,
        "tinycolor {}\n\
         \n\
         USAGE:\n    \
             tinycolor rpc                      JSON-RPC over stdio, line delimited\n    \
             tinycolor bench <workload> <iters> time a workload, print JSON\n    \
             tinycolor version                  print the crate version\n\
         \n\
         WORKLOADS:\n    \
             {}\n\
         \n\
         `noop` measures loop overhead only; quote it next to the others.",
        env!("CARGO_PKG_VERSION"),
        WORKLOADS.join("\n    ")
    );
}

// ---- rpc ---------------------------------------------------------------

fn run_rpc() -> i32 {
    let stdin = io::stdin();
    let mut out = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = match line {
            Ok(l) => l,
            Err(e) => {
                eprintln!("tinycolor rpc: stdin: {e}");
                return 1;
            }
        };
        let request = line.trim();
        if request.is_empty() {
            continue;
        }
        // `dispatch` returns `serde_json` output, which never contains a raw
        // newline, so one response really is one line.
        let response = rpc::dispatch(request);
        if let Err(e) = write_line(&mut out, &response) {
            // The peer hanging up is a normal end of run, not a failure.
            if e.kind() == io::ErrorKind::BrokenPipe {
                return 0;
            }
            eprintln!("tinycolor rpc: stdout: {e}");
            return 1;
        }
    }
    0
}

fn write_line(out: &mut impl Write, s: &str) -> io::Result<()> {
    out.write_all(s.as_bytes())?;
    out.write_all(b"\n")?;
    out.flush()
}

// ---- bench -------------------------------------------------------------

const WORKLOADS: &[&str] = &[
    "parse-hex",
    "parse-rgb",
    "parse-hsl",
    "parse-name",
    "to-hex",
    "to-rgb-string",
    "to-hsl-string",
    "lighten",
    "mix",
    "most-readable",
    "luminance",
    "round-trip",
    "noop",
];

fn run_bench(workload: Option<&String>, iters: Option<&String>) -> i32 {
    let Some(workload) = workload else {
        eprintln!("tinycolor bench: missing <workload>");
        usage(&mut io::stderr());
        return 2;
    };
    let Some(iters) = iters else {
        eprintln!("tinycolor bench: missing <iterations>");
        usage(&mut io::stderr());
        return 2;
    };
    let iterations: u64 = match iters.parse() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("tinycolor bench: <iterations> must be a non-negative integer, got {iters:?}");
            return 2;
        }
    };

    let Some(elapsed) = dispatch_workload(workload, iterations) else {
        eprintln!("tinycolor bench: unknown workload: {workload}");
        eprintln!("known workloads: {}", WORKLOADS.join(", "));
        return 2;
    };

    let nanos_total = elapsed.as_nanos();
    let nanos_per_op = if iterations == 0 {
        0.0
    } else {
        nanos_total as f64 / iterations as f64
    };
    println!(
        "{{\"workload\":\"{workload}\",\"iterations\":{iterations},\"nanos_total\":{nanos_total},\"nanos_per_op\":{nanos_per_op:.3}}}"
    );
    0
}

/// One iteration = one `op` applied to one corpus element, walking the corpus
/// cyclically. `black_box` on both the input reference and the result keeps
/// the optimiser from hoisting the call out of the loop or deleting it.
fn timed<T, R>(corpus: &[T], iterations: u64, mut op: impl FnMut(&T) -> R) -> Duration {
    if corpus.is_empty() || iterations == 0 {
        return Duration::ZERO;
    }
    let mut idx = 0usize;
    let start = Instant::now();
    for _ in 0..iterations {
        black_box(op(black_box(&corpus[idx])));
        idx += 1;
        if idx == corpus.len() {
            idx = 0;
        }
    }
    start.elapsed()
}

fn dispatch_workload(workload: &str, iterations: u64) -> Option<Duration> {
    Some(match workload {
        // -- parsing: the parse is the thing being measured ---------------
        "parse-hex" => timed(HEX_INPUTS, iterations, |s| TinyColor::from_str(s)),
        "parse-rgb" => timed(RGB_INPUTS, iterations, |s| TinyColor::from_str(s)),
        "parse-hsl" => timed(HSL_INPUTS, iterations, |s| TinyColor::from_str(s)),
        "parse-name" => timed(NAME_INPUTS, iterations, |s| TinyColor::from_str(s)),

        // -- formatting: corpus is parsed first, outside the timed region --
        "to-hex" => {
            let corpus = parsed_corpus();
            timed(&corpus, iterations, |c| c.to_hex_string(false))
        }
        "to-rgb-string" => {
            let corpus = parsed_corpus();
            timed(&corpus, iterations, TinyColor::to_rgb_string)
        }
        "to-hsl-string" => {
            let corpus = parsed_corpus();
            timed(&corpus, iterations, TinyColor::to_hsl_string)
        }

        // -- modification / combination -----------------------------------
        "lighten" => {
            let corpus = parsed_corpus();
            timed(&corpus, iterations, |c| ops::lighten(c, Some(10.0)))
        }
        "mix" => {
            let corpus = parsed_corpus();
            // Fixed pairing: element i with element i+1 (wrapping), so the
            // pair list is a deterministic function of the corpus.
            let pairs: Vec<(TinyColor, TinyColor)> = (0..corpus.len())
                .map(|i| (corpus[i].clone(), corpus[(i + 1) % corpus.len()].clone()))
                .collect();
            timed(&pairs, iterations, |(a, b)| ops::mix(a, b, Some(50.0)))
        }
        "most-readable" => {
            let corpus = parsed_corpus();
            let candidates: Vec<TinyColor> =
                READABILITY_CANDIDATES.iter().map(|s| TinyColor::from_str(s)).collect();
            timed(&corpus, iterations, |base| {
                most_readable(base, &candidates, "AA", "small", true)
            })
        }
        "luminance" => {
            let corpus = parsed_corpus();
            timed(&corpus, iterations, TinyColor::get_luminance)
        }

        // -- full cycle: string -> colour -> string -> colour -> string ----
        "round-trip" => timed(ROUND_TRIP_INPUTS, iterations, |s| {
            let c = TinyColor::from_str(s);
            let out = c.to_string(None);
            TinyColor::from_str(&out).to_hex_string(false)
        }),

        // -- the floor: loop, index, black_box, nothing else ---------------
        "noop" => timed(HEX_INPUTS, iterations, |s| s.len()),

        _ => return None,
    })
}

/// `tinycolor.mostReadable` — kept in step with the same logic in
/// `tinycolor_core::rpc`, which is where the tested implementation lives.
fn most_readable(
    base: &TinyColor,
    list: &[TinyColor],
    level: &str,
    size: &str,
    include_fallback: bool,
) -> Option<TinyColor> {
    let mut best: Option<&TinyColor> = None;
    let mut best_score = 0.0f64;
    for c in list {
        let r = ops::readability(base, c);
        if r > best_score {
            best_score = r;
            best = Some(c);
        }
    }
    let candidate = best.cloned().unwrap_or_else(|| TinyColor::from_str(""));
    if ops::is_readable(base, &candidate, level, size) || !include_fallback {
        return best.cloned();
    }
    let white = TinyColor::from_str("#fff");
    let black = TinyColor::from_str("#000");
    Some(if ops::readability(base, &white) > ops::readability(base, &black) {
        white
    } else {
        black
    })
}

// ---- corpora -----------------------------------------------------------
//
// Fixed literals, chosen to exercise every branch a real caller would hit:
// short and long hex, with and without `#`, integer and percentage channels,
// opaque and translucent, achromatic and saturated, named and unnamed.

const HEX_INPUTS: &[&str] = &[
    "#ff0000",
    "#0f0",
    "#1a2b3c",
    "abcdef",
    "#FFF",
    "#00000080",
    "#1234",
    "#c0ffee",
    "#000000",
    "#8a2be2",
    "336699",
    "#fedcba",
];

const RGB_INPUTS: &[&str] = &[
    "rgb(255, 0, 0)",
    "rgb 255 0 0",
    "rgba(0, 128, 255, 0.5)",
    "rgb(50%, 25%, 75%)",
    "rgba(10, 20, 30, 1)",
    "rgb(0,0,0)",
    "rgb(1.5, 200.4, 99)",
    "rgba(255, 255, 255, 0.13)",
];

const HSL_INPUTS: &[&str] = &[
    "hsl(0, 100%, 50%)",
    "hsl 210 50% 25%",
    "hsla(120, 33%, 75%, 0.4)",
    "hsl(359, 1%, 99%)",
    "hsla(0, 0%, 0%, 1)",
    "hsl(180, 100%, 50%)",
    "hsl(-90, 40%, 60%)",
    "hsla(270, 80%, 20%, 0.75)",
];

const NAME_INPUTS: &[&str] = &[
    "red",
    "aliceblue",
    "rebeccapurple",
    "cornflowerblue",
    "darkslategray",
    "seagreen",
    "tomato",
    "transparent",
    "black",
    "white",
];

/// One string per input family, so the derived-corpus workloads are exercised
/// over colours that arrived through every parse path — the resulting
/// `TinyColor`s carry different `format` values, which `to_string` branches on.
const CORPUS_INPUTS: &[&str] = &[
    "#ff0000",
    "#0f0",
    "#1a2b3c",
    "#00000080",
    "rgb(255, 0, 0)",
    "rgba(0, 128, 255, 0.5)",
    "rgb(50%, 25%, 75%)",
    "hsl(0, 100%, 50%)",
    "hsla(120, 33%, 75%, 0.4)",
    "hsv(210, 50%, 25%)",
    "red",
    "rebeccapurple",
    "cornflowerblue",
    "#c0ffee",
    "#808080",
    "#000000",
];

const ROUND_TRIP_INPUTS: &[&str] = CORPUS_INPUTS;

/// Foreground candidates for `most-readable`; the shape a UI actually asks
/// about — a couple of neutrals and a couple of brand-ish colours.
const READABILITY_CANDIDATES: &[&str] =
    &["#fff", "#000", "#808080", "#1a2b3c", "rebeccapurple", "tomato"];

fn parsed_corpus() -> Vec<TinyColor> {
    CORPUS_INPUTS.iter().map(|s| TinyColor::from_str(s)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_corpus_entry_parses() {
        for s in HEX_INPUTS
            .iter()
            .chain(RGB_INPUTS)
            .chain(HSL_INPUTS)
            .chain(NAME_INPUTS)
            .chain(CORPUS_INPUTS)
            .chain(READABILITY_CANDIDATES)
        {
            assert!(
                TinyColor::from_str(s).is_valid(),
                "corpus entry does not parse: {s:?} — a workload would be timing the failure path"
            );
        }
    }

    #[test]
    fn every_workload_is_dispatchable() {
        for w in WORKLOADS {
            assert!(
                dispatch_workload(w, 1).is_some(),
                "workload advertised but not implemented: {w}"
            );
        }
    }

    #[test]
    fn rpc_protocol_round_trips() {
        let r = rpc::dispatch(r#"{"op":"new","input":"red"}"#);
        assert!(r.contains("\"id\""), "unexpected response: {r}");
        assert!(!r.contains('\n'), "response must fit one line: {r}");
    }
}

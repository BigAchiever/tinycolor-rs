# YouTube title and description

Two audiences read this: the judges, who were handed the link and want to
confirm the submission is what it claims, and everyone else, who needs a reason
to click. The title below is written for the first and survives the second.

---

## Title  (recommended)

```
TinyColor ported to Rust — passing the original test suite, unmodified
```

69 characters. Truncates cleanly. States the claim that the whole submission
rests on, and it is a claim most ports cannot make, so it does the work of a
hook without reaching for one.

### Alternates

**If you want the hook instead of the claim** — better for sharing, slightly
worse for a judge scanning a list:

```
The same Rust gave two different answers — porting TinyColor to Rust
```

**If the organisers want submissions identifiable at a glance:**

```
Port Mortem Track F — TinyColor (JavaScript) to Rust, verified
```

Pick one and use it in the submission form too, so the entry and the video
match.

---

## Description

Everything above the fold (first ~150 characters) shows in search and in the
collapsed view. That is the two-line summary. Chapters go next because a judge
with seven minutes will use them.

```
A 1,187-line JavaScript colour library, ported to Rust — and made to pass its
own original test suite with zero bytes changed.

Repo, evidence and decision log: https://github.com/BigAchiever/tinycolor-rs

────────────────────────────────
CHAPTERS
0:00  The bug that was in neither codebase
0:25  The upstream test file, and line 5
0:50  45/45 on the unmodified suite
1:15  Same suite, native binary
1:35  Two objections, answered
2:05  Differential fuzzing
2:35  Math.pow is not bit-identical across libm
3:20  Upstream's own demo page, running on Rust
3:40  The regression, stated plainly
4:10  Where the win actually is
4:28  The decision log

────────────────────────────────
WHAT IS VERIFIED

• 45/45 on tests/original/test.js — upstream's file, sha256-pinned, 0 bytes
  changed. Same result through a WebAssembly module and a native binary.
• 689,345 generated inputs x 45 operations = 31,020,525 comparisons against
  the original running on V8, compared at exact IEEE-754 bit equality rather
  than an epsilon. Zero divergences.
• Zero unsafe, in all three crates — enforced by unsafe_code = "forbid", so
  it is a compile error rather than a claim.
• 22 documented behavioural divergences, each with what upstream does, what a
  naive port produces, and how the choice was verified.

Everything reproduces from a clean checkout with `make verify`, or in a
container with `docker build . && docker run`.

────────────────────────────────
ON PERFORMANCE — READ THIS

The port is slower. Five to ten times slower per colour operation than the
JavaScript, and the shipped WebAssembly build is worse than the original on
startup and memory too.

Peeling the layers apart, the ported Rust runs at 1.28x the original. The
shipped artifact runs at 8.11x. Ninety-six percent of that gap is the
JSON-RPC bridge — one boundary instead of forty-five FFI signatures, which is
the only reason the unmodified test suite can drive the port at all.

The genuine win is process startup: 3.2 ms against 18.4 ms, in 4.4 MB against
34.3 MB.

Method, environment gates and every confound that could not be eliminated:
bench/METHODOLOGY.md

────────────────────────────────
FOUND ALONG THE WAY

An unbounded loop in upstream's analogous() and monochromatic() — a negative
or fractional count never terminates and exhausts the heap. Reported:
https://github.com/bgrins/TinyColor/issues/280

Four of the documented divergences are bugs this port introduced, not
upstream's. They are in the log because a decision log that only records other
people's mistakes isn't one.

────────────────────────────────
Upstream TinyColor by Brian Grinstead, MIT:
https://github.com/bgrins/TinyColor
Pinned at b49018c9f2dbca313d80d7a4dad25e26143cfe01

Built for Port Mortem 2026, Track F (JavaScript to Rust).
```

---

## Settings

- **Visibility: Unlisted**, not private. Judges must be able to open the link
  without being individually granted access, and a private video will read as
  a broken submission. Unlisted keeps it off your channel until you want it
  there.
- **Category:** Science & Technology.
- **Do not** enable "made for kids" — it disables the comments and the
  chapter UI.
- **Thumbnail:** a frame of the terminal showing `45/45 tests passing`. If you
  want a custom one, white monospace `45/45` on the dark terminal, nothing
  else. Avoid a face or an arrow; this is being watched by a judge, not sold
  to a feed.
- **Check the chapters render** after upload. YouTube only accepts them if the
  first is `0:00` and each is at least ten seconds. The list above satisfies
  both.

## If the submission form wants a shorter blurb

```
TinyColor (1,187 lines of JavaScript) ported to Rust and verified against
upstream's own test suite, byte-for-byte unmodified: 45/45 through both a wasm
module and a native binary, plus 31 million differential comparisons at exact
float-bit equality with zero divergences. Zero unsafe, compiler-enforced. 22
documented divergences. The port is slower per operation and the write-up says
so, with the layer-by-layer measurement showing 96% of the gap is the bridge.
```

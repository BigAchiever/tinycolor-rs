# What to say — teleprompter script

Just the words, in order. Stage directions in brackets. `DEMO-SCRIPT.md` has
the full shot list; this is what comes out of your mouth.

**Read it out loud once before recording.** Anything you stumble on, change —
it's your voice, not mine. Written prose and spoken prose are different
languages, and this is written for the ear: short sentences, numbers said the
way people say them, no clause stacking.

596 spoken words — about 4:07 at a comfortable pace, inside a 5:00 video. The
remaining ~53 seconds is terminal output landing. **Let it land.** Silence while
a test suite prints is not dead air, it's evidence.

---

## 0:00 — Cold open  [black, or terminal already cleared]

> One assertion in this suite failed against my port. Off by a single bit.
>
> The bug wasn't in my Rust. It wasn't in the JavaScript either.
>
> I'll come back to that.

*[~8 seconds. Then straight into the terminal. No title card, no "hi, I'm…".]*

---

## 0:08 — Segment 1: the originals are untouched

```bash
./scripts/verify-hashes.sh
```

> This is TinyColor's own test suite, hash-pinned at the kickoff commit. Zero
> bytes changed.
>
> That matters for what happens next.

*[Four OKs land. Say nothing over them. Don't explain the manifest.]*

---

## 0:25 — Segment 2: forty-five out of forty-five

```bash
node tests/run-all.mjs
```

> That's the file we just hashed — running against a Rust port.
>
> The test file doesn't know. We never edited it. The shim sits at the `require`
> path upstream already uses, so the suite loads Rust and runs unchanged.

*[Flash `tests/original/tinycolor.js` for three seconds while you say the last
sentence. Don't read it aloud. Back to the terminal.]*

---

## 1:00 — Segment 3: not a WebAssembly trick

```bash
TINYCOLOR_TRANSPORT=native node tests/run-all.mjs
```

> Same suite. Same forty-five.
>
> This time it isn't WebAssembly — it's a native Rust binary on the other end of
> a pipe. Two independent transports, one core.
>
> The correctness lives in the port, not the plumbing.

---

## 1:25 — Segment 4: the differential fuzzer

```bash
node fuzz/harness.mjs --seconds 20 --seed 1
```

> A fixed suite is table stakes. So: a differential fuzzer.
>
> Random colours — hex, rgb, hsl, malformed junk — into the original library on
> V8, and into the port. Comparing bit for bit. No epsilon. Floats match
> exactly, or it's a divergence.

*[Let the counter run. Then open `fuzz/logs/run-wasm-300s-seed1.json`.]*

> Five minutes. Thirty-one million comparisons. Zero divergences.
>
> And the same on the native transport.

---

## 2:05 — Segment 5: the payoff for the cold open

*[This is the segment people will remember. Slow down.]*

> So — that failing assertion.
>
> Relative luminance. My port was off by one unit in the last place. The
> smallest disagreement two doubles can have.
>
> I assumed a rounding bug. It wasn't. The answer changed depending on what I
> built for. Native Rust gave one number. The same source as WebAssembly gave
> another.

*[Show the D-012 table in `DECISIONS.md`.]*

> `Math.pow` is not bit-identical across libm implementations. Across all two
> hundred and fifty-six channel values, macOS matched V8 on two-oh-seven.
> WebAssembly's on two-two-five. Neither matches.
>
> But luminance only ever sees integers, zero to two fifty-five. The input's
> already rounded. That's not a function — that's a lookup table.
>
> So it's a generated table of V8's exact bits. Bit-exact everywhere, and faster
> than calling `pow`.

---

## 2:50 — Segment 6: upstream's own demo page

*[Browser, already loaded. Type a colour into the box.]*

> This is TinyColor's demo page from the pinned commit. Unmodified HTML.
>
> Every swatch on it is computed in Rust.

*[Let the swatches update. Two seconds of silence.]*

> Same global, same API, different language underneath. Nothing downstream had
> to change.

---

## 3:20 — Segment 7: the regression, said out loud

*[Slow down again. This is a credibility play, not an apology. Say it evenly.]*

> Now the part most demos skip.
>
> The port is slower. Five to ten times slower per operation. And the shipped
> WebAssembly build loses on startup and memory too — seventy-one milliseconds
> to boot, against eighteen.

*[Show the layer table in the README.]*

> But it isn't the colour maths. Peel the layers apart: the ported Rust runs at
> one-point-three times the original. The shipped artifact runs at eight.
>
> Ninety-six percent of that gap is the bridge.
>
> That's a deliberate trade — one JSON boundary instead of forty-five FFI
> signatures. It's the only reason the unmodified suite can drive the port at
> all. I'd make the same call again. I won't pretend it was free.

---

## 3:55 — Segment 8: where the win is, and zero unsafe

> The win is runtime elimination. The native binary starts nearly six times
> faster, in four megabytes instead of thirty-four — and it carries the same
> forty-five out of forty-five, and the same fuzz evidence.
>
> And zero `unsafe`, in all three crates. Not a claim in a README — `forbid`
> makes it a compile error.

---

## 4:20 — Segment 9: the decision log

*[Scroll `DECISIONS.md`. Don't read entries aloud — let the headings pass.]*

> Twenty-two documented divergences. `Math.round` breaks ties differently in the
> two languages. The string `"1.0"` and the number `1.0` are different colours.
>
> And a real bug in the original — `analogous` with a negative count loops
> forever and eats the heap. Filed upstream.

*[Show issue #280 in the browser for two seconds.]*

> Four of these are bugs in *my* port. A handle table that leaked until the
> WebAssembly heap died. A transport that hung every process using it.
>
> They're in the log because a decision log that only records other people's
> mistakes isn't one.

---

## 4:50 — Close

> Correct against the original's own tests. Two transports. The regression on
> the card instead of buried.
>
> Everything reproduces from a clean checkout.

*[Final frame: the repo URL on screen. Stop talking. Let it sit for two seconds.]*

---

## Delivery notes

- **Pace.** Slower than feels natural. Nerves speed you up ~15%. If you finish
  under 4:30 you rushed it.
- **The three places to slow down:** the cold open, "off by one unit in the last
  place", and "the port is slower". Everything else can move.
- **Don't read the screen.** If a number is visible, say what it *means*, not
  what it says.
- **Silence is fine.** Four hash lines landing, a test suite printing, swatches
  updating — let those breathe. Talking over your own evidence buries it.
- **Say numbers as words**, the way the script has them. "Thirty-one million",
  not "thirty one comma zero two zero comma five two five".
- **One stumble is not a retake.** Pause, breathe, say the sentence again. You're
  cutting per segment anyway.

## If you only get one take

Cold open → segments 1, 2, 3, 4, 7. That's about 2:50 and it covers the 40%,
the 30%, and the mandatory disclosure. Segments 5, 6, 8 and 9 are what lift it
above competent, but they are not what it fails without.

# What to say — teleprompter script

Just the words, in order. Stage directions in brackets. `DEMO-SCRIPT.md` has
the full shot list; this is what comes out of your mouth.

**Read it out loud once before recording.** Anything you stumble on, change —
it's your voice, not mine. Written prose and spoken prose are different
languages, and this is written for the ear: short sentences, numbers said the
way people say them, no clause stacking.

619 spoken words — 4:16 at 145 wpm, plus ~42 seconds of deliberate silence.
That lands at **4:58**. There is no slack, so the shot timings in brackets are
budgets, not suggestions: if a shot runs long, cut it short rather than talking
over the next one. **Let it land.** Silence while
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

## 0:25 — Segment 2: show them the actual test file

*[Open `tests/original/test.js` in the editor. Line 5 on screen. This is the
most convincing shot in the video — it proves the mechanism without a word of
architecture. Do not rush it.]*

> This is that file. Two thousand one hundred and ninety-one lines, four hundred
> and thirty-two assertions, written for the JavaScript library.
>
> Line five is how it loads what it's testing.

*[Highlight line 5: `const tinycolor = require("./tinycolor.js");`  — hold two
seconds.]*

> One require. So I put mine at that path.

*[Scroll fast — four seconds, a blur of `assertEquals`. Say nothing. Volume is
the point.]*

---

## 0:50 — Segment 3: forty-five out of forty-five

```bash
node tests/run-all.mjs
```

> Same file. Never edited. Now loading Rust.

*[45/45 lands. Two seconds of silence. Then flash `tests/original/tinycolor.js`
— our shim — for three seconds.]*

> A hundred and ten lines of adapter. None of it does colour maths.

---

## 1:15 — Segment 4: not a WebAssembly trick

```bash
TINYCOLOR_TRANSPORT=native node tests/run-all.mjs
```

> Same suite. Same forty-five. But not WebAssembly this time — a native Rust
> binary on the other end of a pipe.
>
> The correctness lives in the port, not the plumbing.

---

## 1:35 — Segment 5: two things you should be suspicious of

*[Fast. Two commands, no preamble. This kills the two obvious objections.]*

> Two fair objections. First — the original is in this repo. Is the port calling
> it?

```bash
mv upstream/tinycolor.reference.js /tmp/
node tests/run-all.mjs
```

> Deleted. Still forty-five. It's only the fuzzer's reference.

*[Restore the file on camera: `mv /tmp/tinycolor.reference.js upstream/`]*

> Second — that's my runner printing the number. Would it print a smaller one?

*[Show a pre-recorded or live run against a deliberately broken build.]*

```
✗ mostReadable
upstream suite: 40/45 tests passing (88.9%)
```

> Break one method and it says forty, and names the test. Not a rubber stamp.

---

## 2:05 — Segment 6: the differential fuzzer

```bash
node fuzz/harness.mjs --seconds 10 --seed 1
```

> A fixed suite is table stakes. So — a differential fuzzer.
>
> Random colours into the original on V8, and into the port. Compared bit for
> bit. No epsilon. Floats match exactly, or it's a divergence.

*[Let the counter run. Then open `fuzz/logs/run-wasm-300s-seed1.json`.]*

> Five minutes. Thirty-one million comparisons. Zero divergences.
>
> And the same on the native transport.

---

## 2:35 — Segment 7: the payoff for the cold open

*[This is the segment people will remember. Slow down.]*

> So — that failing assertion.
>
> Relative luminance. Off by one unit in the last place — the smallest
> disagreement two doubles can have.
>
> I assumed a rounding bug. It wasn't. The answer changed depending on what I
> built for. Native Rust gave one number. The same source, as WebAssembly, gave
> another.

*[Show the D-012 table in `DECISIONS.md`.]*

> `Math.pow` isn't bit-identical across libm implementations. Over all two
> hundred and fifty-six channel values, macOS matched V8 on two-oh-seven,
> WebAssembly's on two-two-five. Neither matches.
>
> But luminance only ever sees integers, zero to two fifty-five. That's not a
> function — that's a lookup table.
>
> So it's a generated table of V8's exact bits. Bit-exact everywhere, and faster
> than `pow`.

---

## 3:20 — Segment 8: upstream's own demo page

*[Browser, already loaded. Type a colour into the box.]*

> TinyColor's own demo page, unmodified HTML. Every swatch is computed in Rust.

*[Let the swatches update. Two seconds of silence.]*

> Same global, same API. Nothing downstream had to change.

---

## 3:40 — Segment 9: the regression, said out loud

*[Slow down again. This is a credibility play, not an apology. Say it evenly.]*

> Now the part most demos skip.
>
> The port is slower. Five to ten times, per operation. And the WebAssembly
> build loses on startup and memory too — seventy-one milliseconds against
> eighteen.

*[Show the layer table in the README.]*

> But it isn't the colour maths. Peel the layers apart: the ported Rust runs at
> one-point-three times the original. The shipped artifact, eight.
>
> Ninety-six percent of the gap is the bridge — one JSON boundary instead of
> forty-five FFI signatures. That's the only reason the unmodified suite can
> drive the port at all.
>
> I'd make the same call again. I won't pretend it was free.

---

## 4:10 — Segment 10: where the win is, and zero unsafe

> The win is runtime elimination. The native binary starts nearly six times
> faster, in four megabytes instead of thirty-four — carrying the same
> forty-five out of forty-five, and the same fuzz evidence.
>
> And zero `unsafe`, in all three crates. Not a README claim — `forbid` makes it
> a compile error.

---

## 4:28 — Segment 11: the decision log

*[Scroll `DECISIONS.md`. Don't read entries aloud — let the headings pass.]*

> Twenty-two documented divergences. `Math.round` breaks ties differently in the
> two languages. The string `"1.0"` and the number `1.0` are different colours.
>
> And a real bug in the original — `analogous` with a negative count loops
> forever. Filed upstream.

*[Show issue #280 in the browser for two seconds.]*

> Four of these are bugs in *my* port. A handle table that leaked until the
> WebAssembly heap died. A transport that hung every process using it.
>
> A decision log that only records other people's mistakes isn't one.

---

## 4:52 — Close

> Correct against the original's own tests, on two transports, with the
> regression on the card instead of buried. All of it reproduces from a clean
> checkout.

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

Cold open → segments 1, 2, 3, 5, 9. That's about 3:00 and it covers the 40%,
the 30%, and the mandatory disclosure. Segments 4, 6, 7, 9 and 10 lift it above
competent; they are not what it fails without.

If you cut anything, do not cut segment 2. Showing the actual upstream test
file — and line 5 — is the single shot that makes every later claim credible.

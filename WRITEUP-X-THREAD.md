# Write-Up Side Quest — X thread

Post as a thread. Each numbered block is one post, all under 280 characters.
Char counts are in the comment after each. Links only in 1/ and the last post
(X suppresses reach on link-heavy threads).

---

**1/**

I ported a JavaScript colour library to Rust and made the original test suite
run against it, unmodified.

It failed on one assertion. Off by a single bit.

The bug wasn't in my Rust. It wasn't in the JavaScript either.

<!-- 219 chars -->

---

**2/**

Setup: TinyColor, 1,187 lines of JS. Hackathon rule — the original test suite
has to pass, unedited, hash-verified.

Most people translate the tests into their target language. That quietly
forfeits the whole point: you're now testing your tests.

<!-- 246 chars -->

---

**3/**

The trick is that upstream's test file loads the library with exactly one line:

    require("./tinycolor.js")

So put your shim at that path. Compile the Rust to WASM, expose the same API.

test.js: 0 bytes changed. sha256 verifies. 45/45.

<!-- 240 chars -->

---

**4/**

Then this failed:

    readability("#000", "#111")
    expected 1.1121078324840545
    got      1.1121078324840543

One ULP. The smallest possible disagreement between two f64s.

<!-- 177 chars -->

---

**5/**

I assumed a rounding bug in my port. It wasn't.

The number changed depending on *which build target* I ran.

Native Rust: 1.1121078324840545 ✅
Same Rust as WASM: ...543 ❌

Identical source. Different answer.

<!-- 208 chars -->

---

**6/**

Cause: `Math.pow` is not bit-identical across libm implementations.

Over all 256 possible channel values, matching V8 exactly:

  macOS libm (std powf) — 207/256
  wasm32 MUSL pow — 225/256
  V8's fdlibm — the reference

Neither matches. Ever.

<!-- 244 chars -->

---

**7/**

The fix came from noticing the input domain.

`getLuminance()` gets its input from `toRgb()`, which rounds. So `pow` only ever
sees integers 0–255.

256 possible inputs. That's not a function, that's a lookup table.

<!-- 215 chars -->

---

**8/**

So I generated a table of V8's exact f64 bit patterns for all 256 values and
index into it.

Bit-exact on every target. No libm dependency. And a table read is faster than
a transcendental call.

Test passes. 45/45.

<!-- 215 chars -->

---

**9/**

Other things JavaScript does that a naive port gets wrong:

`{r: "1.0"}` and `{r: 1.0}` are different colours. Off by 255x. The string is
silently reinterpreted as "100%".

Math.round(-1.5) is -1 in JS, -2 in Rust.

parseFloat("50%") is 50.

<!-- 240 chars -->

---

**10/**

To catch the rest I ran a differential fuzzer: original on V8 and port on WASM,
same process, same inputs, comparing at exact float-bit equality — not an
epsilon.

31,020,525 comparisons. Zero divergences.

<!-- 205 chars -->

---

**11/**

Getting there took four real bugs. The interesting part is where they were.

Alpha silently dropped. `ToNumber` vs `parseFloat` (they disagree on "1%").
NaN and Infinity turning into null crossing JSON.

And one that took 4.6 million colours to show up.

<!-- 253 chars -->

---

**12/**

That one: my Rust side kept colours in a handle table. JS garbage collection
knows nothing about a Rust HashMap, so every colour ever created leaked.

At ~4.6M handles the WASM heap died.

A 60-second fuzz run never reaches it. A 300-second one does.

<!-- 250 chars -->

---

**13/**

The pattern I didn't expect:

Three of the four genuine bugs were in the *bridge* between JS and Rust. Not one
was in the ported colour maths.

The translation was the easy part. The seam was where everything went wrong.

<!-- 220 chars -->

---

**14/**

The fuzzer also found a bug in the original — 15 years old, 5.2k stars.

`analogous(-1)` decrements a counter and tests truthiness. From -1 it goes
-2, -3, -4… and never hits 0. Each pass allocates.

Unbounded loop. Heap exhaustion. Filed upstream.

<!-- 248 chars -->

---

**15/**

Now the part I'd rather not write.

The port is *slower*. 5–10x slower per colour operation than the JavaScript,
measured honestly with warmup and a fixed corpus.

Rust did not make the colour maths faster. V8 is really good at this.

<!-- 233 chars -->

---

**16/**

What eliminating the runtime actually bought:

  startup: 18.4ms → 3.2ms
  memory: 34.3MB → 4.4MB

Process-level cost, not throughput. That's a real win for a CLI. It is not the
win people assume "rewrite it in Rust" delivers.

<!-- 226 chars -->

---

**17/**

One more honest note: the WASM build is *worse* than the original on both.
71ms startup, 86MB. Instantiating a 1.1MB module costs more than booting V8.

So I built a second transport. Same core, native binary, same 45/45, same fuzz
evidence.

<!-- 241 chars -->

---

**18/**

That mattered more than it sounds. Before it, the fast artifact and the *proven*
artifact were different binaries.

Quoting one's speed under the other's correctness evidence would have been a
sleight of hand. Now both carry the same proof.

<!-- 240 chars -->

---

**19/**

22 behavioural divergences written up, each with what upstream does, what a
naive port produces, and how it's verified.

The port is the easy half. Proving it behaves the same is the actual work.

Code, evidence, decision log:
github.com/BigAchiever/tinycolor-rs

<!-- 262 chars -->

---

## Notes before posting

- **1/ has no link.** X throttles reach on posts with links; the repo goes in
  the last post only.
- **4/, 5/, 6/ are the shareable core.** If you only post a subset, post 1–8.
- Consider a screenshot of the 45/45 output attached to 3/, and the
  207/256 vs 225/256 table as an image on 6/ — tables render badly as text.
- Post 14 could link the upstream issue (#280) if you want the receipt visible,
  at the cost of reach on that post.
- Tag: #rustlang #javascript #webassembly. Two max — more reads as spam.

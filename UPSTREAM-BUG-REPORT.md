# Upstream bug report — ready to file at github.com/bgrins/TinyColor/issues

Found by the differential harness in this repo. Reproduced against upstream at
`b49018c9f2dbca313d80d7a4dad25e26143cfe01`. Not yet filed — see the note at the
bottom.

---

**Title:** `analogous()` and `monochromatic()` loop unboundedly on negative or
fractional counts, exhausting memory

**Body:**

`analogous(results)` and `monochromatic(results)` both decrement their loop
counter and test it for truthiness, so a counter that never lands exactly on `0`
never terminates. Each iteration pushes a `tinycolor` instance, so the process
exhausts memory rather than hanging quietly.

```js
// tinycolor.js
for (hsl.h = (hsl.h - ((part * results) >> 1) + 720) % 360; --results; ) { ... }  // analogous
while (results--) { ... }                                                        // monochromatic
```

From `results = -1` the sequence is `-2, -3, -4, …`; from `results = 1.5` it is
`0.5, -0.5, -1.5, …`. Neither reaches `0`.

**Reproduction** (node 20, upstream unmodified):

```
$ node --max-old-space-size=256 -e "require('tinycolor2')('red').analogous(-1)"
<--- Last few GCs --->
FATAL ERROR: Reached heap limit Allocation failed
$ echo $?
134
```

Same for `analogous(1.5)`, `analogous(0.5)`, `monochromatic(-1)`,
`monochromatic(1.5)`, `monochromatic(0.5)`. `0`, `2` and `null` are fine
(`0` and `null` fall back to the default of 6).

**Why this is worth guarding:** `polyad()` — added later and covering the same
shape of input — already validates:

```js
if (isNaN(number) || number <= 0) {
  throw new Error("Argument to polyad must be a positive number");
}
```

So the hazard is recognised in one of the three combination functions and not the
other two. Any caller passing a user-supplied count to `analogous()` or
`monochromatic()` (a palette-size input in a colour tool, for example) has an
unauthenticated way to exhaust the process heap.

**Suggested fix:** apply the same guard `polyad()` uses, or coerce with
`Math.max(1, Math.floor(results))`. The former is consistent with existing
behaviour; the latter avoids a breaking change for callers currently passing
fractional values that happen to work.

---

## Note on filing

This has NOT been filed upstream. Filing requires a GitHub account, and it should
be filed by the team under their own identity rather than fabricated here. The
report above is complete and can be pasted as-is.

The port's behaviour for these inputs is documented in DECISIONS.md (D-022): it
returns a finite list rather than reproducing the unbounded loop. That is a
deliberate, documented divergence from the original — the one place this port
does not reproduce upstream behaviour, because reproducing it would mean
reproducing a denial-of-service.

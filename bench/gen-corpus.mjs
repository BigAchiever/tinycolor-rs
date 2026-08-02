// Generates bench/corpus.json — the fixed input set both harnesses read.
//
//   node bench/gen-corpus.mjs
//
// Committed to the repo and hashed into every results file, so a published
// number can be tied to the exact inputs that produced it.
//
// Deliberately NOT reusing fuzz/generators.mjs: those are boundary-biased
// (NaN, 1e-7%, pipe separators, malformed) because their job is to find
// divergences. Blending them into a throughput corpus would over-weight
// failure paths. The invalid path is measured here too, but as its own
// named row, never averaged in.
//
// The weights below are an ASSUMPTION about design-system / CSS-tooling
// usage. They are not a measurement of any real codebase. Stated as such in
// the report.

import { writeFileSync } from "node:fs";
import { createHash } from "node:crypto";

function mulberry32(seed) {
  let a = seed >>> 0;
  return function () {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const rng = mulberry32(0x5eed);
const int = (lo, hi) => Math.floor(rng() * (hi - lo + 1)) + lo;
const pick = (a) => a[Math.floor(rng() * a.length)];
const hx = (n) => int(0, n).toString(16).padStart(2, "0");

// Sampled across the alphabet on purpose. parse.rs scans NAMES front-to-back
// and color.rs scans HEX_NAMES back-to-front — inverted — so a corpus drawn
// only from the 'a's would flatter one and punish the other.
const NAMES = [
  "aliceblue", "aqua", "azure", "beige", "black", "blue", "brown",
  "chartreuse", "coral", "crimson", "cyan", "darkblue", "darkgreen",
  "firebrick", "gold", "gray", "green", "hotpink", "indigo", "ivory",
  "khaki", "lavender", "lime", "magenta", "maroon", "navy", "olive",
  "orange", "orchid", "pink", "purple", "red", "salmon", "silver",
  "teal", "tomato", "turquoise", "violet", "wheat", "white",
  "yellow", "yellowgreen", "whitesmoke",
];

// ~20% of valid entries are achromatic: rgb_to_hsl early-returns on max==min
// and hsl_to_rgb short-circuits on s==0, so a corpus of saturated hues is
// measurably more expensive than a neutral ramp on BOTH sides.
function channels() {
  if (rng() < 0.2) {
    const v = int(0, 255);
    return [v, v, v];
  }
  return [int(0, 255), int(0, 255), int(0, 255)];
}

const GENERATORS = [
  ["hex6", 40, () => { const [r,g,b] = channels(); return "#" + hx(r)+hx(g)+hx(b); }],
  ["hex3", 10, () => "#" + int(0,15).toString(16) + int(0,15).toString(16) + int(0,15).toString(16)],
  ["hex8",  5, () => { const [r,g,b] = channels(); return "#" + hx(r)+hx(g)+hx(b)+hx(int(0,255)); }],
  ["hex4",  3, () => "#" + [0,0,0,0].map(() => int(0,15).toString(16)).join("")],
  ["name", 12, () => pick(NAMES)],
  ["rgb",   6, () => { const [r,g,b] = channels(); return `rgb(${r}, ${g}, ${b})`; }],
  ["rgba",  4, () => { const [r,g,b] = channels(); return `rgba(${r}, ${g}, ${b}, ${(rng()).toFixed(2)})`; }],
  ["hsl",   5, () => `hsl(${int(0,360)}, ${int(0,100)}%, ${int(0,100)}%)`],
  ["hsla",  3, () => `hsla(${int(0,360)}, ${int(0,100)}%, ${int(0,100)}%, ${(rng()).toFixed(2)})`],
  ["object",4, () => { const [r,g,b] = channels(); return { r, g, b }; }],
  ["invalid",5, () => pick(["not a color","#gggggg","rgb(","","hsl(999","chartrooze","#fffff"])],
  ["padded", 3, () => { const [r,g,b] = channels(); return pick(["  ","\t",""]) + "#" + (hx(r)+hx(g)+hx(b)).toUpperCase() + pick(["  ","",""]); }],
];

const total = GENERATORS.reduce((s, [, w]) => s + w, 0);
const SIZE = 512;

const entries = [];
for (const [kind, weight, fn] of GENERATORS) {
  const n = Math.round((weight / total) * SIZE);
  for (let i = 0; i < n; i++) entries.push({ kind, value: fn() });
}
// Pad or trim to exactly SIZE using the dominant class.
while (entries.length < SIZE) entries.push({ kind: "hex6", value: GENERATORS[0][2]() });
entries.length = SIZE;

// Deterministic shuffle so classes interleave (branch predictors should not
// get a free ride from 200 consecutive hex6 entries) — but reproducible.
for (let i = entries.length - 1; i > 0; i--) {
  const j = Math.floor(rng() * (i + 1));
  [entries[i], entries[j]] = [entries[j], entries[i]];
}

const counts = {};
for (const e of entries) counts[e.kind] = (counts[e.kind] || 0) + 1;

const payload = {
  seed: "0x5eed",
  size: entries.length,
  generator: "bench/gen-corpus.mjs",
  note: "Weights are an assumption about CSS-tooling usage, not a measurement of any real codebase.",
  counts,
  entries,
};

const json = JSON.stringify(payload, null, 1);
writeFileSync("bench/corpus.json", json);
const sha = createHash("sha256").update(json).digest("hex");

console.log(`wrote bench/corpus.json  ${entries.length} entries`);
console.log(`sha256 ${sha}`);
console.log("class counts:", JSON.stringify(counts));

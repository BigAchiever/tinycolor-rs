// Differential fuzz harness: upstream TinyColor (running on V8) vs the Rust
// port (running as WebAssembly), in the same process, on identical inputs.
//
//   node fuzz/harness.mjs                    # 60s, seed 1
//   node fuzz/harness.mjs --seconds 300      # longer run
//   node fuzz/harness.mjs --seed 42 -v       # reproduce a run, show diffs
//
// A case is a generated input plus the full operation battery applied to it.
// Every result is compared with exact equality — floats bit-for-bit, not
// within an epsilon. That strictness is deliberate: the 1-ULP `Math.pow`
// divergence recorded as D-012 would have been invisible under a tolerance.
//
// Exit code is non-zero if any divergence is found.

import { createRequire } from "node:module";
import { mkdirSync, writeFileSync } from "node:fs";
import { makeRng, makeGenerators, amount } from "./generators.mjs";

const require = createRequire(import.meta.url);
const ref = require("../upstream/tinycolor.reference.js");
const port = require("../tests/original/tinycolor.js");

const argv = process.argv.slice(2);
const flag = (name, def) => {
  const i = argv.indexOf(name);
  return i === -1 ? def : argv[i + 1];
};
const SECONDS = Number(flag("--seconds", 60));
const SEED = Number(flag("--seed", 1));
const VERBOSE = argv.includes("-v") || argv.includes("--verbose");

// ---------------------------------------------------------------------------
// Canonical encoding — must distinguish values JSON would flatten together.
// NaN, -0 and Infinity all have to survive the comparison intact.
// ---------------------------------------------------------------------------
function canon(v) {
  if (v === null) return "null";
  if (v === undefined) return "undefined";
  if (typeof v === "number") {
    if (Number.isNaN(v)) return "NaN";
    if (v === 0) return Object.is(v, -0) ? "-0" : "0";
    if (!Number.isFinite(v)) return String(v);
    // Exact bit pattern, so 1-ULP differences cannot hide.
    const b = Buffer.alloc(8);
    b.writeDoubleLE(v);
    return `f64:${b.readBigUInt64LE().toString(16)}`;
  }
  if (typeof v === "string") return `s:${v}`;
  if (typeof v === "boolean") return `b:${v}`;
  if (Array.isArray(v)) return `[${v.map(canon).join(",")}]`;
  if (typeof v === "object") {
    return `{${Object.keys(v)
      .sort()
      .map((k) => `${k}:${canon(v[k])}`)
      .join(",")}}`;
  }
  return `?${String(v)}`;
}

// A colour instance is compared by a fingerprint of its observable state.
const colorFingerprint = (c) =>
  c == null ? "null" : canon([c.toHex8String(), c.getFormat(), c.getAlpha()]);

// ---------------------------------------------------------------------------
// Operation battery. Each entry runs against one implementation's factory.
// ---------------------------------------------------------------------------
const OPS = [
  ["toHexString", (tc, i) => tc(i).toHexString()],
  ["toHexString(3)", (tc, i) => tc(i).toHexString(true)],
  ["toHex8String", (tc, i) => tc(i).toHex8String()],
  ["toHex8String(4)", (tc, i) => tc(i).toHex8String(true)],
  ["toRgbString", (tc, i) => tc(i).toRgbString()],
  ["toHslString", (tc, i) => tc(i).toHslString()],
  ["toHsvString", (tc, i) => tc(i).toHsvString()],
  ["toPercentageRgbString", (tc, i) => tc(i).toPercentageRgbString()],
  ["toName", (tc, i) => tc(i).toName()],
  ["toString", (tc, i) => tc(i).toString()],
  ["toFilter", (tc, i) => tc(i).toFilter()],
  ["toRgb", (tc, i) => tc(i).toRgb()],
  ["toHsl", (tc, i) => tc(i).toHsl()],
  ["toHsv", (tc, i) => tc(i).toHsv()],
  ["toPercentageRgb", (tc, i) => tc(i).toPercentageRgb()],
  ["isValid", (tc, i) => tc(i).isValid()],
  ["getFormat", (tc, i) => tc(i).getFormat()],
  ["getAlpha", (tc, i) => tc(i).getAlpha()],
  ["getBrightness", (tc, i) => tc(i).getBrightness()],
  ["getLuminance", (tc, i) => tc(i).getLuminance()],
  ["isDark", (tc, i) => tc(i).isDark()],
  ["isLight", (tc, i) => tc(i).isLight()],
  ["getOriginalInput", (tc, i) => canon(tc(i).getOriginalInput())],
  ["clone", (tc, i) => colorFingerprint(tc(i).clone())],
];

// Operations taking a numeric amount.
const AMOUNT_OPS = [
  "lighten",
  "brighten",
  "darken",
  "desaturate",
  "saturate",
  "spin",
];

// Operations returning a list of colours.
const LIST_OPS = ["triad", "tetrad", "splitcomplement", "analogous", "monochromatic"];

// Operations pushed individually inside runCase, counted here so the banner
// cannot drift from the battery again: greyscale, complement, fromRatio,
// fromRatio+format, mix, equals, readability, isReadable, mostReadable, setAlpha.
const ONE_OFF_OPS = 10;

// ---------------------------------------------------------------------------

function runCase(tc, input, amt, second, polyN, ratio) {
  const out = [];
  for (const [name, fn] of OPS) {
    out.push([name, safe(() => fn(tc, input))]);
  }
  for (const m of AMOUNT_OPS) {
    out.push([`${m}(${amt})`, safe(() => tc(input)[m](amt).toHex8String())]);
  }
  out.push(["greyscale", safe(() => tc(input).greyscale().toHex8String())]);
  out.push(["complement", safe(() => tc(input).complement().toHex8String())]);
  for (const m of LIST_OPS) {
    out.push([m, safe(() => tc(input)[m]().map((c) => c.toHex8String()))]);
  }
  // `polyad` is deliberately NOT fuzzed. Upstream comments the prototype
  // method out (mod.js:265) and marks its test `ignore: true` pending
  // bgrins/TinyColor#254, so the reference throws "not a function" for every
  // input. The port implements it because the disabled test expects it, but
  // there is nothing to differentially compare against. See D-015.
  void polyN;
  // fromRatio takes 0..1 ratios rather than 0..255 channels, so it needs its
  // own input shape — it is the one public static the corpus generators cannot
  // reach on their own.
  out.push([
    "fromRatio",
    safe(() => tc.fromRatio(ratio).toHex8String()),
  ]);
  out.push([
    "fromRatio+format",
    safe(() => tc.fromRatio(ratio, { format: "hsl" }).toString()),
  ]);
  out.push(["mix", safe(() => tc.mix(input, second, amt).toHex8String())]);
  out.push(["equals", safe(() => tc.equals(input, second))]);
  out.push(["readability", safe(() => tc.readability(input, second))]);
  out.push(["isReadable", safe(() => tc.isReadable(input, second))]);
  out.push([
    "mostReadable",
    safe(() => colorFingerprint(tc.mostReadable(input, [second, "#fff", "#000"]))),
  ]);
  out.push([
    "setAlpha",
    safe(() => tc(input).setAlpha(0.5).toHex8String()),
  ]);
  return out;
}

// Thrown errors are part of observable behaviour and must match too.
function safe(fn) {
  try {
    return { ok: canon(fn()) };
  } catch (e) {
    return { err: String(e && e.message ? e.message : e) };
  }
}

function compare(a, b) {
  if ("err" in a || "err" in b) {
    if (!("err" in a) || !("err" in b)) return false;
    // Both threw; message text differs by implementation, so only the fact
    // of throwing is compared. Divergent *messages* are not behavioural.
    return true;
  }
  return a.ok === b.ok;
}

// ---------------------------------------------------------------------------

const rng = makeRng(SEED);
const generators = makeGenerators(ref.names);
const started = Date.now();
const deadline = started + SECONDS * 1000;

let cases = 0;
let comparisons = 0;
let lastLogged = -5;
let peakHandles = 0;
const divergences = [];
const byGenerator = Object.fromEntries(generators.map(([n]) => [n, 0]));

console.log(
  `differential fuzz: upstream(V8) vs port(${port.__transport})  seed=${SEED}  budget=${SECONDS}s`
);
const OPS_PER_CASE =
  OPS.length + AMOUNT_OPS.length + LIST_OPS.length + ONE_OFF_OPS;
console.log(`${generators.length} generators x ${OPS_PER_CASE} operations per case\n`);

while (Date.now() < deadline) {
  for (let batch = 0; batch < 200 && Date.now() < deadline; batch++) {
    const [genName, gen] = generators[Math.floor(rng() * generators.length)];
    const input = gen(rng);
    const [, secondGen] = generators[Math.floor(rng() * generators.length)];
    const second = secondGen(rng);
    const amt = amount(rng);
    const polyN = Math.floor(rng() * 8) + 1;
    // Ratio object for fromRatio: mostly in-range, sometimes not, sometimes
    // hsl-shaped, sometimes carrying alpha (which fromRatio must NOT scale).
    const r3 = () => (rng() < 0.85 ? +rng().toFixed(4) : [0, 1, 1.5, -0.2][Math.floor(rng() * 4)]);
    const ratio =
      rng() < 0.6
        ? { r: r3(), g: r3(), b: r3(), ...(rng() < 0.4 ? { a: +rng().toFixed(2) } : {}) }
        : { h: r3(), s: r3(), l: r3() };

    const a = runCase(ref, input, amt, second, polyN, ratio);
    const b = runCase(port, input, amt, second, polyN, ratio);

    cases++;
    byGenerator[genName]++;

    for (let i = 0; i < a.length; i++) {
      comparisons++;
      if (!compare(a[i][1], b[i][1])) {
        divergences.push({
          generator: genName,
          op: a[i][0],
          input: canon(input),
          second: canon(second),
          amount: canon(amt),
          reference: a[i][1],
          port: b[i][1],
        });
      }
    }
  }
  // No colours survive a case, so every handle from the batch is garbage.
  // FinalizationRegistry alone cannot keep up here — this loop allocates far
  // faster than the collector runs — so reclaim deterministically.
  const live = port.__liveHandles(); // sample before reclaiming
  peakHandles = Math.max(peakHandles, live);
  port.__reset();

  const pct = Math.min(100, ((Date.now() - started) / (SECONDS * 1000)) * 100);
  const line = `  ${pct.toFixed(0)}%  cases=${cases}  comparisons=${comparisons}  divergences=${divergences.length}  peakHandles=${peakHandles}`;
  if (process.stdout.isTTY) {
    process.stdout.write(`\r${line}   `);
  } else if (pct - lastLogged >= 5) {
    // Redirected to a file: emit a readable progress line every 5% instead of
    // carriage-returning over one line. The log is a submission artifact.
    lastLogged = pct;
    process.stdout.write(`${line}\n`);
  }
}

const elapsed = (Date.now() - started) / 1000;
process.stdout.write("\n\n");

// Group divergences so a thousand instances of one bug read as one bug.
const groups = new Map();
for (const d of divergences) {
  const key = `${d.op}|${d.generator}`;
  if (!groups.has(key)) groups.set(key, { ...d, count: 0, examples: [] });
  const g = groups.get(key);
  g.count++;
  if (g.examples.length < 3) g.examples.push(d);
}

if (groups.size) {
  console.log(`DIVERGENCES — ${groups.size} distinct, ${divergences.length} total\n`);
  for (const g of groups.values()) {
    console.log(`  ${g.op}  [generator: ${g.generator}]  x${g.count}`);
    for (const e of g.examples.slice(0, VERBOSE ? 3 : 1)) {
      console.log(`      input     ${e.input}`);
      console.log(`      reference ${JSON.stringify(e.reference)}`);
      console.log(`      port      ${JSON.stringify(e.port)}`);
    }
    console.log();
  }
} else {
  console.log("no divergences\n");
}

const summary = {
  // A "comparison" is ONE operation on ONE generated input, checked against
  // the reference. cases x operations_per_case = comparisons. Stated here so
  // the headline figure cannot be mistaken for a count of distinct inputs.
  unit: "one operation on one generated input, compared bit-for-bit",
  operations_per_case: OPS_PER_CASE,
  seed: SEED,
  seconds_requested: SECONDS,
  seconds_elapsed: +elapsed.toFixed(2),
  cases,
  comparisons,
  divergences: divergences.length,
  distinct_divergences: groups.size,
  comparisons_per_second: Math.round(comparisons / elapsed),
  peak_live_handles: peakHandles,
  generators: byGenerator,
  reference: "upstream/tinycolor.reference.js (V8)",
  // Must reflect what actually ran. This was hardcoded to the wasm string,
  // so a native run produced evidence that described itself as a wasm run.
  transport: port.__transport,
  port:
    port.__transport === "native"
      ? "target/release/tinycolor (Rust -> aarch64 native, stdio JSON-RPC)"
      : "tests/original/pkg/tinycolor_wasm.js (Rust -> wasm32)",
  groups: [...groups.values()].map((g) => ({
    op: g.op,
    generator: g.generator,
    count: g.count,
    examples: g.examples,
  })),
};

mkdirSync("fuzz/logs", { recursive: true });
writeFileSync("fuzz/logs/latest.json", JSON.stringify(summary, null, 2));

console.log("─".repeat(64));
console.log(`elapsed        ${elapsed.toFixed(1)}s`);
console.log(`cases          ${cases.toLocaleString()}`);
console.log(`comparisons    ${comparisons.toLocaleString()}  (${summary.comparisons_per_second.toLocaleString()}/s)`);
console.log(`divergences    ${divergences.length}`);
console.log("─".repeat(64));
console.log("wrote fuzz/logs/latest.json");

process.exit(divergences.length ? 1 : 0);

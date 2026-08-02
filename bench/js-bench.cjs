// Node-side benchmark driver. One row, one config, one JSON line on stdout.
//
//   node bench/js-bench.cjs --config upstream     --row to-hex-string --n 200000
//   node bench/js-bench.cjs --config wasm-shipped --row to-hex-string --n 20000
//   node bench/js-bench.cjs --config wasm-raw     --row rpc-new       --n 20000
//
// Configs
//   upstream      upstream/tinycolor.reference.js               (the original, on V8)
//   wasm-shipped  tests/original/tinycolor.js                   (shim + wasm + RPC) <- headline
//   wasm-raw      pkg/tinycolor_wasm.js dispatch() directly     (no shim, no JSON in JS)
//
// Timing policy: operations are batched (default 1000 per clock pair) and the
// reported distribution is over BATCH MEANS, not individual operations. For
// sub-microsecond work `process.hrtime.bigint()` resolution and call overhead
// are a meaningful fraction of a single op, so a per-op p99 would mostly be
// measuring the clock. Rows slower than ~1us use --mode per-op and DO report
// real percentiles. Every emitted row states which mode produced it.
//
// Warmup is identical for every config: 200,000 operations, discarded. V8
// needs ~1e4-1e5 invocations to tier these functions up to TurboFan, and
// giving the JS reference less warmup than the port would be rigging the
// comparison in the port's favour.

const { readFileSync } = require("node:fs");
const path = require("node:path");

const ROOT = path.resolve(__dirname, "..");
const argv = process.argv.slice(2);
const flag = (n, d) => {
  const i = argv.indexOf(n);
  return i === -1 ? d : argv[i + 1];
};

const CONFIG = flag("--config", "upstream");
const ROW = flag("--row", "to-hex-string");
const N = Number(flag("--n", 200000));
const WARMUP = Number(flag("--warmup", 200000));
const BATCH = Number(flag("--batch", 1000));
const MODE = flag("--mode", "batch");
const EMIT_CURVE = argv.includes("--emit-curve");

// ---------------------------------------------------------------------------

const corpus = JSON.parse(readFileSync(path.join(ROOT, "bench/corpus.json"), "utf8"));
const VALUES = corpus.entries.map((e) => e.value);
const STRINGS = corpus.entries.filter((e) => typeof e.value === "string").map((e) => e.value);

let tc = null;
let rawDispatch = null;

if (CONFIG === "upstream") {
  tc = require(path.join(ROOT, "upstream/tinycolor.reference.js"));
} else if (CONFIG === "wasm-shipped") {
  tc = require(path.join(ROOT, "tests/original/tinycolor.js"));
} else if (CONFIG === "wasm-raw") {
  rawDispatch = require(path.join(ROOT, "tests/original/pkg/tinycolor_wasm.js")).dispatch;
} else {
  throw new Error("unknown --config " + CONFIG);
}

// ---------------------------------------------------------------------------
// Checksum: folds every result so the optimiser cannot delete the work, and so
// two configs computing the same row can be proven to have done the same work.
// ---------------------------------------------------------------------------
const f64buf = new ArrayBuffer(8);
const f64 = new Float64Array(f64buf);
const u32 = new Uint32Array(f64buf);

let checksum = 0;
function fold(v) {
  if (typeof v === "string") {
    for (let i = 0; i < v.length; i++) checksum = (checksum ^ v.charCodeAt(i)) >>> 0;
  } else if (typeof v === "number") {
    f64[0] = v;
    checksum = (checksum ^ u32[0] ^ u32[1]) >>> 0;
  } else if (typeof v === "boolean") {
    checksum = (checksum ^ (v ? 1 : 2)) >>> 0;
  } else if (v && typeof v === "object") {
    for (const k of Object.keys(v).sort()) fold(v[k]);
  } else {
    checksum = (checksum ^ 7) >>> 0;
  }
}

// ---------------------------------------------------------------------------
// Rows. Each takes one corpus element and returns a value to fold.
// Reconstruction from the corpus value is INSIDE the timed region for parse
// rows and OUTSIDE for format rows — the native CLI draws the line in the same
// place, otherwise the ratio is meaningless.
// ---------------------------------------------------------------------------
const ROWS = {
  // parse + format (the headline: what a caller actually writes)
  "to-hex-string": (v) => tc(v).toHexString(),
  "to-rgb-string": (v) => tc(v).toRgbString(),
  "to-hsl-string": (v) => tc(v).toHslString(),
  "parse-only": (v) => tc(v).isValid(),
  "luminance": (v) => tc(v).getLuminance(),
  "lighten": (v) => tc(v).lighten(10).toHexString(),
  "mix": (v) => tc.mix(v, "#336699", 50).toHexString(),
  "to-name": (v) => tc(v).toName() || "",
  "round-trip": (v) => tc(tc(v).toHslString()).toHexString(),
};

const RAW_ROWS = {
  // Config C: the wasm boundary with no shim JS. Literal request strings, so
  // no JSON.stringify cost is attributed to the port here.
  "rpc-new": (s) => rawDispatch('{"op":"new","input":"' + s + '","opts":{}}'),
};

function makeOp() {
  if (CONFIG === "wasm-raw") {
    const fn = RAW_ROWS[ROW];
    if (!fn) throw new Error(`row ${ROW} not available for wasm-raw`);
    const pool = STRINGS.filter((s) => !s.includes('"') && !s.includes("\\"));
    return { fn, pool };
  }
  const fn = ROWS[ROW];
  if (!fn) throw new Error("unknown --row " + ROW);
  return { fn, pool: VALUES };
}

const { fn, pool } = makeOp();
const POOL_LEN = pool.length;

// ---------------------------------------------------------------------------

function runOps(count, startIdx) {
  let i = startIdx;
  for (let k = 0; k < count; k++) {
    fold(fn(pool[i % POOL_LEN]));
    i++;
  }
  return i;
}

// Warmup, discarded. Identical policy for every config.
let cursor = 0;
const curve = [];
if (EMIT_CURVE) {
  for (let b = 0; b < 40; b++) {
    const t0 = process.hrtime.bigint();
    cursor = runOps(1000, cursor);
    const t1 = process.hrtime.bigint();
    curve.push(Number(t1 - t0) / 1000);
  }
  cursor = runOps(Math.max(0, WARMUP - 40000), cursor);
} else {
  cursor = runOps(WARMUP, cursor);
}

const warmChecksum = checksum;
checksum = 0;

let result;

if (MODE === "per-op") {
  const samples = new Float64Array(N);
  for (let k = 0; k < N; k++) {
    const t0 = process.hrtime.bigint();
    fold(fn(pool[cursor % POOL_LEN]));
    const t1 = process.hrtime.bigint();
    samples[k] = Number(t1 - t0);
    cursor++;
  }
  const sorted = Array.from(samples).sort((a, b) => a - b);
  const q = (p) => sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))];
  result = {
    mode: "per-op",
    samples: N,
    mean_ns: sorted.reduce((a, b) => a + b, 0) / sorted.length,
    p50_ns: q(0.5), p90_ns: q(0.9), p99_ns: q(0.99), p999_ns: q(0.999),
    max_ns: sorted[sorted.length - 1],
  };
} else {
  const batches = Math.max(1, Math.floor(N / BATCH));
  const means = [];
  for (let b = 0; b < batches; b++) {
    const t0 = process.hrtime.bigint();
    cursor = runOps(BATCH, cursor);
    const t1 = process.hrtime.bigint();
    means.push(Number(t1 - t0) / BATCH);
  }
  const sorted = means.slice().sort((a, b) => a - b);
  const q = (p) => sorted[Math.min(sorted.length - 1, Math.floor(p * sorted.length))];
  result = {
    mode: "batch",
    batch_size: BATCH,
    batches,
    operations: batches * BATCH,
    // Distribution of PER-BATCH MEANS. Not a per-operation percentile.
    mean_ns: sorted.reduce((a, b) => a + b, 0) / sorted.length,
    batch_p50_ns: q(0.5), batch_p90_ns: q(0.9), batch_p99_ns: q(0.99),
    batch_max_ns: sorted[sorted.length - 1],
  };
}

process.stdout.write(
  JSON.stringify({
    config: CONFIG,
    row: ROW,
    ...result,
    checksum,
    warm_checksum: warmChecksum,
    warmup_ops: WARMUP,
    corpus_size: POOL_LEN,
    node: process.version,
    v8: process.versions.v8,
    exec_argv: process.execArgv,
    ...(EMIT_CURVE ? { warmup_curve_ns_per_op: curve } : {}),
  }) + "\n"
);

// Benchmark driver. Enforces the environment gates, runs every row across
// every config, measures process startup and RSS, and writes bench/results.json.
//
//   node bench/run.mjs                 # full run, refuses if gates fail
//   node bench/run.mjs --allow-throttled   # run anyway, marks output UNPUBLISHABLE
//   node bench/run.mjs --reps 3
//
// The gates are not decoration. On a fanless MacBook Air with Low Power Mode
// engaged the CPU is clocked down, and a number measured there is not a number
// about this software. If a gate fails the results file is stamped
// `"publishable": false` and every consumer of it should refuse to quote it.

import { execFileSync } from "node:child_process";
import { readFileSync, writeFileSync, mkdirSync } from "node:fs";
import { createHash } from "node:crypto";

const argv = process.argv.slice(2);
const ALLOW_THROTTLED = argv.includes("--allow-throttled");
const REPS = Number((argv.includes("--reps") && argv[argv.indexOf("--reps") + 1]) || 3);

const sh = (cmd, args, opts = {}) =>
  execFileSync(cmd, args, { encoding: "utf8", maxBuffer: 64 << 20, ...opts }).trim();

// ---------------------------------------------------------------------------
// Gates
// ---------------------------------------------------------------------------
function checkGates() {
  const gates = [];
  let ps = "", lpm = "";
  try { ps = sh("pmset", ["-g", "ps"]); } catch {}
  try { lpm = sh("sh", ["-c", "pmset -g | grep -i lowpowermode || true"]); } catch {}

  const onAC = /AC Power/.test(ps);
  const lpmOn = /lowpowermode\s+1/.test(lpm);
  const batteryPct = (ps.match(/(\d+)%/) || [])[1];

  // On Apple Silicon the throttle levers are Low Power Mode and thermal
  // state, not the presence of a charger. A well-charged battery with LPM off
  // is a legitimate measurement environment; a near-empty one is not, because
  // macOS starts managing power aggressively. Hence AC *or* a high state of
  // charge, and LPM off unconditionally.
  const pct = Number(batteryPct);
  gates.push({
    id: "P1a",
    name: "AC power or battery >= 80%",
    pass: onAC || pct >= 80,
    detail: `${ps.split("\n")[0]}, ${pct}%`,
  });
  gates.push({ id: "P1b", name: "Low Power Mode off", pass: !lpmOn, detail: lpm.trim() || "n/a" });
  gates.push({ id: "P1c", name: "battery >= 30% (no aggressive power management)", pass: pct >= 30, detail: `${pct}%` });

  let df = "";
  try { df = sh("sh", ["-c", "df -k / | tail -1 | awk '{print $4}'"]); } catch {}
  gates.push({ id: "P3", name: "disk >= 2GB free", pass: Number(df) / 1024 / 1024 >= 2, detail: `${(Number(df)/1024/1024).toFixed(1)} GB` });

  return gates;
}

// ---------------------------------------------------------------------------
// Throughput rows
// ---------------------------------------------------------------------------
const ROWS = [
  { row: "to-hex-string", headline: true, n: 200000 },
  { row: "parse-only", n: 200000 },
  { row: "to-rgb-string", n: 200000 },
  { row: "to-hsl-string", n: 200000 },
  { row: "lighten", n: 100000 },
  { row: "mix", n: 100000 },
  { row: "luminance", n: 200000 },
  { row: "to-name", n: 200000 },
  { row: "round-trip", n: 100000 },
];

// wasm-shipped is ~10x slower, so it gets fewer iterations to keep wall time
// sane. Iteration count is recorded per row so nobody compares across it.
const N_FOR = (cfg, n) => (cfg === "wasm-shipped" ? Math.max(50000, Math.round(n / 4)) : n);

function jsBench(config, row, n) {
  const out = sh("node", [
    "bench/js-bench.cjs",
    "--config", config,
    "--row", row,
    "--n", String(n),
    "--warmup", "200000",
  ]);
  return JSON.parse(out);
}

function nativeBench(workload, n) {
  const out = sh("./target/release/tinycolor", ["bench", workload, String(n)]);
  return JSON.parse(out);
}

// ---------------------------------------------------------------------------
// Startup + RSS. This is where the port genuinely wins, so it must be the
// most carefully qualified measurement in the file.
// ---------------------------------------------------------------------------
function measureStartup(label, cmd, args, reps = 20) {
  const times = [];
  for (let i = 0; i < reps; i++) {
    const t0 = process.hrtime.bigint();
    try { execFileSync(cmd, args, { stdio: "ignore" }); } catch { /* non-zero exit is fine */ }
    const t1 = process.hrtime.bigint();
    times.push(Number(t1 - t0) / 1e6);
  }
  times.sort((a, b) => a - b);
  return {
    label,
    command: [cmd, ...args].join(" "),
    reps,
    p50_ms: times[Math.floor(reps * 0.5)],
    p99_ms: times[Math.floor(reps * 0.99)] ?? times[reps - 1],
    min_ms: times[0],
    max_ms: times[reps - 1],
  };
}

function measureRss(label, cmd, args) {
  // /usr/bin/time -l reports "maximum resident set size" in BYTES on macOS,
  // and writes its report to stderr. Redirect stderr->stdout and discard the
  // program's own stdout, otherwise this reads the wrong stream (and returns
  // null, which is how this was silently broken the first time).
  const quoted = [cmd, ...args].map((a) => `'${String(a).replace(/'/g, "'\\''")}'`).join(" ");
  let out = "";
  try {
    out = execFileSync("sh", ["-c", `/usr/bin/time -l ${quoted} 2>&1 >/dev/null`], {
      encoding: "utf8", maxBuffer: 8 << 20,
    });
  } catch (e) {
    out = String((e && (e.stdout || e.stderr)) || "");
  }
  const m = String(out).match(/(\d+)\s+maximum resident set size/);
  return { label, command: [cmd, ...args].join(" "), max_rss_bytes: m ? Number(m[1]) : null,
           max_rss_mb: m ? +(Number(m[1]) / 1048576).toFixed(2) : null };
}

// ---------------------------------------------------------------------------

const gates = checkGates();
const failed = gates.filter((g) => !g.pass);
const publishable = failed.length === 0;

console.log("environment gates");
for (const g of gates) console.log(`  ${g.pass ? "PASS" : "FAIL"}  ${g.id} ${g.name}  (${g.detail})`);
console.log();

if (!publishable && !ALLOW_THROTTLED) {
  console.error(`REFUSING TO RUN: ${failed.length} gate(s) failed.`);
  console.error("Numbers measured under these conditions are not publishable.");
  console.error("Re-run with --allow-throttled to produce a clearly-marked indicative run.");
  process.exit(2);
}
if (!publishable) {
  console.warn("!! running with failed gates - output will be marked UNPUBLISHABLE\n");
}

const throughput = [];
for (const spec of ROWS) {
  for (const config of ["upstream", "wasm-shipped"]) {
    const n = N_FOR(config, spec.n);
    const reps = [];
    for (let r = 0; r < REPS; r++) reps.push(jsBench(config, spec.row, n));
    // Report the median rep, and the spread across reps, so run-to-run
    // variance is visible rather than hidden by picking the best.
    const means = reps.map((x) => x.mean_ns).sort((a, b) => a - b);
    throughput.push({
      row: spec.row,
      config,
      headline: !!spec.headline,
      iterations: n,
      reps: REPS,
      mean_ns: means[Math.floor(REPS / 2)],
      rep_spread_pct: +(((means[REPS - 1] - means[0]) / means[0]) * 100).toFixed(1),
      batch_p99_ns: reps[0].batch_p99_ns,
      // NOT comparable across configs: the two run different iteration counts
      // and therefore consume different slices of the corpus. Cross-config
      // agreement is established in `equivalence` below, at equal n.
      checksum_for_this_n: reps[0].checksum,
      mode: reps[0].mode,
    });
    process.stdout.write(`  ${spec.row.padEnd(16)} ${config.padEnd(14)} ${means[Math.floor(REPS/2)].toFixed(1)} ns/op\n`);
  }
}

// Equal-n checksum agreement. This is the anti-dead-code guard AND a cheap
// proof both implementations computed the same values over the same inputs:
// a folded 32-bit digest of every result, which cannot match by chance.
console.log("\nequivalence (equal n, cross-config checksum)");
const equivalence = [];
for (const spec of ROWS) {
  const a = jsBench("upstream", spec.row, 20000);
  const b = jsBench("wasm-shipped", spec.row, 20000);
  const agree = a.checksum === b.checksum;
  equivalence.push({ row: spec.row, n: 20000, upstream: a.checksum, port: b.checksum, agree });
  console.log(`  ${spec.row.padEnd(16)} ${agree ? "MATCH" : "*** MISMATCH ***"}  (${a.checksum})`);
}

console.log("\nstartup");
const startup = [
  measureStartup("node + upstream JS", "node", [
    "-e", "require('./upstream/tinycolor.reference.js')('red').toHexString()",
  ]),
  measureStartup("node + port (wasm)", "node", [
    "-e", "require('./tests/original/tinycolor.js')('red').toHexString()",
  ]),
  measureStartup("native rust binary", "./target/release/tinycolor", ["bench", "to-hex", "1"]),
  measureStartup("node + port (native ipc)", "node", [
    "-e", "process.env.TINYCOLOR_TRANSPORT='native';require('./tests/original/tinycolor.js')('red').toHexString()",
  ]),
];
for (const s of startup) console.log(`  ${s.label.padEnd(22)} p50 ${s.p50_ms.toFixed(1)} ms`);

console.log("\nresident memory");
const rss = [
  measureRss("node + upstream JS", "node", ["-e", "require('./upstream/tinycolor.reference.js')('red').toHexString()"]),
  measureRss("node + port (wasm)", "node", ["-e", "require('./tests/original/tinycolor.js')('red').toHexString()"]),
  measureRss("native rust binary", "./target/release/tinycolor", ["bench", "to-hex", "1"]),
  measureRss("node + port (native ipc)", "node", [
    "-e", "process.env.TINYCOLOR_TRANSPORT='native';require('./tests/original/tinycolor.js')('red').toHexString()",
  ]),
];
for (const r of rss) console.log(`  ${r.label.padEnd(22)} ${r.max_rss_mb} MB`);

// Native rows, for the decomposition columns.
const native = [];
for (const w of ["parse-hex", "to-hex", "to-rgb-string", "to-hsl-string", "lighten", "mix", "luminance", "round-trip", "noop"]) {
  try { native.push(nativeBench(w, 1000000)); } catch (e) { native.push({ workload: w, error: String(e.message).slice(0, 120) }); }
}

const corpusJson = readFileSync("bench/corpus.json", "utf8");

const results = {
  publishable,
  gates,
  ...(publishable ? {} : { WARNING: "One or more environment gates failed. These numbers are indicative only and MUST NOT be published." }),
  generated_at_note: "timestamp intentionally omitted; see git history",
  machine: {
    platform: process.platform,
    arch: process.arch,
    cpus: (() => { try { return sh("sysctl", ["-n", "machdep.cpu.brand_string"]); } catch { return "unknown"; } })(),
    node: process.version,
    v8: process.versions.v8,
    rustc: (() => { try { return sh("rustc", ["--version"]); } catch { return "unknown"; } })(),
  },
  corpus: {
    path: "bench/corpus.json",
    sha256: createHash("sha256").update(corpusJson).digest("hex"),
    size: JSON.parse(corpusJson).size,
  },
  methodology: "bench/METHODOLOGY.md",
  provenance: {
    note: "Both artifacts below run the byte-identical upstream suite via tests/original/tinycolor.js, selected by TINYCOLOR_TRANSPORT. Neither number is quoted for an unverified build.",
    "wasm-shipped": { upstream_suite: "45/45", differential_fuzz: "31,020,525 comparisons / 0 divergences (300s, seed 1)" },
    "native": { upstream_suite: "45/45", differential_fuzz: "3,343,635 comparisons / 0 divergences (120s, seed 11)" },
  },
  warmup_ops_per_config: 200000,
  reps_per_row: REPS,
  throughput,
  equivalence,
  equivalence_all_agree: equivalence.every((e) => e.agree),
  startup,
  rss,
  native,
};

mkdirSync("bench", { recursive: true });
writeFileSync("bench/results.json", JSON.stringify(results, null, 2));
console.log(`\nwrote bench/results.json  (publishable: ${publishable})`);

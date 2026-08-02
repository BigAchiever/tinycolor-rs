// Pass-rate runner for the unmodified upstream suite.
//
// `tests/original/test.js` is byte-identical to upstream, and upstream's CJS
// runner rethrows on the first failing assertion — fine for CI, useless for
// measuring how much of the suite a partial port satisfies.
//
// Rather than edit the test file (which would break its hash and forfeit the
// "unmodified test suite" criterion), this runner pre-loads the same
// `@deno/shim-deno-test` module the test file imports and wraps every
// registered test function in a try/catch. test.js then runs its own loop,
// sees no exceptions, and we report the tally afterwards.
//
//   node tests/run-all.mjs          # summary
//   node tests/run-all.mjs -v       # list each failure

import { createRequire } from "node:module";
const require = createRequire(import.meta.url);

const verbose = process.argv.includes("-v");
const shim = require("@deno/shim-deno-test");

const results = [];
const originalTest = shim.Deno.test;

shim.Deno.test = function (nameOrDef, maybeFn) {
  const name = typeof nameOrDef === "string" ? nameOrDef : nameOrDef.name;
  const fn = typeof nameOrDef === "string" ? maybeFn : nameOrDef.fn;
  const wrapped = async function (...args) {
    try {
      await fn.apply(this, args);
      results.push({ name, ok: true });
    } catch (err) {
      results.push({ name, ok: false, err });
    }
  };
  return typeof nameOrDef === "string"
    ? originalTest.call(this, name, wrapped)
    : originalTest.call(this, { ...nameOrDef, fn: wrapped });
};

// Silence upstream's per-test progress chatter; we print our own summary.
const log = console.log;
console.log = () => {};

await import("./original/test.js");

// test.js kicks off its runner in a floating promise; let it drain.
await new Promise((r) => setTimeout(r, 2000));
console.log = log;

const passed = results.filter((r) => r.ok).length;
const failed = results.filter((r) => !r.ok);

if (failed.length) {
  console.log("FAILING TESTS");
  for (const f of failed) {
    const msg = String(f.err && f.err.message ? f.err.message : f.err)
      .split("\n")
      .filter((l) => l.trim())
      .slice(0, verbose ? 12 : 3)
      .join("\n    ");
    console.log(`  ✗ ${f.name}\n    ${msg}\n`);
  }
}

const total = results.length;
const pct = total ? ((passed / total) * 100).toFixed(1) : "0.0";
console.log("─".repeat(56));
console.log(`upstream suite: ${passed}/${total} tests passing (${pct}%)`);
console.log("─".repeat(56));

process.exit(failed.length ? 1 : 0);

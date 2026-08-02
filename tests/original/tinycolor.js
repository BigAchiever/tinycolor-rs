// ---------------------------------------------------------------------------
// Test shim — NOT part of the port, and NOT part of the original.
//
// `tests/original/test.js` is the upstream test suite, byte-for-byte
// unmodified (sha256 in tests/HASHES.txt). Its only entry point to the
// library under test is `require("./tinycolor.js")` on line 5, so this file
// sits at exactly that path and presents the upstream API on top of the port.
//
// Two transports, selected by TINYCOLOR_TRANSPORT:
//
//   wasm   (default)  wasm-bindgen module, in-process
//   native            the release binary over a stdio JSON-RPC pipe
//
// Both drive the same `rpc::dispatch` in crates/core, so the *unmodified*
// upstream suite and the differential fuzzer can be run against either. That
// matters because the two builds are not interchangeable: D-012 showed they
// disagreed on floating point, and the benchmark shows they differ by an order
// of magnitude in startup and memory. Evidence gathered against one is not
// automatically a statement about the other. See DECISIONS.md D-020.
//
// The API surface itself lives in shim/api.js, shared with the browser demo.
// ---------------------------------------------------------------------------

const path = require("node:path");
const { buildTinyColorApi } = require("../../shim/api.js");

function wasmDispatch() {
  return require("./pkg/tinycolor_wasm.js").dispatch;
}

// Synchronous request/response over the child's stdio.
//
// `shim/api.js` needs `dispatch` to be synchronous (the upstream tests are
// synchronous and object identity depends on it), so this cannot use the
// stream API. Node does not expose `.fd` on pipe streams, but the underlying
// libuv handle does, and fs.readSync/writeSync work on it.
function nativeDispatch() {
  const { spawn } = require("node:child_process");
  const fs = require("node:fs");

  const bin =
    process.env.TINYCOLOR_BIN ||
    path.resolve(__dirname, "../../target/release/tinycolor");

  const child = spawn(bin, ["rpc"], { stdio: ["pipe", "pipe", "inherit"] });
  child.on("error", (e) => {
    throw new Error(`native port failed to start (${bin}): ${e.message}`);
  });

  const inFd = child.stdin._handle.fd;
  const outFd = child.stdout._handle.fd;

  // The child and its two pipes are libuv handles, and an active handle keeps
  // Node's event loop alive — so without this a process that merely *uses* the
  // library never exits. It looks like a hang with the work already done.
  //
  // Missed initially because both harnesses call process.exit() explicitly,
  // which papers over it. All I/O here is fs.readSync/writeSync on the raw
  // descriptors, so nothing needs these handles referenced. See D-021.
  child.unref();
  child.stdin.unref();
  child.stdout.unref();

  const buf = Buffer.alloc(1 << 20);
  let pending = "";

  const shutdown = () => {
    try { child.kill(); } catch (_) { /* already gone */ }
  };
  process.on("exit", shutdown);
  process.on("SIGINT", () => { shutdown(); process.exit(130); });

  return function dispatch(req) {
    fs.writeSync(inFd, req + "\n");
    for (;;) {
      const nl = pending.indexOf("\n");
      if (nl >= 0) {
        const line = pending.slice(0, nl);
        pending = pending.slice(nl + 1);
        return line;
      }
      let n = 0;
      try {
        n = fs.readSync(outFd, buf, 0, buf.length, null);
      } catch (e) {
        // The pipe is non-blocking; spin until the child answers.
        if (e.code === "EAGAIN") continue;
        throw e;
      }
      if (n === 0) {
        throw new Error(
          "native port closed the pipe (it may have panicked; note panic=abort)"
        );
      }
      pending += buf.toString("utf8", 0, n);
    }
  };
}

const TRANSPORT = process.env.TINYCOLOR_TRANSPORT === "native" ? "native" : "wasm";

const tinycolor = buildTinyColorApi(
  TRANSPORT === "native" ? nativeDispatch() : wasmDispatch()
);

// Lets the suite / fuzzer / bench state which artifact produced a result.
tinycolor.__transport = TRANSPORT;

module.exports = tinycolor;
module.exports.default = tinycolor;

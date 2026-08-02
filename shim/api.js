// Shared API layer for the port.
//
// Builds the upstream TinyColor surface on top of a single `dispatch(json)`
// function. Both transports use this unchanged:
//
//   tests/original/tinycolor.js  -> wasm-bindgen nodejs target (test suite)
//   demo/tinycolor.js           -> wasm-bindgen web target    (demo page)
//
// Nothing here implements colour behaviour. Its only jobs are object identity
// (`tinycolor(x) === x`), handle lifetime, and encoding values JSON cannot
// carry (NaN, +/-Infinity, -0).

function buildTinyColorApi(dispatch) {
  function rpc(request) {
    const res = JSON.parse(dispatch(JSON.stringify(request)));
    if (res && res.error) throw new Error("port rpc: " + res.error);
    return res;
  }

  function revive(v) {
    if (v === "NaN") return NaN;
    if (v === "Infinity") return Infinity;
    if (v === "-Infinity") return -Infinity;
    return v;
  }

  function reviveObj(o) {
    const out = {};
    for (const k of Object.keys(o)) out[k] = revive(o[k]);
    return out;
  }

  // `JSON.stringify(Infinity)` is "null" and `-0` stringifies to "0", so
  // values JSON cannot represent travel tagged and are decoded in rpc.rs.
  function encode(v) {
    if (v instanceof tinycolor) return { __id: v.__id };
    if (typeof v === "number") {
      if (!Number.isFinite(v)) return { __num: String(v) };
      if (Object.is(v, -0)) return { __num: "-0" };
      return v;
    }
    if (Array.isArray(v)) return v.map(encode);
    if (v && typeof v === "object") {
      const o = {};
      for (const k of Object.keys(v)) o[k] = encode(v[k]);
      return o;
    }
    return v;
  }

  function tinycolor(color, opts) {
    if (color instanceof tinycolor) return color;
    if (!(this instanceof tinycolor)) return new tinycolor(color, opts);

    opts = opts || {};
    const input = color ? color : ""; // `color = color ? color : ""`
    this._originalInput = input;
    this.__id = rpc({ op: "new", input: encode(input), opts }).id;
    track(this);
  }

  // Colours live in a Rust-side handle table, which JS garbage collection
  // knows nothing about. Without this, every colour ever constructed leaks
  // and the wasm heap eventually dies with "memory access out of bounds" —
  // it took ~4.6M colours in a 300s fuzz run. See DECISIONS.md D-018.
  const reaper =
    typeof FinalizationRegistry !== "undefined"
      ? new FinalizationRegistry((id) => {
          try {
            rpc({ op: "free", id });
          } catch (_) {
            /* shutting down */
          }
        })
      : null;

  function track(obj) {
    if (reaper) reaper.register(obj, obj.__id);
    return obj;
  }

  function fromHandle(id) {
    const c = Object.create(tinycolor.prototype);
    c.__id = id;
    c._originalInput = "";
    return track(c);
  }

  const call = (self, method, args) =>
    rpc({ op: "call", id: self.__id, method, args: (args || []).map(encode) });

  const CHAINING = [
    "setAlpha", "lighten", "brighten", "darken",
    "desaturate", "saturate", "greyscale", "spin",
  ];
  const SCALAR = [
    "isValid", "isDark", "isLight", "getFormat", "getAlpha",
    "getBrightness", "getLuminance", "toHex", "toHexString", "toHex8",
    "toHex8String", "toRgbString", "toHslString", "toHsvString",
    "toPercentageRgbString", "toName", "toFilter", "toString",
  ];
  const OBJECT = ["toRgb", "toHsl", "toHsv", "toPercentageRgb"];
  const COLOR = ["clone", "complement"];
  const LIST = [
    "analogous", "monochromatic", "splitcomplement",
    "triad", "tetrad", "polyad",
  ];

  tinycolor.prototype = {
    constructor: tinycolor,
    getOriginalInput: function () {
      return this._originalInput;
    },
  };

  for (const m of SCALAR) {
    tinycolor.prototype[m] = function (...a) { return revive(call(this, m, a)); };
  }
  for (const m of OBJECT) {
    tinycolor.prototype[m] = function (...a) { return reviveObj(call(this, m, a)); };
  }
  for (const m of CHAINING) {
    tinycolor.prototype[m] = function (...a) { call(this, m, a); return this; };
  }
  for (const m of COLOR) {
    tinycolor.prototype[m] = function (...a) { return fromHandle(call(this, m, a).id); };
  }
  for (const m of LIST) {
    tinycolor.prototype[m] = function (...a) { return call(this, m, a).ids.map(fromHandle); };
  }

  function statik(method, args, kind) {
    const res = rpc({ op: "static", method, args: args.map(encode) });
    if (kind === "color") return res === null ? null : fromHandle(res.id);
    return revive(res);
  }

  tinycolor.equals = (a, b) => statik("equals", [a, b]);
  tinycolor.readability = (a, b) => statik("readability", [a, b]);
  tinycolor.isReadable = (a, b, w) => statik("isReadable", [a, b, w]);
  tinycolor.mix = (a, b, amt) => statik("mix", [a, b, amt], "color");
  tinycolor.fromRatio = (c, o) => statik("fromRatio", [c, o], "color");
  tinycolor.mostReadable = (b, l, a) => statik("mostReadable", [b, l, a], "color");
  tinycolor.random = () => statik("random", [], "color");

  for (const m of CHAINING.slice(1)) {
    tinycolor[m] = (color, amount) => fromHandle(
      rpc({ op: "new", input: encode(color), opts: {} }).id
    )[m](amount);
  }
  for (const m of ["complement", ...LIST]) {
    tinycolor[m] = (color, ...rest) => tinycolor(color)[m](...rest);
  }

  tinycolor.names = rpc({ op: "static", method: "names", args: [] });

  // Harness hooks, not part of the upstream API.
  tinycolor.__seed = (n) => rpc({ op: "seed", seed: n });
  tinycolor.__reset = () => rpc({ op: "reset" });
  tinycolor.__liveHandles = () => rpc({ op: "stats" }).live;

  return tinycolor;
}

if (typeof module !== "undefined" && module.exports) {
  module.exports = { buildTinyColorApi };
}

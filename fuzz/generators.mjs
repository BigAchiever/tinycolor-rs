// Input generators for the differential harness.
//
// Uniformly random strings would spend almost all their budget on inputs that
// both implementations reject identically, which proves nothing. These
// generators are grammar-directed: they emit values drawn from the shapes
// TinyColor actually accepts, then deliberately push each shape past its
// boundaries (negative channels, >100% percentages, sub-1e-6 ratios, mixed
// separators) because that is where two implementations stop agreeing.
//
// Every generator takes the PRNG so a seed reproduces a run exactly.

export function makeRng(seed) {
  // mulberry32 — small, fast, and identical across runs for a given seed.
  let a = seed >>> 0;
  return function rng() {
    a |= 0;
    a = (a + 0x6d2b79f5) | 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

const HEX = "0123456789abcdefABCDEF";

const pick = (rng, arr) => arr[Math.floor(rng() * arr.length)];
const int = (rng, lo, hi) => Math.floor(rng() * (hi - lo + 1)) + lo;

function hexDigits(rng, n) {
  let s = "";
  for (let i = 0; i < n; i++) s += HEX[Math.floor(rng() * HEX.length)];
  return s;
}

// Numeric components, biased toward boundaries rather than the middle.
function channel(rng) {
  const r = rng();
  if (r < 0.55) return int(rng, 0, 255);
  if (r < 0.65) return pick(rng, [0, 1, 254, 255, 256, -1, -255, 511]);
  if (r < 0.8) return +(rng() * 255).toFixed(int(rng, 0, 6));
  if (r < 0.9) return pick(rng, [0.5, 0.999999, 1.000001, 127.5]);
  return pick(rng, [NaN, Infinity, -Infinity, 1e21, 1e-7, -0]);
}

// Percentage strings, including the sub-1e-6 range that trips D-011 and the
// "1.0" form that isOnePointZero reinterprets as "100%" (D-002).
function percentage(rng) {
  const r = rng();
  if (r < 0.5) return `${int(rng, 0, 100)}%`;
  if (r < 0.65) return `${+(rng() * 100).toFixed(int(rng, 0, 4))}%`;
  if (r < 0.8) return pick(rng, ["0%", "100%", "101%", "-1%", "1000%"]);
  if (r < 0.92)
    return pick(rng, ["0.0000001%", "0.000000001%", "1e-7%", "2.55e-7%", "0.0000255%"]);
  return pick(rng, ["1.0", "1.00", ".5", "1e2%", "+50%", "-0.0%"]);
}

const unit = (rng) => (rng() < 0.5 ? channel(rng) : percentage(rng));

// Separators: upstream's character classes contain a literal '|', so a pipe
// is an accepted delimiter (D-007). Included deliberately.
const SEPS = [", ", ",", " ", "  ", "|", ",|", " | ", "\t", ",\n"];
const sep = (rng) => pick(rng, SEPS);

function functional(rng, name, n) {
  const parts = [];
  for (let i = 0; i < n; i++) parts.push(unit(rng));
  const open = rng() < 0.85 ? "(" : rng() < 0.5 ? " " : "|";
  const close = rng() < 0.9 ? ")" : "";
  let s = name + open;
  parts.forEach((p, i) => {
    s += (i ? sep(rng) : "") + p;
  });
  return s + close;
}

export function makeGenerators(names) {
  const NAMES = Object.keys(names);

  return [
    ["hex3", (rng) => (rng() < 0.5 ? "#" : "") + hexDigits(rng, 3)],
    ["hex4", (rng) => (rng() < 0.5 ? "#" : "") + hexDigits(rng, 4)],
    ["hex6", (rng) => (rng() < 0.5 ? "#" : "") + hexDigits(rng, 6)],
    ["hex8", (rng) => (rng() < 0.5 ? "#" : "") + hexDigits(rng, 8)],
    ["name", (rng) => pick(rng, NAMES)],
    ["name-case", (rng) => mixCase(rng, pick(rng, NAMES))],
    ["transparent", () => "transparent"],
    ["rgb", (rng) => functional(rng, "rgb", 3)],
    ["rgba", (rng) => functional(rng, "rgba", 4)],
    ["hsl", (rng) => functional(rng, "hsl", 3)],
    ["hsla", (rng) => functional(rng, "hsla", 4)],
    ["hsv", (rng) => functional(rng, "hsv", 3)],
    ["hsva", (rng) => functional(rng, "hsva", 4)],
    ["obj-rgb", (rng) => ({ r: unit(rng), g: unit(rng), b: unit(rng) })],
    [
      "obj-rgba",
      (rng) => ({ r: unit(rng), g: unit(rng), b: unit(rng), a: alpha(rng) }),
    ],
    ["obj-hsl", (rng) => ({ h: unit(rng), s: unit(rng), l: unit(rng) })],
    ["obj-hsv", (rng) => ({ h: unit(rng), s: unit(rng), v: unit(rng) })],
    [
      "obj-partial",
      (rng) => {
        const o = { r: unit(rng), g: unit(rng), b: unit(rng) };
        delete o[pick(rng, ["r", "g", "b"])];
        return o;
      },
    ],
    ["whitespace", (rng) => pad(rng, hexDigits(rng, 6))],
    [
      "malformed",
      (rng) =>
        pick(rng, [
          "",
          " ",
          "#",
          "#f",
          "#ff",
          "#fffff",
          "not a color",
          "rgb()",
          "rgb(,,)",
          "hsl(0,0)",
          "rgb(1,2)",
          "rgb(1,2,3,4,5)",
          "0",
          "null",
          "undefined",
          "#GGG",
          "rgba(1,2,3,)",
        ]),
    ],
    [
      "non-string",
      (rng) => pick(rng, [null, undefined, false, 0, "", NaN, {}]),
    ],
  ];
}

function alpha(rng) {
  const r = rng();
  if (r < 0.4) return +rng().toFixed(2);
  if (r < 0.6) return pick(rng, [0, 1, 0.5, -0.1, 1.1, 2]);
  if (r < 0.8) return `${int(rng, 0, 100)}%`;
  return pick(rng, [NaN, Infinity, "abc", null]);
}

function mixCase(rng, s) {
  return s
    .split("")
    .map((c) => (rng() < 0.5 ? c.toUpperCase() : c))
    .join("");
}

function pad(rng, s) {
  const w = ["", " ", "  ", "\t", "\n", "   "];
  return pick(rng, w) + s + pick(rng, w);
}

/// Random argument for the modification methods.
export function amount(rng) {
  const r = rng();
  if (r < 0.5) return int(rng, 0, 100);
  if (r < 0.7) return +(rng() * 100).toFixed(3);
  if (r < 0.85) return pick(rng, [0, 100, -10, 200, 360, -360]);
  return pick(rng, [undefined, NaN, Infinity, 1e-9]);
}

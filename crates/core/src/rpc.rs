//! JSON-RPC bridge over the port.
//!
//! One protocol, two transports: the `wasm` crate exposes `dispatch` to Node
//! so the *unmodified* upstream test suite can drive the port in-process, and
//! the `cli` crate exposes the same protocol over stdio for the differential
//! fuzzer and the benchmarks.
//!
//! Colours live in a handle table rather than being serialised in and out,
//! so JS object identity and the mutating chain methods (`lighten` etc.)
//! behave as they do upstream.

use crate::color::{self as ops, TinyColor};
use crate::convert::Unit;
use crate::error::{Error, MethodKind, Result};
use crate::names::NAMES;
use crate::parse::{ColorObj, Input};
use serde_json::{json, Map, Value};
use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static SLAB: RefCell<HashMap<u64, TinyColor>> = RefCell::new(HashMap::new());
    static NEXT_ID: RefCell<u64> = const { RefCell::new(1) };
    static RNG: RefCell<u64> = const { RefCell::new(0x2545F4914F6CDD1D) };
}

fn store(c: TinyColor) -> u64 {
    let id = NEXT_ID.with(|n| {
        let mut n = n.borrow_mut();
        let id = *n;
        *n += 1;
        id
    });
    SLAB.with(|s| s.borrow_mut().insert(id, c));
    id
}

fn get(id: u64) -> Option<TinyColor> {
    SLAB.with(|s| s.borrow().get(&id).cloned())
}

fn put(id: u64, c: TinyColor) {
    SLAB.with(|s| {
        s.borrow_mut().insert(id, c);
    });
}

/// xorshift64* — deterministic so fuzz runs replay exactly. `seed` resets it.
fn next_rand() -> f64 {
    RNG.with(|r| {
        let mut x = *r.borrow();
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        *r.borrow_mut() = x;
        ((x.wrapping_mul(0x2545F4914F6CDD1D) >> 11) as f64) / ((1u64 << 53) as f64)
    })
}

// ---- JSON <-> port types ----------------------------------------------

/// Decodes the `{"__num": "..."}` tag the shim uses for values JSON cannot
/// carry: NaN, +/-Infinity and -0.
fn decode_tagged_num(v: &Value) -> Option<f64> {
    let s = v.as_object()?.get("__num")?.as_str()?;
    Some(match s {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "-0" => -0.0,
        other => crate::jsnum::to_number(other),
    })
}

fn to_unit(v: &Value) -> Option<Unit> {
    if let Some(n) = decode_tagged_num(v) {
        return Some(Unit::Num(n));
    }
    match v {
        Value::Number(n) => Some(Unit::Num(n.as_f64().unwrap_or(f64::NAN))),
        Value::String(s) => Some(Unit::Str(s.clone())),
        Value::Bool(b) => Some(Unit::Num(if *b { 1.0 } else { 0.0 })),
        Value::Null => Some(Unit::Num(f64::NAN)),
        _ => None,
    }
}

fn to_color_obj(m: &Map<String, Value>) -> ColorObj {
    let f = |k: &str| m.get(k).and_then(to_unit);
    ColorObj {
        r: f("r"),
        g: f("g"),
        b: f("b"),
        h: f("h"),
        s: f("s"),
        l: f("l"),
        v: f("v"),
        a: f("a"),
        format: m.get("format").and_then(|x| x.as_str()).map(String::from),
    }
}

fn to_input(v: &Value) -> Input {
    if let Some(n) = decode_tagged_num(v) {
        // A bare non-finite number as the colour argument is truthy unless
        // it is NaN or -0, both of which upstream coerces to "".
        return if n.is_nan() || n == 0.0 {
            Input::Str(String::new())
        } else {
            Input::Str(crate::jsnum::num_to_string(n))
        };
    }
    match v {
        Value::String(s) => Input::Str(s.clone()),
        Value::Object(m) => Input::Obj(to_color_obj(m)),
        // `color ? color : ""` — falsy inputs become the empty string.
        Value::Null | Value::Bool(false) => Input::Str(String::new()),
        Value::Number(n) => Input::Str(crate::jsnum::num_to_string(n.as_f64().unwrap_or(0.0))),
        _ => Input::Str(String::new()),
    }
}

/// JS numbers are all f64; NaN/Infinity are not representable in JSON, so
/// they cross the boundary as strings and the shim revives them.
fn num(x: f64) -> Value {
    if x.is_finite() {
        json!(x)
    } else if x.is_nan() {
        json!("NaN")
    } else if x > 0.0 {
        json!("Infinity")
    } else {
        json!("-Infinity")
    }
}

fn arg(args: &[Value], i: usize) -> Option<&Value> {
    args.get(i).filter(|v| !v.is_null())
}

fn arg_f64(args: &[Value], i: usize) -> Option<f64> {
    arg(args, i).and_then(|v| {
        if let Some(n) = decode_tagged_num(v) {
            return Some(n);
        }
        match v {
            Value::Number(n) => n.as_f64(),
            Value::String(s) => Some(crate::jsnum::parse_float(s)),
            _ => None,
        }
    })
}

fn arg_bool(args: &[Value], i: usize) -> bool {
    matches!(args.get(i), Some(Value::Bool(true)))
}

fn arg_str(args: &[Value], i: usize) -> Option<String> {
    arg(args, i)
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn color_from_arg(v: &Value) -> TinyColor {
    #[allow(clippy::items_after_statements)]
    // A handle reference `{"__id":n}` means "an existing TinyColor".
    if let Value::Object(m) = v {
        if let Some(id) = m.get("__id").and_then(|x| x.as_u64()) {
            if let Some(c) = get(id) {
                return c;
            }
        }
    }
    TinyColor::new(to_input(v))
}

fn ids(list: Vec<TinyColor>) -> Value {
    json!({ "ids": list.into_iter().map(store).collect::<Vec<_>>() })
}

// ---- dispatch ----------------------------------------------------------

pub fn dispatch(request: &str) -> String {
    let v: Value = match serde_json::from_str(request) {
        Ok(v) => v,
        Err(e) => return json!({ "error": Error::MalformedRequest(e.to_string()).to_string() }).to_string(),
    };
    match handle(&v) {
        Ok(v) => v.to_string(),
        Err(e) => json!({ "error": e.to_string() }).to_string(),
    }
}

fn handle(v: &Value) -> Result<Value> {
    let op = v.get("op").and_then(|x| x.as_str()).unwrap_or("");
    let empty: Vec<Value> = vec![];
    let args = v
        .get("args")
        .and_then(|x| x.as_array())
        .unwrap_or(&empty)
        .clone();

    match op {
        "new" => {
            let input = to_input(v.get("input").unwrap_or(&Value::Null));
            let opts = v.get("opts");
            let format = opts
                .and_then(|o| o.get("format"))
                .and_then(|x| x.as_str())
                .map(String::from);
            let gradient = opts
                .and_then(|o| o.get("gradientType"))
                .map(|x| !matches!(x, Value::Null | Value::Bool(false)))
                .unwrap_or(false);
            Ok(json!({ "id": store(TinyColor::with_opts(input, format, gradient)) }))
        }
        "free" => {
            if let Some(id) = v.get("id").and_then(|x| x.as_u64()) {
                SLAB.with(|s| s.borrow_mut().remove(&id));
            }
            Ok(json!({ "ok": true }))
        }
        "reset" => {
            // Drops every live handle. Test/fuzz harness use only: any JS
            // wrapper still holding an id becomes dangling after this.
            //
            // Needed because `FinalizationRegistry` reclamation is tied to GC
            // timing, and a tight fuzz loop allocates far faster than the
            // collector runs. See DECISIONS.md D-018.
            SLAB.with(|s| s.borrow_mut().clear());
            Ok(json!({ "ok": true }))
        }
        "stats" => Ok(json!({ "live": SLAB.with(|s| s.borrow().len()) })),
        "seed" => {
            let s = v.get("seed").and_then(|x| x.as_u64()).unwrap_or(1);
            RNG.with(|r| *r.borrow_mut() = s | 1);
            Ok(json!({ "ok": true }))
        }
        "call" => {
            let id = v
                .get("id")
                .and_then(|x| x.as_u64())
                .ok_or(Error::BadRequest("call requires an id"))?;
            let mut c = get(id).ok_or(Error::UnknownHandle(id))?;
            let method = v.get("method").and_then(|x| x.as_str()).unwrap_or("");
            call(id, &mut c, method, &args)
        }
        "static" => {
            let method = v.get("method").and_then(|x| x.as_str()).unwrap_or("");
            statics(method, &args)
        }
        other => Err(Error::UnknownOp(other.to_string())),
    }
}

fn call(id: u64, c: &mut TinyColor, method: &str, args: &[Value]) -> Result<Value> {
    // Mutating, chainable methods write back into the same handle.
    let chain = |c: &TinyColor| {
        put(id, c.clone());
        Ok(json!({ "id": id }))
    };

    match method {
        "isValid" => Ok(json!(c.is_valid())),
        "isDark" => Ok(json!(c.is_dark())),
        "isLight" => Ok(json!(c.is_light())),
        "getFormat" => Ok(c.get_format().map(Value::String).unwrap_or(json!(false))),
        "getAlpha" => Ok(num(c.get_alpha())),
        "getBrightness" => Ok(num(c.get_brightness())),
        "getLuminance" => Ok(num(c.get_luminance())),
        "toHex" => Ok(json!(c.to_hex(arg_bool(args, 0)))),
        "toHexString" => Ok(json!(c.to_hex_string(arg_bool(args, 0)))),
        "toHex8" => Ok(json!(c.to_hex8(arg_bool(args, 0)))),
        "toHex8String" => Ok(json!(c.to_hex8_string(arg_bool(args, 0)))),
        "toRgbString" => Ok(json!(c.to_rgb_string())),
        "toHslString" => Ok(json!(c.to_hsl_string())),
        "toHsvString" => Ok(json!(c.to_hsv_string())),
        "toPercentageRgbString" => Ok(json!(c.to_percentage_rgb_string())),
        "toName" => Ok(c.to_name().map(Value::String).unwrap_or(json!(false))),
        "toString" => Ok(json!(c.to_string(arg_str(args, 0).as_deref()))),
        "toRgb" => {
            let (r, g, b, a) = c.to_rgb();
            Ok(json!({"r": num(r), "g": num(g), "b": num(b), "a": num(a)}))
        }
        "toHsl" => {
            let (h, s, l, a) = c.to_hsl();
            Ok(json!({"h": num(h), "s": num(s), "l": num(l), "a": num(a)}))
        }
        "toHsv" => {
            let (h, s, v, a) = c.to_hsv();
            Ok(json!({"h": num(h), "s": num(s), "v": num(v), "a": num(a)}))
        }
        "toPercentageRgb" => {
            let (r, g, b, a) = c.to_percentage_rgb();
            Ok(json!({"r": r, "g": g, "b": b, "a": num(a)}))
        }
        "toFilter" => {
            let second = arg(args, 0).map(color_from_arg);
            Ok(json!(c.to_filter(second.as_ref())))
        }
        "setAlpha" => {
            let u = arg(args, 0).and_then(to_unit).unwrap_or(Unit::Num(f64::NAN));
            c.set_alpha(&u);
            chain(c)
        }
        "clone" => Ok(json!({ "id": store(c.clone_color()) })),

        "lighten" => {
            c.lighten(arg_f64(args, 0));
            chain(c)
        }
        "brighten" => {
            c.brighten(arg_f64(args, 0));
            chain(c)
        }
        "darken" => {
            c.darken(arg_f64(args, 0));
            chain(c)
        }
        "saturate" => {
            c.saturate(arg_f64(args, 0));
            chain(c)
        }
        "desaturate" => {
            c.desaturate(arg_f64(args, 0));
            chain(c)
        }
        "greyscale" => {
            c.greyscale();
            chain(c)
        }
        "spin" => {
            c.spin(arg_f64(args, 0).unwrap_or(f64::NAN));
            chain(c)
        }

        "complement" => Ok(json!({ "id": store(ops::complement(c)) })),
        "analogous" => Ok(ids(ops::analogous(c, arg_f64(args, 0), arg_f64(args, 1)))),
        "monochromatic" => Ok(ids(ops::monochromatic(c, arg_f64(args, 0)))),
        "splitcomplement" => Ok(ids(ops::splitcomplement(c))),
        "triad" => Ok(ids(ops::polyad(c, 3.0)?)),
        "tetrad" => Ok(ids(ops::polyad(c, 4.0)?)),
        "polyad" => {
            let n = arg_f64(args, 0).unwrap_or(f64::NAN);
            Ok(ids(ops::polyad(c, n)?))
        }
        other => Err(Error::UnknownMethod { kind: MethodKind::Instance, name: other.to_string() }),
    }
}

fn statics(method: &str, args: &[Value]) -> Result<Value> {
    match method {
        "equals" => {
            // `!color1 || !color2 ? false : ...` — the full JS falsy set:
            // false, 0, -0, "", null, undefined, NaN.
            let falsy = |v: Option<&Value>| match v {
                None | Some(Value::Null) | Some(Value::Bool(false)) => true,
                Some(Value::String(s)) => s.is_empty(),
                Some(Value::Number(n)) => match n.as_f64() {
                    Some(x) => x == 0.0 || x.is_nan(),
                    None => true,
                },
                Some(v) => match decode_tagged_num(v) {
                    Some(x) => x == 0.0 || x.is_nan(),
                    None => false,
                },
            };
            if falsy(args.first()) || falsy(args.get(1)) {
                return Ok(json!(false));
            }
            Ok(json!(ops::equals(
                &color_from_arg(&args[0]),
                &color_from_arg(&args[1])
            )))
        }
        "mix" => {
            let c = ops::mix(
                &color_from_arg(args.first().unwrap_or(&Value::Null)),
                &color_from_arg(args.get(1).unwrap_or(&Value::Null)),
                arg_f64(args, 2),
            );
            Ok(json!({ "id": store(c) }))
        }
        "readability" => Ok(num(ops::readability(
            &color_from_arg(args.first().unwrap_or(&Value::Null)),
            &color_from_arg(args.get(1).unwrap_or(&Value::Null)),
        ))),
        "isReadable" => {
            let o = args.get(2);
            let level = o
                .and_then(|x| x.get("level"))
                .and_then(|x| x.as_str())
                .unwrap_or("AA");
            let size = o
                .and_then(|x| x.get("size"))
                .and_then(|x| x.as_str())
                .unwrap_or("small");
            Ok(json!(ops::is_readable(
                &color_from_arg(args.first().unwrap_or(&Value::Null)),
                &color_from_arg(args.get(1).unwrap_or(&Value::Null)),
                level,
                size
            )))
        }
        "mostReadable" => {
            let base = color_from_arg(args.first().unwrap_or(&Value::Null));
            let list: Vec<TinyColor> = args
                .get(1)
                .and_then(|x| x.as_array())
                .map(|a| a.iter().map(color_from_arg).collect())
                .unwrap_or_default();
            let o = args.get(2);
            let level = o
                .and_then(|x| x.get("level"))
                .and_then(|x| x.as_str())
                .unwrap_or("AA");
            let size = o
                .and_then(|x| x.get("size"))
                .and_then(|x| x.as_str())
                .unwrap_or("small");
            let fallbacks = o
                .and_then(|x| x.get("includeFallbackColors"))
                .map(|x| matches!(x, Value::Bool(true)))
                .unwrap_or(false);

            let mut best: Option<TinyColor> = None;
            let mut best_score = 0.0;
            for c in &list {
                let r = ops::readability(&base, c);
                if r > best_score {
                    best_score = r;
                    best = Some(c.clone());
                }
            }
            // `isReadable(base, null)` treats a missing best as black.
            let cand = best.clone().unwrap_or_else(|| TinyColor::from_str(""));
            if ops::is_readable(&base, &cand, level, size) || !fallbacks {
                return Ok(match best {
                    Some(c) => json!({ "id": store(c) }),
                    None => json!(null),
                });
            }
            let white = TinyColor::from_str("#fff");
            let black = TinyColor::from_str("#000");
            let pick = if ops::readability(&base, &white) > ops::readability(&base, &black) {
                white
            } else {
                black
            };
            Ok(json!({ "id": store(pick) }))
        }
        "fromRatio" => {
            let obj = match args.first() {
                Some(Value::Object(m)) => to_color_obj(m),
                other => {
                    let c = TinyColor::new(to_input(other.unwrap_or(&Value::Null)));
                    return Ok(json!({ "id": store(c) }));
                }
            };
            let format = args
                .get(1)
                .and_then(|o| o.get("format"))
                .and_then(|x| x.as_str())
                .map(String::from);
            Ok(json!({ "id": store(ops::from_ratio(&obj, format)) }))
        }
        "random" => {
            // Upstream is `fromRatio({r: Math.random(), ...})`, so the ratio
            // conversion is what gives a random colour the "prgb" format.
            let obj = ColorObj {
                r: Some(Unit::Num(next_rand())),
                g: Some(Unit::Num(next_rand())),
                b: Some(Unit::Num(next_rand())),
                ..Default::default()
            };
            Ok(json!({ "id": store(ops::from_ratio(&obj, None)) }))
        }
        "names" => {
            let mut m = Map::new();
            for (k, v) in NAMES {
                m.insert((*k).to_string(), json!(v));
            }
            Ok(Value::Object(m))
        }
        other => Err(Error::UnknownMethod { kind: MethodKind::Static, name: other.to_string() }),
    }
}

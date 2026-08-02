//! CSS colour-string parsing — port of `matchers`, `stringInputToObject`,
//! `isValidCSSUnit` and `inputToRGB`.

use crate::convert::{
    bound_alpha, convert_hex_to_decimal, convert_to_percentage, hsl_to_rgb, hsv_to_rgb, rgb_to_rgb,
    Unit,
};
use crate::jsnum::{num_to_string, parse_int_radix};
use crate::names::NAME_MAP;
use once_cell::sync::Lazy;
use regex::Regex;

/// A colour object as accepted by `tinycolor()`. Mirrors the duck-typed JS
/// object: any subset of keys may be present.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ColorObj {
    pub r: Option<Unit>,
    pub g: Option<Unit>,
    pub b: Option<Unit>,
    pub h: Option<Unit>,
    pub s: Option<Unit>,
    pub l: Option<Unit>,
    pub v: Option<Unit>,
    pub a: Option<Unit>,
    pub format: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Input {
    Str(String),
    Obj(ColorObj),
}

impl Input {
    pub fn str(s: impl Into<String>) -> Self {
        Input::Str(s.into())
    }
}

const CSS_INTEGER: &str = r"[-\+]?\d+%?";
const CSS_NUMBER: &str = r"[-\+]?\d*\.\d+%?";

fn css_unit() -> String {
    format!("(?:{CSS_NUMBER})|(?:{CSS_INTEGER})")
}

/// Note the separator classes: upstream writes `[\s|\(]+` and `[,|\s]+`,
/// which place a literal `|` *inside* a character class. A pipe is therefore
/// an accepted delimiter — `tinycolor("rgb|255|0|0")` parses successfully.
/// Almost certainly an escaping slip upstream; the port reproduces it and
/// DECISIONS.md D-007 records it as a candidate upstream defect.
fn permissive_match(n: usize) -> String {
    let u = css_unit();
    let mut s = format!(r"[\s|\(]+({u})");
    for _ in 1..n {
        s.push_str(&format!(r"[,|\s]+({u})"));
    }
    s.push_str(r"\s*\)?");
    s
}

struct Matchers {
    css_unit: Regex,
    rgb: Regex,
    rgba: Regex,
    hsl: Regex,
    hsla: Regex,
    hsv: Regex,
    hsva: Regex,
    hex3: Regex,
    hex6: Regex,
    hex4: Regex,
    hex8: Regex,
}

static M: Lazy<Matchers> = Lazy::new(|| {
    let m3 = permissive_match(3);
    let m4 = permissive_match(4);
    Matchers {
        css_unit: Regex::new(&css_unit()).unwrap(),
        rgb: Regex::new(&format!("rgb{m3}")).unwrap(),
        rgba: Regex::new(&format!("rgba{m4}")).unwrap(),
        hsl: Regex::new(&format!("hsl{m3}")).unwrap(),
        hsla: Regex::new(&format!("hsla{m4}")).unwrap(),
        hsv: Regex::new(&format!("hsv{m3}")).unwrap(),
        hsva: Regex::new(&format!("hsva{m4}")).unwrap(),
        hex3: Regex::new(r"^#?([0-9a-fA-F]{1})([0-9a-fA-F]{1})([0-9a-fA-F]{1})$").unwrap(),
        hex6: Regex::new(r"^#?([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})$").unwrap(),
        hex4: Regex::new(
            r"^#?([0-9a-fA-F]{1})([0-9a-fA-F]{1})([0-9a-fA-F]{1})([0-9a-fA-F]{1})$",
        )
        .unwrap(),
        hex8: Regex::new(
            r"^#?([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})([0-9a-fA-F]{2})$",
        )
        .unwrap(),
    }
});

/// `isValidCSSUnit(n)` = `!!matchers.CSS_UNIT.exec(color)`.
/// The value is stringified first, so numbers are tested via their JS repr.
pub fn is_valid_css_unit(u: &Option<Unit>) -> bool {
    match u {
        None => false,
        Some(Unit::Num(n)) => M.css_unit.is_match(&num_to_string(*n)),
        Some(Unit::Str(s)) => M.css_unit.is_match(s),
    }
}

/// Upstream is `names[color]` — an O(1) object lookup. This was a linear
/// `NAMES.iter().find(...)` scan, which made cost depend on alphabetical
/// position (`aliceblue` cheap, `yellowgreen` 149 comparisons). See D-019.
fn lookup_name(c: &str) -> Option<&'static str> {
    NAME_MAP.get(c).copied()
}

/// `stringInputToObject` — returns `None` where upstream returns `false`.
pub fn string_input_to_object(color: &str) -> Option<ColorObj> {
    let mut color = color.trim().to_lowercase();
    let mut named = false;

    if let Some(hex) = lookup_name(&color) {
        color = hex.to_string();
        named = true;
    } else if color == "transparent" {
        return Some(ColorObj {
            r: Some(Unit::Num(0.0)),
            g: Some(Unit::Num(0.0)),
            b: Some(Unit::Num(0.0)),
            a: Some(Unit::Num(0.0)),
            format: Some("name".into()),
            ..Default::default()
        });
    }

    // One `captures()` per matcher, not `is_match()` followed by a fresh
    // `captures()` per capture group. The previous shape ran the rgb regex
    // four times for a single successful parse — once to test, then once per
    // component — and every earlier matcher in the chain twice more. See D-019.
    fn groups(re: &Regex, hay: &str, n: usize) -> Option<Vec<Unit>> {
        let c = re.captures(hay)?;
        let mut out = Vec::with_capacity(n);
        for i in 1..=n {
            out.push(Unit::Str(c.get(i)?.as_str().to_string()));
        }
        Some(out)
    }

    // Order is significant and matches upstream exactly.
    if let Some(g) = groups(&M.rgb, &color, 3) {
        let mut it = g.into_iter();
        return Some(ColorObj {
            r: it.next(),
            g: it.next(),
            b: it.next(),
            ..Default::default()
        });
    }
    if let Some(g) = groups(&M.rgba, &color, 4) {
        let mut it = g.into_iter();
        return Some(ColorObj {
            r: it.next(),
            g: it.next(),
            b: it.next(),
            a: it.next(),
            ..Default::default()
        });
    }
    if let Some(g) = groups(&M.hsl, &color, 3) {
        let mut it = g.into_iter();
        return Some(ColorObj {
            h: it.next(),
            s: it.next(),
            l: it.next(),
            ..Default::default()
        });
    }
    if let Some(g) = groups(&M.hsla, &color, 4) {
        let mut it = g.into_iter();
        return Some(ColorObj {
            h: it.next(),
            s: it.next(),
            l: it.next(),
            a: it.next(),
            ..Default::default()
        });
    }
    if let Some(g) = groups(&M.hsv, &color, 3) {
        let mut it = g.into_iter();
        return Some(ColorObj {
            h: it.next(),
            s: it.next(),
            v: it.next(),
            ..Default::default()
        });
    }
    if let Some(g) = groups(&M.hsva, &color, 4) {
        let mut it = g.into_iter();
        return Some(ColorObj {
            h: it.next(),
            s: it.next(),
            v: it.next(),
            a: it.next(),
            ..Default::default()
        });
    }

    let hex_fmt = |named: bool, f: &str| Some(if named { "name".to_string() } else { f.into() });

    if let Some(c) = M.hex8.captures(&color) {
        let g = |i: usize| c.get(i).unwrap().as_str();
        return Some(ColorObj {
            r: Some(Unit::Num(parse_int_radix(g(1), 16))),
            g: Some(Unit::Num(parse_int_radix(g(2), 16))),
            b: Some(Unit::Num(parse_int_radix(g(3), 16))),
            a: Some(Unit::Num(convert_hex_to_decimal(g(4)))),
            format: hex_fmt(named, "hex8"),
            ..Default::default()
        });
    }
    if let Some(c) = M.hex6.captures(&color) {
        let g = |i: usize| c.get(i).unwrap().as_str();
        return Some(ColorObj {
            r: Some(Unit::Num(parse_int_radix(g(1), 16))),
            g: Some(Unit::Num(parse_int_radix(g(2), 16))),
            b: Some(Unit::Num(parse_int_radix(g(3), 16))),
            format: hex_fmt(named, "hex"),
            ..Default::default()
        });
    }
    if let Some(c) = M.hex4.captures(&color) {
        let d = |i: usize| {
            let s = c.get(i).unwrap().as_str();
            format!("{s}{s}")
        };
        return Some(ColorObj {
            r: Some(Unit::Num(parse_int_radix(&d(1), 16))),
            g: Some(Unit::Num(parse_int_radix(&d(2), 16))),
            b: Some(Unit::Num(parse_int_radix(&d(3), 16))),
            a: Some(Unit::Num(convert_hex_to_decimal(&d(4)))),
            format: hex_fmt(named, "hex8"),
            ..Default::default()
        });
    }
    if let Some(c) = M.hex3.captures(&color) {
        let d = |i: usize| {
            let s = c.get(i).unwrap().as_str();
            format!("{s}{s}")
        };
        return Some(ColorObj {
            r: Some(Unit::Num(parse_int_radix(&d(1), 16))),
            g: Some(Unit::Num(parse_int_radix(&d(2), 16))),
            b: Some(Unit::Num(parse_int_radix(&d(3), 16))),
            format: hex_fmt(named, "hex"),
            ..Default::default()
        });
    }

    None
}

pub struct RgbaResult {
    pub ok: bool,
    pub format: Option<String>,
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
}

/// `inputToRGB`
pub fn input_to_rgb(input: &Input) -> RgbaResult {
    let obj: ColorObj = match input {
        Input::Str(s) => string_input_to_object(s).unwrap_or_default(),
        Input::Obj(o) => o.clone(),
    };

    let mut r = 0.0;
    let mut g = 0.0;
    let mut b = 0.0;
    let mut ok = false;
    let mut format: Option<String> = None;

    if is_valid_css_unit(&obj.r) && is_valid_css_unit(&obj.g) && is_valid_css_unit(&obj.b) {
        let c = rgb_to_rgb(
            obj.r.as_ref().unwrap(),
            obj.g.as_ref().unwrap(),
            obj.b.as_ref().unwrap(),
        );
        r = c.r;
        g = c.g;
        b = c.b;
        ok = true;
        // `String(color.r).substr(-1) === "%"`
        let rs = match obj.r.as_ref().unwrap() {
            Unit::Num(n) => num_to_string(*n),
            Unit::Str(s) => s.clone(),
        };
        format = Some(if rs.ends_with('%') { "prgb" } else { "rgb" }.into());
    } else if is_valid_css_unit(&obj.h) && is_valid_css_unit(&obj.s) && is_valid_css_unit(&obj.v) {
        let s = convert_to_percentage(obj.s.as_ref().unwrap());
        let v = convert_to_percentage(obj.v.as_ref().unwrap());
        let c = hsv_to_rgb(obj.h.as_ref().unwrap(), &s, &v);
        r = c.r;
        g = c.g;
        b = c.b;
        ok = true;
        format = Some("hsv".into());
    } else if is_valid_css_unit(&obj.h) && is_valid_css_unit(&obj.s) && is_valid_css_unit(&obj.l) {
        let s = convert_to_percentage(obj.s.as_ref().unwrap());
        let l = convert_to_percentage(obj.l.as_ref().unwrap());
        let c = hsl_to_rgb(obj.h.as_ref().unwrap(), &s, &l);
        r = c.r;
        g = c.g;
        b = c.b;
        ok = true;
        format = Some("hsl".into());
    }

    let a = match &obj.a {
        Some(u) => bound_alpha(u),
        None => 1.0,
    };

    RgbaResult {
        ok,
        // `color.format || format` — an explicit format on the object wins.
        format: obj.format.clone().or(format),
        r: r.max(0.0).min(255.0),
        g: g.max(0.0).min(255.0),
        b: b.max(0.0).min(255.0),
        a,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_named_and_hex() {
        let c = input_to_rgb(&Input::str("red"));
        assert_eq!((c.r, c.g, c.b), (255.0, 0.0, 0.0));
        assert_eq!(c.format.as_deref(), Some("name"));

        let c = input_to_rgb(&Input::str("#ff0000"));
        assert_eq!((c.r, c.g, c.b), (255.0, 0.0, 0.0));
        assert_eq!(c.format.as_deref(), Some("hex"));
    }

    #[test]
    fn parses_functional_notation() {
        let c = input_to_rgb(&Input::str("rgb(255, 0, 0)"));
        assert_eq!((c.r, c.g, c.b), (255.0, 0.0, 0.0));

        let c = input_to_rgb(&Input::str("hsl(0, 100%, 50%)"));
        assert_eq!(c.r.round(), 255.0);
    }

    #[test]
    fn pipe_separator_quirk_is_reproduced() {
        // See D-007: upstream's separator class contains a literal '|'.
        let c = input_to_rgb(&Input::str("rgb|255|0|0"));
        assert!(c.ok);
        assert_eq!((c.r, c.g, c.b), (255.0, 0.0, 0.0));
    }

    #[test]
    fn transparent_is_special_cased() {
        let c = input_to_rgb(&Input::str("transparent"));
        assert_eq!(c.a, 0.0);
        assert_eq!(c.format.as_deref(), Some("name"));
    }
}

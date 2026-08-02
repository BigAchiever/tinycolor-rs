//! Colour-space conversions, ported 1:1 from upstream `mod.js`.
//!
//! Every function here mirrors an upstream function of the same name. Where
//! upstream relies on a JS-specific behaviour the port calls into `jsnum`
//! rather than the Rust equivalent.

use crate::jsnum::{
    is_one_point_zero, is_percentage, math_round, num_to_string, parse_float,
    parse_int_from_number, parse_int_radix, to_number,
};

/// A CSS component as TinyColor sees it: either a JS number or a JS string.
///
/// The distinction is load-bearing. `bound01` and `isOnePointZero` branch on
/// `typeof n == "string"`, so `"1.0"` and `1.0` produce different results
/// (`"1.0"` is reinterpreted as `"100%"`). Collapsing both into `f64` here
/// would silently change behaviour — see DECISIONS.md D-002.
#[derive(Clone, Debug, PartialEq)]
pub enum Unit {
    Num(f64),
    Str(String),
}

impl Unit {
    pub fn from_str_unit(s: impl Into<String>) -> Self {
        Unit::Str(s.into())
    }

    /// `parseFloat(n)` where n may already be a number.
    pub fn as_f64(&self) -> f64 {
        match self {
            Unit::Num(n) => *n,
            Unit::Str(s) => parse_float(s),
        }
    }

    /// `ToNumber(n)` — what JS uses for `<=` and `*`, not `parseFloat`.
    pub fn to_number(&self) -> f64 {
        match self {
            Unit::Num(n) => *n,
            Unit::Str(s) => to_number(s),
        }
    }

    fn is_one_point_zero(&self) -> bool {
        match self {
            Unit::Num(_) => false, // typeof check excludes numbers
            Unit::Str(s) => is_one_point_zero(s),
        }
    }

    fn is_percentage(&self) -> bool {
        match self {
            Unit::Num(_) => false,
            Unit::Str(s) => is_percentage(s),
        }
    }
}

impl From<f64> for Unit {
    fn from(n: f64) -> Self {
        Unit::Num(n)
    }
}

/// `bound01(n, max)` — normalise a component into [0, 1].
///
/// Faithful to upstream including the `parseInt(n * max, 10) / 100` step,
/// which is a stringify-then-prefix-parse, *not* a truncation. See
/// `jsnum::parse_int_from_number` and DECISIONS.md D-011.
pub fn bound01(n: &Unit, max: f64) -> f64 {
    let mut n = n.clone();
    if n.is_one_point_zero() {
        n = Unit::Str("100%".to_string());
    }

    let process_percent = n.is_percentage();
    let mut v = max.min(parse_float(&match &n {
        Unit::Num(x) => num_to_string(*x),
        Unit::Str(s) => s.clone(),
    })
    .max(0.0));

    if process_percent {
        v = parse_int_from_number(v * max) / 100.0;
    }

    // Upstream's float-slop guard.
    if (v - max).abs() < 0.000001 {
        return 1.0;
    }

    (v % max) / max
}

/// `boundAlpha(a)` — anything outside [0, 1] or non-numeric becomes 1.
pub fn bound_alpha(a: &Unit) -> f64 {
    let v = a.as_f64();
    if v.is_nan() || v < 0.0 || v > 1.0 {
        1.0
    } else {
        v
    }
}

pub struct Rgb {
    pub r: f64,
    pub g: f64,
    pub b: f64,
}
pub struct Hsl {
    pub h: f64,
    pub s: f64,
    pub l: f64,
}
pub struct Hsv {
    pub h: f64,
    pub s: f64,
    pub v: f64,
}

pub fn rgb_to_rgb(r: &Unit, g: &Unit, b: &Unit) -> Rgb {
    Rgb {
        r: bound01(r, 255.0) * 255.0,
        g: bound01(g, 255.0) * 255.0,
        b: bound01(b, 255.0) * 255.0,
    }
}

pub fn rgb_to_hsl(r: &Unit, g: &Unit, b: &Unit) -> Hsl {
    let r = bound01(r, 255.0);
    let g = bound01(g, 255.0);
    let b = bound01(b, 255.0);

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;

    if max == min {
        return Hsl { h: 0.0, s: 0.0, l };
    }

    let d = max - min;
    let s = if l > 0.5 {
        d / (2.0 - max - min)
    } else {
        d / (max + min)
    };

    // `switch (max) { case r: ... case g: ... case b: }` — first match wins,
    // which matters when two channels tie for the maximum.
    let h = if max == r {
        (g - b) / d + if g < b { 6.0 } else { 0.0 }
    } else if max == g {
        (b - r) / d + 2.0
    } else {
        (r - g) / d + 4.0
    } / 6.0;

    Hsl { h, s, l }
}

pub fn hsl_to_rgb(h: &Unit, s: &Unit, l: &Unit) -> Rgb {
    let h = bound01(h, 360.0);
    let s = bound01(s, 100.0);
    let l = bound01(l, 100.0);

    fn hue2rgb(p: f64, q: f64, mut t: f64) -> f64 {
        if t < 0.0 {
            t += 1.0;
        }
        if t > 1.0 {
            t -= 1.0;
        }
        if t < 1.0 / 6.0 {
            return p + (q - p) * 6.0 * t;
        }
        if t < 1.0 / 2.0 {
            return q;
        }
        if t < 2.0 / 3.0 {
            return p + (q - p) * (2.0 / 3.0 - t) * 6.0;
        }
        p
    }

    let (r, g, b) = if s == 0.0 {
        (l, l, l)
    } else {
        let q = if l < 0.5 { l * (1.0 + s) } else { l + s - l * s };
        let p = 2.0 * l - q;
        (
            hue2rgb(p, q, h + 1.0 / 3.0),
            hue2rgb(p, q, h),
            hue2rgb(p, q, h - 1.0 / 3.0),
        )
    };

    Rgb {
        r: r * 255.0,
        g: g * 255.0,
        b: b * 255.0,
    }
}

pub fn rgb_to_hsv(r: &Unit, g: &Unit, b: &Unit) -> Hsv {
    let r = bound01(r, 255.0);
    let g = bound01(g, 255.0);
    let b = bound01(b, 255.0);

    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let v = max;
    let d = max - min;
    let s = if max == 0.0 { 0.0 } else { d / max };

    let h = if max == min {
        0.0
    } else {
        (if max == r {
            (g - b) / d + if g < b { 6.0 } else { 0.0 }
        } else if max == g {
            (b - r) / d + 2.0
        } else {
            (r - g) / d + 4.0
        }) / 6.0
    };

    Hsv { h, s, v }
}

pub fn hsv_to_rgb(h: &Unit, s: &Unit, v: &Unit) -> Rgb {
    let h = bound01(h, 360.0) * 6.0;
    let s = bound01(s, 100.0);
    let v = bound01(v, 100.0);

    let i = h.floor();
    let f = h - i;
    let p = v * (1.0 - s);
    let q = v * (1.0 - f * s);
    let t = v * (1.0 - (1.0 - f) * s);
    let m = ((i % 6.0) + 6.0) % 6.0;
    let m = m as usize % 6;

    let r = [v, q, p, p, t, v][m];
    let g = [t, v, v, q, p, p][m];
    let b = [p, p, t, v, v, q][m];

    Rgb {
        r: r * 255.0,
        g: g * 255.0,
        b: b * 255.0,
    }
}

/// `pad2` — note this pads by *length*, so a non-numeric channel that
/// stringifies to e.g. "NaN" is passed through untouched rather than padded.
pub fn pad2(c: &str) -> String {
    if c.chars().count() == 1 {
        format!("0{c}")
    } else {
        c.to_string()
    }
}

/// `Math.round(x).toString(16)`
fn round_to_hex(x: f64) -> String {
    let r = math_round(x);
    if r.is_nan() {
        return "NaN".to_string();
    }
    if r.is_infinite() {
        return if r > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if r < 0.0 {
        return format!("-{:x}", (-r) as i64);
    }
    format!("{:x}", r as i64)
}

/// `convertDecimalToHex(d)` = `Math.round(parseFloat(d) * 255).toString(16)`
pub fn convert_decimal_to_hex(d: f64) -> String {
    round_to_hex(d * 255.0)
}

/// `convertHexToDecimal(h)` = `parseInt(h, 16) / 255`
pub fn convert_hex_to_decimal(h: &str) -> f64 {
    parse_int_radix(h, 16) / 255.0
}

pub fn rgb_to_hex(r: f64, g: f64, b: f64, allow_3_char: bool) -> String {
    let hex = [
        pad2(&round_to_hex(r)),
        pad2(&round_to_hex(g)),
        pad2(&round_to_hex(b)),
    ];
    if allow_3_char && hex.iter().all(|h| first_two_match(h)) {
        return hex.iter().map(|h| first_char(h)).collect();
    }
    hex.concat()
}

pub fn rgba_to_hex(r: f64, g: f64, b: f64, a: f64, allow_4_char: bool) -> String {
    let hex = [
        pad2(&round_to_hex(r)),
        pad2(&round_to_hex(g)),
        pad2(&round_to_hex(b)),
        pad2(&convert_decimal_to_hex(a)),
    ];
    if allow_4_char && hex.iter().all(|h| first_two_match(h)) {
        return hex.iter().map(|h| first_char(h)).collect();
    }
    hex.concat()
}

pub fn rgba_to_argb_hex(r: f64, g: f64, b: f64, a: f64) -> String {
    [
        pad2(&convert_decimal_to_hex(a)),
        pad2(&round_to_hex(r)),
        pad2(&round_to_hex(g)),
        pad2(&round_to_hex(b)),
    ]
    .concat()
}

/// `hex.charAt(0) == hex.charAt(1)`. `charAt` on a short string yields "",
/// so a 1-char string compares its only char against "" and is not a match.
fn first_two_match(s: &str) -> bool {
    let mut it = s.chars();
    match (it.next(), it.next()) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}

fn first_char(s: &str) -> String {
    s.chars().next().map(String::from).unwrap_or_default()
}

/// `convertToPercentage(n)` — values <= 1 are reinterpreted as fractions.
///
/// Takes the original `Unit` rather than an `f64` because the `<=` comparison
/// uses `ToNumber`, not `parseFloat`. For the input `"1%"` those disagree:
/// `ToNumber("1%")` is NaN so the comparison is false and the value passes
/// through untouched, whereas `parseFloat("1%")` is 1 and would wrongly
/// rewrite it as `"100%"`. Found by the differential fuzzer; see D-014.
pub fn convert_to_percentage(n: &Unit) -> Unit {
    let v = n.to_number();
    if v <= 1.0 {
        Unit::Str(format!("{}%", num_to_string(v * 100.0)))
    } else {
        n.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bound01_percentage_and_plain() {
        assert_eq!(bound01(&Unit::Num(255.0), 255.0), 1.0);
        assert_eq!(bound01(&Unit::Str("100%".into()), 255.0), 1.0);
        assert_eq!(bound01(&Unit::Str("50%".into()), 255.0), 0.5);
        // "1.0" is coerced to "100%" by isOnePointZero, the numeric 1.0 is not.
        assert_eq!(bound01(&Unit::Str("1.0".into()), 255.0), 1.0);
        assert!(bound01(&Unit::Num(1.0), 255.0) < 0.01);
    }

    #[test]
    fn hex_round_trip() {
        assert_eq!(rgb_to_hex(255.0, 0.0, 0.0, false), "ff0000");
        assert_eq!(rgb_to_hex(255.0, 0.0, 0.0, true), "f00");
        assert_eq!(rgb_to_hex(0.0, 0.0, 0.0, true), "000");
        assert_eq!(rgba_to_argb_hex(255.0, 0.0, 0.0, 1.0), "ffff0000");
    }

    #[test]
    fn max_channel_tie_prefers_r() {
        // r == g > b: upstream's switch takes `case r` first.
        let h = rgb_to_hsl(&Unit::Num(200.0), &Unit::Num(200.0), &Unit::Num(10.0));
        assert!((h.h - 60.0 / 360.0).abs() < 1e-12);
    }
}

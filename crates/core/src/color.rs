//! The `TinyColor` type — port of the `tinycolor` constructor, its prototype
//! and the module-level statics.

use crate::convert::{
    bound01, bound_alpha, rgb_to_hex, rgb_to_hsl, rgb_to_hsv, rgba_to_argb_hex, rgba_to_hex, Unit,
};
use crate::jsnum::{clamp01, math_round, num_to_string};
use crate::names::HEX_NAME_MAP;
use crate::error::{Error, Result};
use crate::parse::{input_to_rgb, ColorObj, Input};

#[derive(Clone, Debug)]
pub struct TinyColor {
    pub r: f64,
    pub g: f64,
    pub b: f64,
    pub a: f64,
    pub round_a: f64,
    pub format: Option<String>,
    pub gradient_type: bool,
    pub ok: bool,
    pub original_input: Input,
}

/// `ToInt32`, needed by `analogous`'s `>>` which coerces its operand first.
fn to_int32(x: f64) -> i32 {
    if !x.is_finite() {
        return 0;
    }
    let m = x.trunc().rem_euclid(4294967296.0);
    if m >= 2147483648.0 {
        (m - 4294967296.0) as i32
    } else {
        m as i32
    }
}

impl TinyColor {
    pub fn new(input: Input) -> Self {
        Self::with_opts(input, None, false)
    }

    pub fn with_opts(input: Input, format: Option<String>, gradient_type: bool) -> Self {
        let rgb = input_to_rgb(&input);
        let a = rgb.a;
        let mut c = TinyColor {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
            a,
            round_a: math_round(100.0 * a) / 100.0,
            format: format.or(rgb.format),
            gradient_type,
            ok: rgb.ok,
            original_input: input,
        };
        // "Don't let the range of [0,255] come back in [0,1]."
        if c.r < 1.0 {
            c.r = math_round(c.r);
        }
        if c.g < 1.0 {
            c.g = math_round(c.g);
        }
        if c.b < 1.0 {
            c.b = math_round(c.b);
        }
        c
    }

    pub fn from_str(s: &str) -> Self {
        Self::new(Input::str(s))
    }

    // ---- predicates ----------------------------------------------------

    pub fn is_valid(&self) -> bool {
        self.ok
    }
    pub fn get_format(&self) -> Option<String> {
        self.format.clone()
    }
    pub fn get_alpha(&self) -> f64 {
        self.a
    }
    pub fn is_dark(&self) -> bool {
        self.get_brightness() < 128.0
    }
    pub fn is_light(&self) -> bool {
        !self.is_dark()
    }

    pub fn get_brightness(&self) -> f64 {
        let c = self.to_rgb();
        (c.0 * 299.0 + c.1 * 587.0 + c.2 * 114.0) / 1000.0
    }

    /// `getLuminance()` — WCAG relative luminance.
    ///
    /// The sRGB linearisation is read from a generated table rather than
    /// computed, because `Math.pow` is not bit-identical across libm
    /// implementations and upstream's `readability` test asserts exact
    /// equality. `toRgb()` rounds, so the input domain is exactly the 256
    /// integers the table covers. See DECISIONS.md D-012.
    pub fn get_luminance(&self) -> f64 {
        let (r, g, b, _) = self.to_rgb();
        let f = |c: f64| {
            crate::luminance::srgb_linear(c).unwrap_or_else(|| {
                let s = c / 255.0;
                if s <= 0.03928 {
                    s / 12.92
                } else {
                    ((s + 0.055) / 1.055).powf(2.4)
                }
            })
        };
        0.2126 * f(r) + 0.7152 * f(g) + 0.0722 * f(b)
    }

    pub fn set_alpha(&mut self, v: &Unit) -> &mut Self {
        self.a = bound_alpha(v);
        self.round_a = math_round(100.0 * self.a) / 100.0;
        self
    }

    // ---- conversions ---------------------------------------------------

    /// (r, g, b, a) with r/g/b rounded, as `toRgb()` returns.
    pub fn to_rgb(&self) -> (f64, f64, f64, f64) {
        (
            math_round(self.r),
            math_round(self.g),
            math_round(self.b),
            self.a,
        )
    }

    pub fn to_rgb_string(&self) -> String {
        let (r, g, b, _) = self.to_rgb();
        let (r, g, b) = (num_to_string(r), num_to_string(g), num_to_string(b));
        if self.a == 1.0 {
            format!("rgb({r}, {g}, {b})")
        } else {
            format!("rgba({r}, {g}, {b}, {})", num_to_string(self.round_a))
        }
    }

    fn pct(&self) -> (f64, f64, f64) {
        (
            math_round(bound01(&Unit::Num(self.r), 255.0) * 100.0),
            math_round(bound01(&Unit::Num(self.g), 255.0) * 100.0),
            math_round(bound01(&Unit::Num(self.b), 255.0) * 100.0),
        )
    }

    /// (r%, g%, b%, a)
    pub fn to_percentage_rgb(&self) -> (String, String, String, f64) {
        let (r, g, b) = self.pct();
        (
            format!("{}%", num_to_string(r)),
            format!("{}%", num_to_string(g)),
            format!("{}%", num_to_string(b)),
            self.a,
        )
    }

    pub fn to_percentage_rgb_string(&self) -> String {
        let (r, g, b) = self.pct();
        let (r, g, b) = (num_to_string(r), num_to_string(g), num_to_string(b));
        if self.a == 1.0 {
            format!("rgb({r}%, {g}%, {b}%)")
        } else {
            format!(
                "rgba({r}%, {g}%, {b}%, {})",
                num_to_string(self.round_a)
            )
        }
    }

    /// (h in degrees, s, l, a)
    pub fn to_hsl(&self) -> (f64, f64, f64, f64) {
        let h = rgb_to_hsl(&Unit::Num(self.r), &Unit::Num(self.g), &Unit::Num(self.b));
        (h.h * 360.0, h.s, h.l, self.a)
    }

    pub fn to_hsl_string(&self) -> String {
        let c = rgb_to_hsl(&Unit::Num(self.r), &Unit::Num(self.g), &Unit::Num(self.b));
        let h = num_to_string(math_round(c.h * 360.0));
        let s = num_to_string(math_round(c.s * 100.0));
        let l = num_to_string(math_round(c.l * 100.0));
        if self.a == 1.0 {
            format!("hsl({h}, {s}%, {l}%)")
        } else {
            format!("hsla({h}, {s}%, {l}%, {})", num_to_string(self.round_a))
        }
    }

    /// (h in degrees, s, v, a)
    pub fn to_hsv(&self) -> (f64, f64, f64, f64) {
        let c = rgb_to_hsv(&Unit::Num(self.r), &Unit::Num(self.g), &Unit::Num(self.b));
        (c.h * 360.0, c.s, c.v, self.a)
    }

    pub fn to_hsv_string(&self) -> String {
        let c = rgb_to_hsv(&Unit::Num(self.r), &Unit::Num(self.g), &Unit::Num(self.b));
        let h = num_to_string(math_round(c.h * 360.0));
        let s = num_to_string(math_round(c.s * 100.0));
        let v = num_to_string(math_round(c.v * 100.0));
        if self.a == 1.0 {
            format!("hsv({h}, {s}%, {v}%)")
        } else {
            format!("hsva({h}, {s}%, {v}%, {})", num_to_string(self.round_a))
        }
    }

    pub fn to_hex(&self, allow_3_char: bool) -> String {
        rgb_to_hex(self.r, self.g, self.b, allow_3_char)
    }
    pub fn to_hex_string(&self, allow_3_char: bool) -> String {
        format!("#{}", self.to_hex(allow_3_char))
    }
    pub fn to_hex8(&self, allow_4_char: bool) -> String {
        rgba_to_hex(self.r, self.g, self.b, self.a, allow_4_char)
    }
    pub fn to_hex8_string(&self, allow_4_char: bool) -> String {
        format!("#{}", self.to_hex8(allow_4_char))
    }

    /// `toName()` — `false` upstream becomes `None` here.
    pub fn to_name(&self) -> Option<String> {
        if self.a == 0.0 {
            return Some("transparent".into());
        }
        if self.a < 1.0 {
            return None;
        }
        let hex = rgb_to_hex(self.r, self.g, self.b, true);
        // `hexNames` is built by flipping `names`, so later entries win — the
        // map is built in source order to preserve that (D-004). Upstream is
        // an O(1) object lookup; this was a 149-entry scan with no early exit,
        // and a miss (the common case) always cost the full scan. See D-019.
        HEX_NAME_MAP.get(hex.as_str()).map(|n| n.to_string())
    }

    pub fn to_filter(&self, second: Option<&TinyColor>) -> String {
        let first = format!("#{}", rgba_to_argb_hex(self.r, self.g, self.b, self.a));
        let second = match second {
            Some(s) => format!("#{}", rgba_to_argb_hex(s.r, s.g, s.b, s.a)),
            None => first.clone(),
        };
        let gt = if self.gradient_type {
            "GradientType = 1, "
        } else {
            ""
        };
        format!(
            "progid:DXImageTransform.Microsoft.gradient({gt}startColorstr={first},endColorstr={second})"
        )
    }

    pub fn to_string(&self, format: Option<&str>) -> String {
        let format_set = format.is_some();
        let fmt = format
            .map(str::to_string)
            .or_else(|| self.format.clone())
            .unwrap_or_default();

        let has_alpha = self.a < 1.0 && self.a >= 0.0;
        let needs_alpha_format = !format_set
            && has_alpha
            && matches!(
                fmt.as_str(),
                "hex" | "hex6" | "hex3" | "hex4" | "hex8" | "name"
            );

        if needs_alpha_format {
            if fmt == "name" && self.a == 0.0 {
                return self.to_name().unwrap_or_default();
            }
            return self.to_rgb_string();
        }

        let formatted: Option<String> = match fmt.as_str() {
            "rgb" => Some(self.to_rgb_string()),
            "prgb" => Some(self.to_percentage_rgb_string()),
            "hex" | "hex6" => Some(self.to_hex_string(false)),
            "hex3" => Some(self.to_hex_string(true)),
            "hex4" => Some(self.to_hex8_string(true)),
            "hex8" => Some(self.to_hex8_string(false)),
            "name" => self.to_name(),
            "hsl" => Some(self.to_hsl_string()),
            "hsv" => Some(self.to_hsv_string()),
            _ => None,
        };

        // `formattedString || this.toHexString()` — empty string is falsy too.
        match formatted {
            Some(s) if !s.is_empty() => s,
            _ => self.to_hex_string(false),
        }
    }

    pub fn clone_color(&self) -> TinyColor {
        TinyColor::from_str(&self.to_string(None))
    }

    // ---- modifications (mutating, chainable) ---------------------------

    fn apply(&mut self, other: TinyColor) -> &mut Self {
        self.r = other.r;
        self.g = other.g;
        self.b = other.b;
        self.set_alpha(&Unit::Num(other.a));
        self
    }

    pub fn lighten(&mut self, amount: Option<f64>) -> &mut Self {
        let c = lighten(self, amount);
        self.apply(c)
    }
    pub fn brighten(&mut self, amount: Option<f64>) -> &mut Self {
        let c = brighten(self, amount);
        self.apply(c)
    }
    pub fn darken(&mut self, amount: Option<f64>) -> &mut Self {
        let c = darken(self, amount);
        self.apply(c)
    }
    pub fn desaturate(&mut self, amount: Option<f64>) -> &mut Self {
        let c = desaturate(self, amount);
        self.apply(c)
    }
    pub fn saturate(&mut self, amount: Option<f64>) -> &mut Self {
        let c = saturate(self, amount);
        self.apply(c)
    }
    pub fn greyscale(&mut self) -> &mut Self {
        let c = desaturate(self, Some(100.0));
        self.apply(c)
    }
    pub fn spin(&mut self, amount: f64) -> &mut Self {
        let c = spin(self, amount);
        self.apply(c)
    }
}

/// `tinycolor({h, s, l})` — a *fresh* object literal, so alpha is dropped.
/// Used by `polyad` and `splitcomplement`.
fn hsl_color(h: f64, s: f64, l: f64) -> TinyColor {
    TinyColor::new(Input::Obj(ColorObj {
        h: Some(Unit::Num(h)),
        s: Some(Unit::Num(s)),
        l: Some(Unit::Num(l)),
        ..Default::default()
    }))
}

/// `tinycolor(hsl)` where `hsl` is the object returned by `toHsl()` — which
/// carries `a`, so alpha survives.
///
/// Whether a combination preserves alpha depends on which of these two forms
/// upstream happens to use, and it is not consistent: `complement` and
/// `analogous` pass the mutated `toHsl()` object through (alpha preserved),
/// while `polyad`, `splitcomplement` and `monochromatic` build a fresh
/// `{h, s, l}` literal (alpha dropped). The differential fuzzer caught this;
/// see DECISIONS.md D-013.
fn hsl_color_a(h: f64, s: f64, l: f64, a: f64) -> TinyColor {
    TinyColor::new(Input::Obj(ColorObj {
        h: Some(Unit::Num(h)),
        s: Some(Unit::Num(s)),
        l: Some(Unit::Num(l)),
        a: Some(Unit::Num(a)),
        ..Default::default()
    }))
}

/// `amount === 0 ? 0 : amount || 10`
fn amount_or_default(a: Option<f64>) -> f64 {
    match a {
        Some(v) if v == 0.0 => 0.0,
        Some(v) if v.is_nan() => 10.0, // NaN is falsy in JS
        Some(v) => v,
        None => 10.0,
    }
}

pub fn desaturate(c: &TinyColor, amount: Option<f64>) -> TinyColor {
    let amount = amount_or_default(amount);
    let (h, s, l, a) = c.to_hsl();
    hsl_color_a(h, clamp01(s - amount / 100.0), l, a)
}

pub fn saturate(c: &TinyColor, amount: Option<f64>) -> TinyColor {
    let amount = amount_or_default(amount);
    let (h, s, l, a) = c.to_hsl();
    hsl_color_a(h, clamp01(s + amount / 100.0), l, a)
}

pub fn lighten(c: &TinyColor, amount: Option<f64>) -> TinyColor {
    let amount = amount_or_default(amount);
    let (h, s, l, a) = c.to_hsl();
    hsl_color_a(h, s, clamp01(l + amount / 100.0), a)
}

pub fn darken(c: &TinyColor, amount: Option<f64>) -> TinyColor {
    let amount = amount_or_default(amount);
    let (h, s, l, a) = c.to_hsl();
    hsl_color_a(h, s, clamp01(l - amount / 100.0), a)
}

pub fn brighten(c: &TinyColor, amount: Option<f64>) -> TinyColor {
    let amount = amount_or_default(amount);
    let (r, g, b, a) = c.to_rgb();
    let adj = |x: f64| (x - math_round(255.0 * -(amount / 100.0))).min(255.0).max(0.0);
    TinyColor::new(Input::Obj(ColorObj {
        r: Some(Unit::Num(adj(r))),
        g: Some(Unit::Num(adj(g))),
        b: Some(Unit::Num(adj(b))),
        a: Some(Unit::Num(a)),
        ..Default::default()
    }))
}

pub fn spin(c: &TinyColor, amount: f64) -> TinyColor {
    let (h, s, l, a) = c.to_hsl();
    let hue = (h + amount) % 360.0;
    hsl_color_a(if hue < 0.0 { 360.0 + hue } else { hue }, s, l, a)
}

// ---- combinations ------------------------------------------------------

pub fn complement(c: &TinyColor) -> TinyColor {
    let (h, s, l, a) = c.to_hsl();
    hsl_color_a((h + 180.0) % 360.0, s, l, a)
}

pub fn polyad(c: &TinyColor, number: f64) -> Result<Vec<TinyColor>> {
    if number.is_nan() || number <= 0.0 {
        return Err(Error::InvalidPolyadCount);
    }
    let (h, s, l, _) = c.to_hsl();
    let mut out = vec![c.clone()];
    let step = 360.0 / number;
    let mut i = 1.0;
    while i < number {
        out.push(hsl_color((h + i * step) % 360.0, s, l));
        i += 1.0;
    }
    Ok(out)
}

pub fn splitcomplement(c: &TinyColor) -> Vec<TinyColor> {
    let (h, s, l, _) = c.to_hsl();
    vec![
        c.clone(),
        hsl_color((h + 72.0) % 360.0, s, l),
        hsl_color((h + 216.0) % 360.0, s, l),
    ]
}

pub fn analogous(c: &TinyColor, results: Option<f64>, slices: Option<f64>) -> Vec<TinyColor> {
    let mut results = results.filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(6.0);
    let slices = slices.filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(30.0);

    let (h0, s, l, a) = c.to_hsl();
    let part = 360.0 / slices;
    let mut out = vec![c.clone()];

    // `(hsl.h - ((part * results) >> 1) + 720) % 360` — the shift is an
    // int32 coercion followed by an arithmetic shift, not a float halving.
    let mut h = (h0 - (to_int32(part * results) >> 1) as f64 + 720.0) % 360.0;

    results -= 1.0;
    while results > 0.0 {
        h = (h + part) % 360.0;
        out.push(hsl_color_a(h, s, l, a));
        results -= 1.0;
    }
    out
}

pub fn monochromatic(c: &TinyColor, results: Option<f64>) -> Vec<TinyColor> {
    let results = results.filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(6.0);
    let (h, s, mut v, _) = c.to_hsv();
    let mut out = Vec::new();
    let modification = 1.0 / results;
    let mut n = results;
    while n > 0.0 {
        out.push(TinyColor::new(Input::Obj(ColorObj {
            h: Some(Unit::Num(h)),
            s: Some(Unit::Num(s)),
            v: Some(Unit::Num(v)),
            ..Default::default()
        })));
        v = (v + modification) % 1.0;
        n -= 1.0;
    }
    out
}

// ---- statics -----------------------------------------------------------

pub fn mix(c1: &TinyColor, c2: &TinyColor, amount: Option<f64>) -> TinyColor {
    let amount = match amount {
        Some(v) if v == 0.0 => 0.0,
        Some(v) if v.is_nan() => 50.0,
        Some(v) => v,
        None => 50.0,
    };
    let (r1, g1, b1, a1) = c1.to_rgb();
    let (r2, g2, b2, a2) = c2.to_rgb();
    let p = amount / 100.0;
    TinyColor::new(Input::Obj(ColorObj {
        r: Some(Unit::Num((r2 - r1) * p + r1)),
        g: Some(Unit::Num((g2 - g1) * p + g1)),
        b: Some(Unit::Num((b2 - b1) * p + b1)),
        a: Some(Unit::Num((a2 - a1) * p + a1)),
        ..Default::default()
    }))
}

pub fn readability(c1: &TinyColor, c2: &TinyColor) -> f64 {
    let l1 = c1.get_luminance();
    let l2 = c2.get_luminance();
    (l1.max(l2) + 0.05) / (l1.min(l2) + 0.05)
}

pub fn is_readable(c1: &TinyColor, c2: &TinyColor, level: &str, size: &str) -> bool {
    let r = readability(c1, c2);
    let level = match level.to_uppercase().as_str() {
        "AAA" => "AAA",
        _ => "AA",
    };
    let size = match size.to_lowercase().as_str() {
        "large" => "large",
        _ => "small",
    };
    match (level, size) {
        ("AA", "small") | ("AAA", "large") => r >= 4.5,
        ("AA", "large") => r >= 3.0,
        ("AAA", "small") => r >= 7.0,
        _ => false,
    }
}

pub fn equals(c1: &TinyColor, c2: &TinyColor) -> bool {
    c1.to_rgb_string() == c2.to_rgb_string()
}

pub fn from_ratio(obj: &ColorObj, format: Option<String>) -> TinyColor {
    use crate::convert::convert_to_percentage;
    let conv = |u: &Option<Unit>| u.as_ref().map(convert_to_percentage);
    let scaled = ColorObj {
        r: conv(&obj.r),
        g: conv(&obj.g),
        b: conv(&obj.b),
        h: conv(&obj.h),
        s: conv(&obj.s),
        l: conv(&obj.l),
        v: conv(&obj.v),
        a: obj.a.clone(), // `a` is passed through unscaled
        format: obj.format.clone(),
    };
    TinyColor::with_opts(Input::Obj(scaled), format, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn basic_round_trips() {
        assert_eq!(TinyColor::from_str("red").to_hex_string(false), "#ff0000");
        assert_eq!(TinyColor::from_str("#f00").to_rgb_string(), "rgb(255, 0, 0)");
        assert_eq!(TinyColor::from_str("red").to_name().as_deref(), Some("red"));
        assert_eq!(TinyColor::from_str("red").to_hsl_string(), "hsl(0, 100%, 50%)");
    }

    #[test]
    fn alpha_forces_rgba_output() {
        let c = TinyColor::from_str("rgba(255, 0, 0, 0.5)");
        assert_eq!(c.to_string(None), "rgba(255, 0, 0, 0.5)");
    }

    #[test]
    fn modifications_chain() {
        let mut c = TinyColor::from_str("red");
        c.lighten(Some(10.0));
        assert_eq!(c.to_hex_string(false), "#ff3333");
    }

    #[test]
    fn invalid_input_is_not_ok() {
        assert!(!TinyColor::from_str("not a color").is_valid());
        assert!(TinyColor::from_str("red").is_valid());
    }
}

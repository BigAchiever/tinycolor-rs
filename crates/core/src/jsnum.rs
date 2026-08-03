//! JavaScript numeric semantics, reproduced exactly.
//!
//! TinyColor leans on `parseFloat`, `parseInt`, `Math.round` and implicit
//! `Number -> String` coercion in ways that are load-bearing for its output.
//! Rust's equivalents differ in every one of those cases, so the port routes
//! all arithmetic-on-untrusted-input through this module rather than using
//! std directly. Each divergence is recorded in DECISIONS.md.
//!
//! Nothing here allocates on the hot path except `num_to_string`.

/// `Math.round` — ties round toward +Infinity.
///
/// Rust's `f64::round` rounds ties *away from zero*, so it disagrees with JS
/// on every negative half-integer: JS `Math.round(-0.5)` is `-0`, Rust's
/// `(-0.5f64).round()` is `-1.0`. TinyColor hits this through `spin()` with
/// negative hues and through `convertDecimalToHex` on out-of-range alpha.
///
/// Implemented via floor/fract rather than the `floor(x + 0.5)` shorthand:
/// the shorthand is wrong for values just under a half (`0.49999999999999994`
/// becomes `1.0` because `x + 0.5` rounds up before the floor).
pub fn math_round(x: f64) -> f64 {
    if x.is_nan() || x.is_infinite() || x == 0.0 {
        return x;
    }
    let floor = x.floor();
    let frac = x - floor;
    let r = if frac >= 0.5 { floor + 1.0 } else { floor };
    // JS preserves negative zero for -0.5 <= x < 0.
    if r == 0.0 && x < 0.0 {
        -0.0
    } else {
        r
    }
}

/// `parseFloat` — parses the longest valid numeric *prefix*, NaN if none.
///
/// Rust's `str::parse::<f64>` is all-or-nothing and rejects trailing garbage,
/// so `"50%"` returns `Err` there but `50.0` in JS. TinyColor depends on the
/// prefix behaviour for every percentage unit it accepts.
pub fn parse_float(s: &str) -> f64 {
    let t = s.trim_start_matches(js_whitespace);
    let b = t.as_bytes();
    let mut i = 0usize;

    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        i += 1;
    }
    let sign_len = i;

    if t[i..].starts_with("Infinity") {
        let neg = sign_len == 1 && b[0] == b'-';
        return if neg { f64::NEG_INFINITY } else { f64::INFINITY };
    }

    let int_start = i;
    while i < b.len() && b[i].is_ascii_digit() {
        i += 1;
    }
    let int_digits = i - int_start;

    let mut frac_digits = 0usize;
    if i < b.len() && b[i] == b'.' {
        i += 1;
        let fs = i;
        while i < b.len() && b[i].is_ascii_digit() {
            i += 1;
        }
        frac_digits = i - fs;
    }

    // No mantissa digits at all -> NaN (JS returns NaN for "." and "abc").
    if int_digits == 0 && frac_digits == 0 {
        return f64::NAN;
    }

    // Exponent is only consumed if it is well-formed; otherwise it is not
    // part of the numeric prefix and parsing simply stops before it.
    let before_exp = i;
    if i < b.len() && (b[i] == b'e' || b[i] == b'E') {
        let mut j = i + 1;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let ds = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j > ds {
            i = j;
        } else {
            i = before_exp;
        }
    }

    t[..i].parse::<f64>().unwrap_or(f64::NAN)
}

/// `parseInt(str, radix)` — prefix parse in the given radix, NaN if none.
pub fn parse_int_radix(s: &str, radix: u32) -> f64 {
    let t = s.trim_start_matches(js_whitespace);
    let b = t.as_bytes();
    let mut i = 0usize;
    let mut neg = false;

    if i < b.len() && (b[i] == b'+' || b[i] == b'-') {
        neg = b[i] == b'-';
        i += 1;
    }
    if radix == 16 && (t[i..].starts_with("0x") || t[i..].starts_with("0X")) {
        i += 2;
    }

    let start = i;
    let mut acc = 0f64;
    while i < b.len() {
        match (b[i] as char).to_digit(36) {
            Some(d) if d < radix => {
                acc = acc * radix as f64 + d as f64;
                i += 1;
            }
            _ => break,
        }
    }
    if i == start {
        return f64::NAN;
    }
    if neg {
        -acc
    } else {
        acc
    }
}

/// `parseInt(someNumber, 10)` — the number is stringified first, *then* parsed.
///
/// This is not a rounding operation and must not be replaced with `trunc()`.
/// When the value stringifies to exponential notation, `parseInt` reads only
/// the mantissa: `parseInt(2.55e-7)` is `2`, not `0`. TinyColor reaches this
/// path inside `bound01` for sufficiently small percentage inputs, which is
/// the suspected upstream defect tracked in DECISIONS.md (D-011).
pub fn parse_int_from_number(x: f64) -> f64 {
    parse_int_radix(&num_to_string(x), 10)
}

/// `String(number)` for f64 — shortest round-trip, with JS's exponential
/// thresholds (|x| >= 1e21 or |x| < 1e-6).
///
/// Rust's `{}` never emits exponential notation and its `{:e}` omits the `+`
/// on positive exponents, so neither is usable directly.
pub fn num_to_string(x: f64) -> String {
    if x.is_nan() {
        return "NaN".to_string();
    }
    if x.is_infinite() {
        return if x > 0.0 { "Infinity" } else { "-Infinity" }.to_string();
    }
    if x == 0.0 {
        return "0".to_string(); // JS prints -0 as "0"
    }

    let a = x.abs();
    if !(1e-6..1e21).contains(&a) {
        let e = format!("{:e}", x); // e.g. "1e-7", "1.5e21"
        return match e.split_once('e') {
            Some((m, exp)) if !exp.starts_with('-') => format!("{m}e+{exp}"),
            _ => e,
        };
    }
    format!("{x}")
}

/// `ToNumber(string)` — the abstract operation behind `<`, `<=`, `*` and
/// friends when an operand is a string.
///
/// Critically different from [`parse_float`]: `ToNumber` requires the *whole*
/// string to be numeric, so `Number("1%")` is `NaN` while `parseFloat("1%")`
/// is `1`. TinyColor's `convertToPercentage` compares with `<=`, which routes
/// through here, and then `bound01` parses the same value with `parseFloat`.
/// Using one for the other silently changes which branch is taken — see
/// DECISIONS.md D-014.
pub fn to_number(s: &str) -> f64 {
    let t = s.trim_matches(js_whitespace);
    if t.is_empty() {
        return 0.0;
    }
    if let Some(h) = t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
        return u64::from_str_radix(h, 16).map(|v| v as f64).unwrap_or(f64::NAN);
    }
    if let Some(o) = t.strip_prefix("0o").or_else(|| t.strip_prefix("0O")) {
        return u64::from_str_radix(o, 8).map(|v| v as f64).unwrap_or(f64::NAN);
    }
    if let Some(b) = t.strip_prefix("0b").or_else(|| t.strip_prefix("0B")) {
        return u64::from_str_radix(b, 2).map(|v| v as f64).unwrap_or(f64::NAN);
    }
    match t {
        "Infinity" | "+Infinity" => return f64::INFINITY,
        "-Infinity" => return f64::NEG_INFINITY,
        _ => {}
    }
    // Rust accepts "inf"/"nan" spellings that JS does not.
    if t.chars().any(|c| c.is_ascii_alphabetic() && c != 'e' && c != 'E') {
        return f64::NAN;
    }
    t.parse::<f64>().unwrap_or(f64::NAN)
}

/// `isOnePointZero` — a *string* containing "." whose value is exactly 1.
///
/// Upstream uses this to treat `"1.0"` as `"100%"`. It is deliberately
/// type-sensitive: the numeric `1.0` is not affected, only the string form.
/// The port therefore keeps string and numeric inputs distinguishable all the
/// way down to this check (see `Unit` in `parse.rs`).
pub fn is_one_point_zero(s: &str) -> bool {
    s.contains('.') && parse_float(s) == 1.0
}

/// `isPercentage`
pub fn is_percentage(s: &str) -> bool {
    s.contains('%')
}

/// `Math.min(1, Math.max(0, val))`
///
/// Uses `clamp` rather than `.max(0.0).min(1.0)`, and the difference is
/// behavioural, not stylistic. Rust's `f64::max` returns the *other* operand
/// when one side is NaN, so `NAN.max(0.0).min(1.0)` is `0.0` — whereas JS
/// propagates: `Math.min(1, Math.max(0, NaN))` is `NaN`. `f64::clamp`
/// propagates NaN and therefore matches. The bounds are constants, so the
/// `min > max` panic is unreachable.
pub fn clamp01(v: f64) -> f64 {
    v.clamp(0.0, 1.0)
}

/// JS whitespace set as recognised by `parseFloat`/`parseInt`.
fn js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{0009}'..='\u{000D}'
            | '\u{0020}'
            | '\u{00A0}'
            | '\u{1680}'
            | '\u{2000}'..='\u{200A}'
            | '\u{2028}'
            | '\u{2029}'
            | '\u{202F}'
            | '\u{205F}'
            | '\u{3000}'
            | '\u{FEFF}'
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn math_round_matches_js_on_ties() {
        assert_eq!(math_round(0.5), 1.0);
        assert_eq!(math_round(1.5), 2.0);
        assert_eq!(math_round(-1.5), -1.0); // Rust's round() gives -2.0
        assert_eq!(math_round(2.5), 3.0);
        assert!(math_round(-0.5).is_sign_negative()); // -0
        assert_eq!(math_round(0.49999999999999994), 0.0);
    }

    #[test]
    fn parse_float_takes_prefix() {
        assert_eq!(parse_float("50%"), 50.0);
        assert_eq!(parse_float("  -3.5deg"), -3.5);
        assert_eq!(parse_float("1e3x"), 1000.0);
        assert_eq!(parse_float("1e"), 1.0); // malformed exponent not consumed
        assert!(parse_float("abc").is_nan());
        assert!(parse_float(".").is_nan());
        assert_eq!(parse_float("Infinity"), f64::INFINITY);
    }

    #[test]
    fn parse_int_from_number_reads_mantissa_only() {
        assert_eq!(parse_int_from_number(2.55e-7), 2.0);
        assert_eq!(parse_int_from_number(0.0000255), 0.0);
        assert_eq!(parse_int_from_number(255.9), 255.0);
    }

    #[test]
    fn num_to_string_thresholds() {
        assert_eq!(num_to_string(1e-7), "1e-7");
        assert_eq!(num_to_string(0.000001), "0.000001");
        assert_eq!(num_to_string(1e21), "1e+21");
        assert_eq!(num_to_string(-0.0), "0");
        assert_eq!(num_to_string(1.0), "1");
    }
}

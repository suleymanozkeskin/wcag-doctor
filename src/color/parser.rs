use regex::Regex;
use std::sync::LazyLock;

use super::Rgba;
use super::hsl::hsla_to_rgba;
use super::named_colors;
use super::oklch::oklch_to_rgba;

/// Result of attempting to parse a color string.
#[derive(Debug)]
pub enum ColorParseResult {
    /// Successfully resolved to a concrete color.
    Resolved(Rgba),
    /// Contains a CSS variable reference that needs external resolution.
    /// The inner string is the variable name (e.g. "--primary").
    VarReference(String),
    /// Contains a CSS variable wrapped in a color function.
    /// E.g. `hsl(var(--primary))` -> wrapper="hsl", var_name="--primary".
    WrappedVarReference { wrapper: String, var_name: String },
    /// Could not parse.
    Unresolvable(String),
}

// Precompiled regex patterns for performance.

static RGB_FUNC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^rgba?\(\s*([0-9.]+%?)\s*[,\s]\s*([0-9.]+%?)\s*[,\s]\s*([0-9.]+%?)(?:\s*[,/]\s*([0-9.]+%?))?\s*\)$",
    )
    .unwrap()
});

static HSL_FUNC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^hsla?\(\s*([0-9.]+)(?:deg)?\s*[,\s]\s*([0-9.]+)%\s*[,\s]\s*([0-9.]+)%(?:\s*[,/]\s*([0-9.]+%?))?\s*\)$",
    )
    .unwrap()
});

static BARE_HSL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([0-9.]+)\s+([0-9.]+)%\s+([0-9.]+)%$").unwrap()
});

static OKLCH_FUNC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"(?i)^oklch\(\s*([0-9.]+%?)\s+([0-9.]+)\s+([0-9.]+)(?:deg)?(?:\s*/\s*([0-9.]+%?))?\s*\)$",
    )
    .unwrap()
});

static VAR_FUNC: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^var\(\s*(--[a-zA-Z0-9_-]+)\s*(?:,\s*(.*))?\)$").unwrap()
});

static WRAPPED_VAR: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(hsl|hsla|rgb|rgba)\(\s*var\(\s*(--[a-zA-Z0-9_-]+)\s*\)\s*(?:[,/]\s*([0-9.]+%?))?\s*\)$").unwrap()
});

/// Parse a color string into a `ColorParseResult`.
///
/// Handles: hex, rgb(), rgba(), hsl(), hsla(), bare HSL, oklch(),
/// named colors, var() references, and hsl(var(--x)) wrappers.
pub fn parse_color(input: &str) -> ColorParseResult {
    let trimmed = input.trim();

    if trimmed.is_empty() {
        return ColorParseResult::Unresolvable("empty string".to_string());
    }

    let first_byte = trimmed.as_bytes()[0];

    // Fast-path: hex colors start with '#' — most common in resolved values
    if first_byte == b'#' {
        if let Some(rgba) = try_parse_hex(trimmed) {
            return ColorParseResult::Resolved(rgba);
        }
    }

    // Function-form colors and var() references start with a letter
    if first_byte.is_ascii_alphabetic() {
        let lower_start = trimmed.get(..4).unwrap_or(trimmed).to_ascii_lowercase();

        // Wrapped var: hsl(var(--x)), rgb(var(--x))
        if lower_start.starts_with("hsl") || lower_start.starts_with("rgb") {
            if trimmed.contains("var(") {
                if let Some(caps) = WRAPPED_VAR.captures(trimmed) {
                    let wrapper = caps[1].to_lowercase();
                    let var_name = caps[2].to_string();
                    return ColorParseResult::WrappedVarReference { wrapper, var_name };
                }
            }
            // Plain hsl()/rgb()
            if lower_start.starts_with("rgb") {
                if let Some(rgba) = try_parse_rgb(trimmed) {
                    return ColorParseResult::Resolved(rgba);
                }
            }
            if lower_start.starts_with("hsl") {
                if let Some(rgba) = try_parse_hsl_func(trimmed) {
                    return ColorParseResult::Resolved(rgba);
                }
            }
        }

        // var() reference
        if lower_start.starts_with("var(") {
            if let Some(caps) = VAR_FUNC.captures(trimmed) {
                let var_name = caps[1].to_string();
                return ColorParseResult::VarReference(var_name);
            }
        }

        // oklch()
        if lower_start.starts_with("oklc") {
            if let Some(rgba) = try_parse_oklch(trimmed) {
                return ColorParseResult::Resolved(rgba);
            }
        }

        // Named CSS colors (only worth trying for alphabetic-only inputs)
        if let Some(rgba) = named_colors::lookup(&trimmed.to_lowercase()) {
            return ColorParseResult::Resolved(rgba);
        }
    }

    // Bare HSL: starts with a digit (e.g. "204 88% 24.1%")
    if first_byte.is_ascii_digit() {
        if let Some(rgba) = try_parse_bare_hsl(trimmed) {
            return ColorParseResult::Resolved(rgba);
        }
    }

    ColorParseResult::Unresolvable(format!("unrecognized color format: {trimmed}"))
}

fn hex_digit(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

fn try_parse_hex(s: &str) -> Option<Rgba> {
    let bytes = s.as_bytes();
    if bytes.first() != Some(&b'#') {
        return None;
    }
    let hex = &bytes[1..];
    if !hex.iter().all(|b| hex_digit(*b).is_some()) {
        return None;
    }

    match hex.len() {
        3 => {
            let r = hex_digit(hex[0])?;
            let g = hex_digit(hex[1])?;
            let b = hex_digit(hex[2])?;
            Some(Rgba::opaque(r * 16 + r, g * 16 + g, b * 16 + b))
        }
        6 => {
            let r = hex_digit(hex[0])? * 16 + hex_digit(hex[1])?;
            let g = hex_digit(hex[2])? * 16 + hex_digit(hex[3])?;
            let b = hex_digit(hex[4])? * 16 + hex_digit(hex[5])?;
            Some(Rgba::opaque(r, g, b))
        }
        8 => {
            let r = hex_digit(hex[0])? * 16 + hex_digit(hex[1])?;
            let g = hex_digit(hex[2])? * 16 + hex_digit(hex[3])?;
            let b = hex_digit(hex[4])? * 16 + hex_digit(hex[5])?;
            let a = hex_digit(hex[6])? * 16 + hex_digit(hex[7])?;
            Some(Rgba::new(r, g, b, a as f64 / 255.0))
        }
        _ => None,
    }
}

fn try_parse_rgb(s: &str) -> Option<Rgba> {
    let caps = RGB_FUNC.captures(s)?;

    let parse_channel = |val: &str| -> Option<u8> {
        if val.ends_with('%') {
            let pct: f64 = val.trim_end_matches('%').parse().ok()?;
            Some((pct / 100.0 * 255.0).round().clamp(0.0, 255.0) as u8)
        } else {
            let v: f64 = val.parse().ok()?;
            Some(v.round().clamp(0.0, 255.0) as u8)
        }
    };

    let r = parse_channel(&caps[1])?;
    let g = parse_channel(&caps[2])?;
    let b = parse_channel(&caps[3])?;
    let a = match caps.get(4) {
        Some(m) => parse_alpha(m.as_str())?,
        None => 1.0,
    };

    Some(Rgba::new(r, g, b, a))
}

fn try_parse_hsl_func(s: &str) -> Option<Rgba> {
    let caps = HSL_FUNC.captures(s)?;

    let h: f64 = caps[1].parse().ok()?;
    let s_pct: f64 = caps[2].parse().ok()?;
    let l_pct: f64 = caps[3].parse().ok()?;
    let a = match caps.get(4) {
        Some(m) => parse_alpha(m.as_str())?,
        None => 1.0,
    };

    Some(hsla_to_rgba(h, s_pct / 100.0, l_pct / 100.0, a))
}

fn try_parse_bare_hsl(s: &str) -> Option<Rgba> {
    let caps = BARE_HSL.captures(s)?;

    let h: f64 = caps[1].parse().ok()?;
    let s_pct: f64 = caps[2].parse().ok()?;
    let l_pct: f64 = caps[3].parse().ok()?;

    Some(hsla_to_rgba(h, s_pct / 100.0, l_pct / 100.0, 1.0))
}

fn try_parse_oklch(s: &str) -> Option<Rgba> {
    let caps = OKLCH_FUNC.captures(s)?;

    let l_str = &caps[1];
    let l: f64 = if l_str.ends_with('%') {
        l_str.trim_end_matches('%').parse::<f64>().ok()? / 100.0
    } else {
        l_str.parse().ok()?
    };

    let c: f64 = caps[2].parse().ok()?;
    let h: f64 = caps[3].parse().ok()?;
    let a = match caps.get(4) {
        Some(m) => parse_alpha(m.as_str())?,
        None => 1.0,
    };

    Some(oklch_to_rgba(l, c, h, a))
}

/// Parse an alpha value that may be a percentage or a 0.0-1.0 float.
fn parse_alpha(s: &str) -> Option<f64> {
    if s.ends_with('%') {
        let pct: f64 = s.trim_end_matches('%').parse().ok()?;
        Some((pct / 100.0).clamp(0.0, 1.0))
    } else {
        let v: f64 = s.parse().ok()?;
        Some(v.clamp(0.0, 1.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_resolved(input: &str, expected_r: u8, expected_g: u8, expected_b: u8) {
        match parse_color(input) {
            ColorParseResult::Resolved(c) => {
                assert_eq!(
                    (c.r, c.g, c.b),
                    (expected_r, expected_g, expected_b),
                    "color mismatch for input: {input}"
                );
            }
            other => panic!("expected Resolved for '{input}', got: {other:?}"),
        }
    }

    #[test]
    fn test_hex_3() {
        assert_resolved("#fff", 255, 255, 255);
        assert_resolved("#000", 0, 0, 0);
        assert_resolved("#f00", 255, 0, 0);
    }

    #[test]
    fn test_hex_6() {
        assert_resolved("#ffffff", 255, 255, 255);
        assert_resolved("#1a2b3c", 26, 43, 60);
    }

    #[test]
    fn test_hex_8() {
        match parse_color("#1a2b3cff") {
            ColorParseResult::Resolved(c) => {
                assert_eq!((c.r, c.g, c.b), (26, 43, 60));
                assert!((c.a - 1.0).abs() < 0.01);
            }
            other => panic!("expected Resolved, got: {other:?}"),
        }
    }

    #[test]
    fn test_rgb() {
        assert_resolved("rgb(255, 0, 0)", 255, 0, 0);
        assert_resolved("rgb(255 128 0)", 255, 128, 0);
    }

    #[test]
    fn test_rgba() {
        match parse_color("rgba(255, 0, 0, 0.5)") {
            ColorParseResult::Resolved(c) => {
                assert_eq!((c.r, c.g, c.b), (255, 0, 0));
                assert!((c.a - 0.5).abs() < 0.01);
            }
            other => panic!("expected Resolved, got: {other:?}"),
        }
    }

    #[test]
    fn test_hsl() {
        assert_resolved("hsl(0, 100%, 50%)", 255, 0, 0);
        assert_resolved("hsl(120, 100%, 50%)", 0, 255, 0);
        assert_resolved("hsl(240 100% 50%)", 0, 0, 255);
    }

    #[test]
    fn test_bare_hsl() {
        // This is the shadcn/Tailwind convention: "204 88% 24.1%"
        match parse_color("204 88% 24.1%") {
            ColorParseResult::Resolved(c) => {
                // Should be a dark blue
                assert!(c.r < 50);
                assert!(c.b > 50);
            }
            other => panic!("expected Resolved for bare HSL, got: {other:?}"),
        }
    }

    #[test]
    fn test_oklch() {
        match parse_color("oklch(0.5 0.2 240)") {
            ColorParseResult::Resolved(_) => {}
            other => panic!("expected Resolved for oklch, got: {other:?}"),
        }
    }

    #[test]
    fn test_named_color() {
        assert_resolved("red", 255, 0, 0);
        assert_resolved("White", 255, 255, 255);
        assert_resolved("BLUE", 0, 0, 255);
    }

    #[test]
    fn test_var_reference() {
        match parse_color("var(--primary)") {
            ColorParseResult::VarReference(name) => {
                assert_eq!(name, "--primary");
            }
            other => panic!("expected VarReference, got: {other:?}"),
        }
    }

    #[test]
    fn test_wrapped_var() {
        match parse_color("hsl(var(--primary))") {
            ColorParseResult::WrappedVarReference { wrapper, var_name } => {
                assert_eq!(wrapper, "hsl");
                assert_eq!(var_name, "--primary");
            }
            other => panic!("expected WrappedVarReference, got: {other:?}"),
        }
    }

    #[test]
    fn test_unresolvable() {
        match parse_color("not-a-color-at-all") {
            ColorParseResult::Unresolvable(_) => {}
            other => panic!("expected Unresolvable, got: {other:?}"),
        }
    }

    #[test]
    fn test_empty() {
        match parse_color("") {
            ColorParseResult::Unresolvable(_) => {}
            other => panic!("expected Unresolvable, got: {other:?}"),
        }
    }
}

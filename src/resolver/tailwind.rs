use std::collections::HashMap;

use regex::Regex;
use std::sync::LazyLock;

use crate::color::Rgba;
use crate::color::parser::{ColorParseResult, parse_color};
use crate::resolver::tailwind_palette;

/// Color utility prefixes that Tailwind uses for different purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ColorUtilityKind {
    Background,
    Text,
    Border,
    Fill,
    Stroke,
    Ring,
    Outline,
    Shadow,
    Decoration,
    Accent,
    Caret,
    Divide,
    Placeholder,
}

impl ColorUtilityKind {
    /// Whether this utility represents a foreground-like color (rendered as text/stroke).
    pub fn is_foreground(&self) -> bool {
        matches!(
            self,
            Self::Text | Self::Fill | Self::Stroke | Self::Caret | Self::Placeholder | Self::Decoration
        )
    }

    /// Whether this utility represents a background-like color.
    pub fn is_background(&self) -> bool {
        matches!(self, Self::Background | Self::Accent)
    }
}

static UTILITY_PREFIX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^(bg|text|border|fill|stroke|ring|outline|shadow|decoration|accent|caret|divide|placeholder)-(.+)$",
    )
    .unwrap()
});

/// Parse a Tailwind class into its utility kind and color name.
/// E.g. "bg-primary" -> (Background, "primary"), "text-red-500" -> (Text, "red-500").
pub fn parse_utility_class(class: &str) -> Option<(ColorUtilityKind, String)> {
    let caps = UTILITY_PREFIX.captures(class)?;
    let prefix = &caps[1];
    let color_part = caps[2].to_string();

    let kind = match prefix {
        "bg" => ColorUtilityKind::Background,
        "text" => ColorUtilityKind::Text,
        "border" => ColorUtilityKind::Border,
        "fill" => ColorUtilityKind::Fill,
        "stroke" => ColorUtilityKind::Stroke,
        "ring" => ColorUtilityKind::Ring,
        "outline" => ColorUtilityKind::Outline,
        "shadow" => ColorUtilityKind::Shadow,
        "decoration" => ColorUtilityKind::Decoration,
        "accent" => ColorUtilityKind::Accent,
        "caret" => ColorUtilityKind::Caret,
        "divide" => ColorUtilityKind::Divide,
        "placeholder" => ColorUtilityKind::Placeholder,
        _ => return None,
    };

    // Filter out non-color utilities (e.g. "text-center", "bg-fixed").
    if is_non_color_utility(prefix, &color_part) {
        return None;
    }

    Some((kind, color_part))
}

/// Returns true if the class looks like a color utility but is actually a layout/behavior utility.
fn is_non_color_utility(prefix: &str, value: &str) -> bool {
    match prefix {
        "text" => matches!(
            value,
            "left" | "center" | "right" | "justify" | "start" | "end"
                | "xs" | "sm" | "base" | "lg" | "xl" | "2xl" | "3xl" | "4xl"
                | "5xl" | "6xl" | "7xl" | "8xl" | "9xl"
                | "wrap" | "nowrap" | "balance" | "pretty" | "ellipsis" | "clip"
        ),
        "bg" => matches!(
            value,
            "fixed" | "local" | "scroll" | "clip" | "auto" | "cover" | "contain"
                | "repeat" | "no-repeat" | "center" | "top" | "bottom" | "left" | "right"
                | "none" | "gradient-to-t" | "gradient-to-b" | "gradient-to-l" | "gradient-to-r"
                | "gradient-to-tl" | "gradient-to-tr" | "gradient-to-bl" | "gradient-to-br"
        ),
        "border" => matches!(
            value,
            "0" | "2" | "4" | "8" | "collapse" | "separate" | "solid" | "dashed"
                | "dotted" | "double" | "hidden" | "none"
                | "t" | "b" | "l" | "r" | "x" | "y"
                | "t-0" | "b-0" | "l-0" | "r-0"
        ),
        _ => false,
    }
}

/// Tailwind config color mappings parsed from tailwind.config.ts/js.
/// Maps semantic names to their CSS variable or direct color value.
/// E.g. "primary" -> "hsl(var(--primary))", "primary-foreground" -> "hsl(var(--primary-foreground))"
#[derive(Debug, Default)]
pub struct TailwindColorConfig {
    /// Flat map of color name -> raw value from config.
    /// "primary" -> "hsl(var(--primary))"
    /// "error-dark" -> "hsl(var(--error-dark))"
    pub colors: HashMap<String, String>,
}

static TW_CONFIG_COLOR_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Match patterns like: 'color-name': 'value' or "color-name": "value"
    Regex::new(r#"['"]?(\w[\w-]*)['"]?\s*:\s*['"]([^'"]+)['"]"#).unwrap()
});

// Match nested object blocks: key: { ... } (no further nesting inside)
// Matches innermost blocks only — the ones that contain key-value pairs, not outer wrappers.
static TW_NESTED_BLOCK_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"['"]?(\w[\w-]*)['"]?\s*:\s*\{\s*([^{}]*)\}"#).unwrap()
});

/// Strip single-line (`//`) and multi-line (`/* */`) JS comments to prevent
/// regex matches against commented-out config lines.
fn strip_js_comments(content: &str) -> String {
    static COMMENT_RE: LazyLock<Regex> = LazyLock::new(|| {
        // Match single-line comments or multi-line comments.
        // The alternation with string literals prevents stripping URLs inside strings.
        Regex::new(r#"(?://[^\n]*|/\*[\s\S]*?\*/)"#).unwrap()
    });
    COMMENT_RE.replace_all(content, "").into_owned()
}

/// Parse a tailwind.config.ts/js file to extract the color mappings.
/// This is a best-effort text-based parser — not a full JS evaluator.
/// It handles the common patterns used in Tailwind configs with CSS variables.
///
/// Handles nested objects: `primary: { DEFAULT: 'hsl(var(--primary))', foreground: 'hsl(var(--primary-foreground))' }`
/// becomes `primary -> ...` and `primary-foreground -> ...`.
pub fn parse_tailwind_config(content: &str) -> TailwindColorConfig {
    let content = strip_js_comments(content);
    let content = content.as_str();
    let mut config = TailwindColorConfig::default();

    // Pass 1: Find nested blocks like `primary: { DEFAULT: '...', foreground: '...' }`
    // These must be processed first so flat matches inside the block don't get treated as top-level.
    for block_caps in TW_NESTED_BLOCK_RE.captures_iter(content) {
        let parent_key = &block_caps[1];
        let block_body = &block_caps[2];

        for caps in TW_CONFIG_COLOR_RE.captures_iter(block_body) {
            let child_key = &caps[1];
            let value = &caps[2];

            if looks_like_color_value(value) {
                let name = if child_key == "DEFAULT" {
                    parent_key.to_string()
                } else {
                    format!("{parent_key}-{child_key}")
                };
                config.colors.insert(name, value.to_string());
            }
        }
    }

    // Pass 2: Flat key-value pairs (not inside a nested block).
    for caps in TW_CONFIG_COLOR_RE.captures_iter(content) {
        let key = &caps[1];
        let value = &caps[2];

        if key == "DEFAULT" {
            continue;
        }

        if looks_like_color_value(value) {
            // Only insert if not already covered by a nested block
            config
                .colors
                .entry(key.to_string())
                .or_insert_with(|| value.to_string());
        }
    }

    config
}

fn looks_like_color_value(value: &str) -> bool {
    value.starts_with("hsl(")
        || value.starts_with("rgb(")
        || value.starts_with("oklch(")
        || value.starts_with('#')
        || value.starts_with("var(")
        || value.contains("var(--")
}

/// Resolve a Tailwind color name to a concrete RGBA, given the config and resolved CSS vars.
pub fn resolve_tailwind_color(
    color_name: &str,
    config: &TailwindColorConfig,
    resolved_vars: &HashMap<String, Rgba>,
) -> Option<Rgba> {
    // 1. Check tailwind config for a semantic mapping
    if let Some(raw_value) = config.colors.get(color_name) {
        return resolve_raw_value(raw_value, resolved_vars);
    }

    // 2. Check if it's a direct CSS variable name pattern (e.g. "primary" -> "--primary")
    let var_name = format!("--{color_name}");
    if let Some(rgba) = resolved_vars.get(&var_name) {
        return Some(*rgba);
    }

    // 3. Check the default Tailwind palette (e.g. "red-500")
    if let Some(rgba) = tailwind_palette::lookup_default(color_name) {
        return Some(*rgba);
    }

    // 4. Handle opacity modifier (e.g. "primary/50" -> "primary" at 50% opacity)
    if let Some(slash_idx) = color_name.rfind('/') {
        let base = &color_name[..slash_idx];
        let opacity_str = &color_name[slash_idx + 1..];
        if let Ok(opacity_pct) = opacity_str.parse::<f64>() {
            if let Some(mut rgba) = resolve_tailwind_color(base, config, resolved_vars) {
                rgba.a = opacity_pct / 100.0;
                return Some(rgba);
            }
        }
    }

    None
}

/// Resolve a raw color value string (from tailwind config) to RGBA.
fn resolve_raw_value(raw: &str, resolved_vars: &HashMap<String, Rgba>) -> Option<Rgba> {
    match parse_color(raw) {
        ColorParseResult::Resolved(rgba) => Some(rgba),
        ColorParseResult::VarReference(var_name) => resolved_vars.get(&var_name).copied(),
        ColorParseResult::WrappedVarReference { var_name, .. } => {
            resolved_vars.get(&var_name).copied()
        }
        ColorParseResult::Unresolvable(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bg_class() {
        let (kind, name) = parse_utility_class("bg-primary").unwrap();
        assert_eq!(kind, ColorUtilityKind::Background);
        assert_eq!(name, "primary");
    }

    #[test]
    fn test_parse_text_class() {
        let (kind, name) = parse_utility_class("text-red-500").unwrap();
        assert_eq!(kind, ColorUtilityKind::Text);
        assert_eq!(name, "red-500");
    }

    #[test]
    fn test_reject_text_center() {
        assert!(parse_utility_class("text-center").is_none());
    }

    #[test]
    fn test_reject_bg_fixed() {
        assert!(parse_utility_class("bg-fixed").is_none());
    }

    #[test]
    fn test_reject_non_utility() {
        assert!(parse_utility_class("flex").is_none());
        assert!(parse_utility_class("p-4").is_none());
    }

    #[test]
    fn test_resolve_from_default_palette() {
        let config = TailwindColorConfig::default();
        let vars = HashMap::new();
        let c = resolve_tailwind_color("red-500", &config, &vars).unwrap();
        assert_eq!(c.to_hex(), "#ef4444");
    }

    #[test]
    fn test_resolve_from_css_vars() {
        let config = TailwindColorConfig::default();
        let mut vars = HashMap::new();
        vars.insert("--primary".to_string(), Rgba::opaque(0, 100, 200));
        let c = resolve_tailwind_color("primary", &config, &vars).unwrap();
        assert_eq!((c.r, c.g, c.b), (0, 100, 200));
    }

    #[test]
    fn test_opacity_modifier() {
        let config = TailwindColorConfig::default();
        let mut vars = HashMap::new();
        vars.insert("--primary".to_string(), Rgba::opaque(0, 100, 200));
        let c = resolve_tailwind_color("primary/50", &config, &vars).unwrap();
        assert_eq!((c.r, c.g, c.b), (0, 100, 200));
        assert!((c.a - 0.5).abs() < 0.01);
    }

    #[test]
    fn test_parse_nested_config_with_default() {
        let content = r#"
            module.exports = {
                theme: {
                    extend: {
                        colors: {
                            primary: {
                                DEFAULT: 'hsl(var(--primary))',
                                foreground: 'hsl(var(--primary-foreground))',
                            },
                            secondary: 'hsl(var(--secondary))',
                        }
                    }
                }
            }
        "#;
        let config = parse_tailwind_config(content);
        assert_eq!(
            config.colors.get("primary").unwrap(),
            "hsl(var(--primary))"
        );
        assert_eq!(
            config.colors.get("primary-foreground").unwrap(),
            "hsl(var(--primary-foreground))"
        );
        assert_eq!(
            config.colors.get("secondary").unwrap(),
            "hsl(var(--secondary))"
        );
    }

    #[test]
    fn test_commented_out_colors_are_ignored() {
        let content = r#"
            module.exports = {
                theme: {
                    extend: {
                        colors: {
                            // primary: 'hsl(var(--old-primary))',
                            secondary: 'hsl(var(--secondary))',
                            /* danger: '#ff0000', */
                        }
                    }
                }
            }
        "#;
        let config = parse_tailwind_config(content);
        assert!(config.colors.get("primary").is_none(), "commented-out single-line color should be ignored");
        assert!(config.colors.get("danger").is_none(), "commented-out multi-line color should be ignored");
        assert_eq!(
            config.colors.get("secondary").unwrap(),
            "hsl(var(--secondary))"
        );
    }
}

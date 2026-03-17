use regex::Regex;
use std::sync::LazyLock;

/// Extracted inline style color properties from a JSX style attribute.
#[derive(Debug, Default)]
pub struct InlineStyleColors {
    /// Value of `color` property if present.
    pub color: Option<String>,
    /// Value of `backgroundColor` property if present.
    pub background_color: Option<String>,
    /// Value of `fill` property if present.
    pub fill: Option<String>,
}

static STYLE_PROP_RE: LazyLock<Regex> = LazyLock::new(|| {
    // Match: propertyName: 'value' or propertyName: "value" or propertyName: `value`
    Regex::new(
        r#"(color|backgroundColor|fill)\s*:\s*['"`]([^'"`]+)['"`]"#,
    )
    .unwrap()
});

/// Extract color-related properties from a JSX inline style object string.
/// Input is the raw text content of a `style={{ ... }}` attribute.
pub fn extract_inline_colors(style_text: &str) -> InlineStyleColors {
    let mut result = InlineStyleColors::default();

    for caps in STYLE_PROP_RE.captures_iter(style_text) {
        let prop = &caps[1];
        let value = caps[2].trim().to_string();

        match prop {
            "color" => result.color = Some(value),
            "backgroundColor" => result.background_color = Some(value),
            "fill" => result.fill = Some(value),
            _ => {}
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_extraction() {
        let input = r#"{ backgroundColor: '#fff', color: 'red' }"#;
        let colors = extract_inline_colors(input);
        assert_eq!(colors.background_color.as_deref(), Some("#fff"));
        assert_eq!(colors.color.as_deref(), Some("red"));
    }

    #[test]
    fn test_hsl_value() {
        let input = r#"{ backgroundColor: 'hsl(0, 0%, 100%)' }"#;
        let colors = extract_inline_colors(input);
        assert_eq!(
            colors.background_color.as_deref(),
            Some("hsl(0, 0%, 100%)")
        );
    }

    #[test]
    fn test_no_colors() {
        let input = r#"{ padding: '10px', margin: '20px' }"#;
        let colors = extract_inline_colors(input);
        assert!(colors.color.is_none());
        assert!(colors.background_color.is_none());
    }
}

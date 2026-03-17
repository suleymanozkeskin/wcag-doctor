use crate::color::Rgba;

/// Linearize a single sRGB channel value (0-255) to linear light (0.0-1.0).
fn linearize(channel: u8) -> f64 {
    let c = channel as f64 / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

/// Calculate the WCAG 2.1 relative luminance of a color.
/// Returns a value in range [0.0, 1.0] where 0.0 is darkest and 1.0 is lightest.
///
/// Reference: https://www.w3.org/TR/WCAG21/#dfn-relative-luminance
pub fn relative_luminance(color: &Rgba) -> f64 {
    let r = linearize(color.r);
    let g = linearize(color.g);
    let b = linearize(color.b);
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// Composite a foreground color with alpha over a background using "source over" blending.
/// Returns a fully opaque color representing how the foreground actually appears.
fn composite_over(fg: &Rgba, bg: &Rgba) -> Rgba {
    let a = fg.a;
    let blend = |fc: u8, bc: u8| -> u8 {
        let f = fc as f64 / 255.0;
        let b = bc as f64 / 255.0;
        let c = f * a + b * (1.0 - a);
        (c * 255.0).round().clamp(0.0, 255.0) as u8
    };
    Rgba::opaque(blend(fg.r, bg.r), blend(fg.g, bg.g), blend(fg.b, bg.b))
}

/// Calculate the WCAG 2.1 contrast ratio between two colors.
/// Returns a value >= 1.0 (always lighter/darker, never inverted).
///
/// When either color has alpha < 1.0 the function composites it over the other color
/// (foreground over background by convention: color1 = fg, color2 = bg).
/// If the final backdrop is unknown, translucent pairs are evaluated over both
/// black and white backdrops and the lower contrast is reported conservatively.
///
/// Reference: https://www.w3.org/TR/WCAG21/#dfn-contrast-ratio
pub fn contrast_ratio(color1: &Rgba, color2: &Rgba) -> f64 {
    if color1.a >= 1.0 && color2.a >= 1.0 {
        return opaque_contrast_ratio(color1, color2);
    }

    let black = Rgba::opaque(0, 0, 0);
    let white = Rgba::opaque(255, 255, 255);
    contrast_ratio_over_backdrop(color1, color2, &black)
        .min(contrast_ratio_over_backdrop(color1, color2, &white))
}

fn contrast_ratio_over_backdrop(color1: &Rgba, color2: &Rgba, backdrop: &Rgba) -> f64 {
    let bg = if color2.a < 1.0 {
        composite_over(color2, backdrop)
    } else {
        *color2
    };

    let fg = if color1.a < 1.0 {
        composite_over(color1, &bg)
    } else {
        *color1
    };

    opaque_contrast_ratio(&fg, &bg)
}

fn opaque_contrast_ratio(color1: &Rgba, color2: &Rgba) -> f64 {
    let l1 = relative_luminance(color1);
    let l2 = relative_luminance(color2);
    let lighter = l1.max(l2);
    let darker = l1.min(l2);
    (lighter + 0.05) / (darker + 0.05)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_black_luminance() {
        let black = Rgba::opaque(0, 0, 0);
        assert!((relative_luminance(&black) - 0.0).abs() < 0.001);
    }

    #[test]
    fn test_white_luminance() {
        let white = Rgba::opaque(255, 255, 255);
        assert!((relative_luminance(&white) - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_max_contrast() {
        let black = Rgba::opaque(0, 0, 0);
        let white = Rgba::opaque(255, 255, 255);
        let ratio = contrast_ratio(&black, &white);
        assert!((ratio - 21.0).abs() < 0.1);
    }

    #[test]
    fn test_same_color_contrast() {
        let red = Rgba::opaque(255, 0, 0);
        let ratio = contrast_ratio(&red, &red);
        assert!((ratio - 1.0).abs() < 0.001);
    }

    #[test]
    fn test_contrast_is_symmetric() {
        let a = Rgba::opaque(100, 50, 200);
        let b = Rgba::opaque(200, 200, 50);
        let r1 = contrast_ratio(&a, &b);
        let r2 = contrast_ratio(&b, &a);
        assert!((r1 - r2).abs() < 0.001);
    }

    #[test]
    fn test_known_contrast() {
        // Pure blue (#0000ff) on white (#ffffff)
        // Known ratio is approximately 8.59:1
        let blue = Rgba::opaque(0, 0, 255);
        let white = Rgba::opaque(255, 255, 255);
        let ratio = contrast_ratio(&blue, &white);
        assert!((ratio - 8.59).abs() < 0.1, "blue on white ratio: {ratio}");
    }

    #[test]
    fn test_semi_transparent_fg() {
        // 50% black on white should composite to grey, ratio ~= 3.95:1
        let half_black = Rgba::new(0, 0, 0, 0.5);
        let white = Rgba::opaque(255, 255, 255);
        let ratio = contrast_ratio(&half_black, &white);
        // Composited color is ~(128,128,128) on white
        assert!(ratio > 3.0 && ratio < 5.0, "half-black on white: {ratio}");
    }

    #[test]
    fn test_semi_transparent_bg() {
        // Conservative worst-case should consider both black and white backdrops.
        // For black text on 50% black background:
        // over white => black on grey(128) ≈ 5.32:1
        // over black => black on black = 1:1
        let black = Rgba::opaque(0, 0, 0);
        let half_black = Rgba::new(0, 0, 0, 0.5);
        let ratio = contrast_ratio(&black, &half_black);
        assert!((ratio - 1.0).abs() < 0.01, "black on half-black: {ratio}");
    }

    #[test]
    fn test_transparent_is_worst_case() {
        // Fully transparent foreground can collapse to backdrop/background identity,
        // so the conservative result remains 1:1.
        let transparent = Rgba::new(0, 0, 0, 0.0);
        let white = Rgba::opaque(255, 255, 255);
        let ratio = contrast_ratio(&transparent, &white);
        assert!((ratio - 1.0).abs() < 0.01, "transparent on white: {ratio}");
    }

    #[test]
    fn test_semi_transparent_bg_uses_lower_of_black_and_white_backdrops() {
        let white = Rgba::opaque(255, 255, 255);
        let half_white = Rgba::new(255, 255, 255, 0.5);
        let ratio = contrast_ratio(&white, &half_white);
        assert!((ratio - 1.0).abs() < 0.01, "white on half-white: {ratio}");
    }
}

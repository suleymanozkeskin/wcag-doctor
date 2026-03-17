use super::Rgba;

/// Convert HSL to RGB.
/// h: degrees (0-360), s: fraction (0.0-1.0), l: fraction (0.0-1.0).
pub fn hsl_to_rgb(h: f64, s: f64, l: f64) -> (u8, u8, u8) {
    if s == 0.0 {
        let v = (l * 255.0).round() as u8;
        return (v, v, v);
    }

    let h = ((h % 360.0) + 360.0) % 360.0;

    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let h_prime = h / 60.0;
    let x = c * (1.0 - (h_prime % 2.0 - 1.0).abs());
    let m = l - c / 2.0;

    let (r1, g1, b1) = match h_prime as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };

    let to_byte = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    (to_byte(r1), to_byte(g1), to_byte(b1))
}

/// Convert HSL + alpha to RGBA.
pub fn hsla_to_rgba(h: f64, s: f64, l: f64, a: f64) -> Rgba {
    let (r, g, b) = hsl_to_rgb(h, s, l);
    Rgba::new(r, g, b, a)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pure_red() {
        let (r, g, b) = hsl_to_rgb(0.0, 1.0, 0.5);
        assert_eq!((r, g, b), (255, 0, 0));
    }

    #[test]
    fn test_pure_green() {
        let (r, g, b) = hsl_to_rgb(120.0, 1.0, 0.5);
        assert_eq!((r, g, b), (0, 255, 0));
    }

    #[test]
    fn test_pure_blue() {
        let (r, g, b) = hsl_to_rgb(240.0, 1.0, 0.5);
        assert_eq!((r, g, b), (0, 0, 255));
    }

    #[test]
    fn test_white() {
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 1.0);
        assert_eq!((r, g, b), (255, 255, 255));
    }

    #[test]
    fn test_black() {
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 0.0);
        assert_eq!((r, g, b), (0, 0, 0));
    }

    #[test]
    fn test_grey() {
        let (r, g, b) = hsl_to_rgb(0.0, 0.0, 0.5);
        assert_eq!((r, g, b), (128, 128, 128));
    }
}

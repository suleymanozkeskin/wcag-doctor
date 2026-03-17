use super::Rgba;

/// Convert oklch(L, C, H) to RGBA.
/// L: lightness (0.0-1.0), C: chroma (0.0-~0.4), H: hue degrees (0-360).
pub fn oklch_to_rgba(l: f64, c: f64, h_deg: f64, alpha: f64) -> Rgba {
    let h_rad = h_deg.to_radians();
    let a = c * h_rad.cos();
    let b = c * h_rad.sin();

    // OKLab to linear sRGB via the LMS intermediate
    let l_ = l + 0.3963377774 * a + 0.2158037573 * b;
    let m_ = l - 0.1055613458 * a - 0.0638541728 * b;
    let s_ = l - 0.0894841775 * a - 1.2914855480 * b;

    let l_cubed = l_ * l_ * l_;
    let m_cubed = m_ * m_ * m_;
    let s_cubed = s_ * s_ * s_;

    let r_lin = 4.0767416621 * l_cubed - 3.3077115913 * m_cubed + 0.2309699292 * s_cubed;
    let g_lin = -1.2684380046 * l_cubed + 2.6097574011 * m_cubed - 0.3413193965 * s_cubed;
    let b_lin = -0.0041960863 * l_cubed - 0.7034186147 * m_cubed + 1.7076147010 * s_cubed;

    let to_srgb = |v: f64| -> u8 {
        let clamped = v.clamp(0.0, 1.0);
        let gamma = if clamped <= 0.0031308 {
            clamped * 12.92
        } else {
            1.055 * clamped.powf(1.0 / 2.4) - 0.055
        };
        (gamma * 255.0).round().clamp(0.0, 255.0) as u8
    };

    Rgba::new(to_srgb(r_lin), to_srgb(g_lin), to_srgb(b_lin), alpha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_black() {
        let c = oklch_to_rgba(0.0, 0.0, 0.0, 1.0);
        assert_eq!(c.r, 0);
        assert_eq!(c.g, 0);
        assert_eq!(c.b, 0);
    }

    #[test]
    fn test_white() {
        let c = oklch_to_rgba(1.0, 0.0, 0.0, 1.0);
        assert_eq!(c.r, 255);
        assert_eq!(c.g, 255);
        assert_eq!(c.b, 255);
    }
}

pub mod hsl;
pub mod named_colors;
pub mod oklch;
pub mod parser;

/// RGBA color with channels in 0-255 range and alpha in 0.0-1.0.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: f64,
}

impl Rgba {
    pub fn new(r: u8, g: u8, b: u8, a: f64) -> Self {
        Self { r, g, b, a }
    }

    pub fn opaque(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 1.0 }
    }

    /// Format as hex string (#rrggbb or #rrggbbaa if alpha < 1.0).
    pub fn to_hex(&self) -> String {
        let alpha_byte = (self.a * 255.0).round() as u8;
        if alpha_byte == u8::MAX {
            format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
        } else {
            format!("#{:02x}{:02x}{:02x}{:02x}", self.r, self.g, self.b, alpha_byte)
        }
    }
}

impl std::fmt::Display for Rgba {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.to_hex())
    }
}

#[cfg(test)]
mod tests {
    use super::Rgba;

    #[test]
    fn to_hex_treats_rounded_opaque_alpha_as_rgb() {
        let color = Rgba::new(0x12, 0x34, 0x56, 0.999_999);
        assert_eq!(color.to_hex(), "#123456");
    }

    #[test]
    fn to_hex_keeps_visible_alpha_channel() {
        let color = Rgba::new(0x12, 0x34, 0x56, 0.99);
        assert_eq!(color.to_hex(), "#123456fc");
    }
}

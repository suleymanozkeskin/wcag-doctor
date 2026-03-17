use std::collections::HashMap;
use std::sync::LazyLock;

use crate::color::Rgba;

/// Tailwind CSS v3 default color palette.
/// Each entry maps "color-shade" (e.g. "red-500") to its hex value.
pub static DEFAULT_PALETTE: LazyLock<HashMap<&'static str, Rgba>> = LazyLock::new(build_palette);

fn hex(s: &str) -> Rgba {
    let s = s.trim_start_matches('#');
    let r = u8::from_str_radix(&s[0..2], 16).unwrap();
    let g = u8::from_str_radix(&s[2..4], 16).unwrap();
    let b = u8::from_str_radix(&s[4..6], 16).unwrap();
    Rgba::opaque(r, g, b)
}

fn build_palette() -> HashMap<&'static str, Rgba> {
    let mut m = HashMap::new();

    // Slate
    m.insert("slate-50", hex("f8fafc"));
    m.insert("slate-100", hex("f1f5f9"));
    m.insert("slate-200", hex("e2e8f0"));
    m.insert("slate-300", hex("cbd5e1"));
    m.insert("slate-400", hex("94a3b8"));
    m.insert("slate-500", hex("64748b"));
    m.insert("slate-600", hex("475569"));
    m.insert("slate-700", hex("334155"));
    m.insert("slate-800", hex("1e293b"));
    m.insert("slate-900", hex("0f172a"));
    m.insert("slate-950", hex("020617"));

    // Gray
    m.insert("gray-50", hex("f9fafb"));
    m.insert("gray-100", hex("f3f4f6"));
    m.insert("gray-200", hex("e5e7eb"));
    m.insert("gray-300", hex("d1d5db"));
    m.insert("gray-400", hex("9ca3af"));
    m.insert("gray-500", hex("6b7280"));
    m.insert("gray-600", hex("4b5563"));
    m.insert("gray-700", hex("374151"));
    m.insert("gray-800", hex("1f2937"));
    m.insert("gray-900", hex("111827"));
    m.insert("gray-950", hex("030712"));

    // Zinc
    m.insert("zinc-50", hex("fafafa"));
    m.insert("zinc-100", hex("f4f4f5"));
    m.insert("zinc-200", hex("e4e4e7"));
    m.insert("zinc-300", hex("d4d4d8"));
    m.insert("zinc-400", hex("a1a1aa"));
    m.insert("zinc-500", hex("71717a"));
    m.insert("zinc-600", hex("52525b"));
    m.insert("zinc-700", hex("3f3f46"));
    m.insert("zinc-800", hex("27272a"));
    m.insert("zinc-900", hex("18181b"));
    m.insert("zinc-950", hex("09090b"));

    // Neutral
    m.insert("neutral-50", hex("fafafa"));
    m.insert("neutral-100", hex("f5f5f5"));
    m.insert("neutral-200", hex("e5e5e5"));
    m.insert("neutral-300", hex("d4d4d4"));
    m.insert("neutral-400", hex("a3a3a3"));
    m.insert("neutral-500", hex("737373"));
    m.insert("neutral-600", hex("525252"));
    m.insert("neutral-700", hex("404040"));
    m.insert("neutral-800", hex("262626"));
    m.insert("neutral-900", hex("171717"));
    m.insert("neutral-950", hex("0a0a0a"));

    // Stone
    m.insert("stone-50", hex("fafaf9"));
    m.insert("stone-100", hex("f5f5f4"));
    m.insert("stone-200", hex("e7e5e4"));
    m.insert("stone-300", hex("d6d3d1"));
    m.insert("stone-400", hex("a8a29e"));
    m.insert("stone-500", hex("78716c"));
    m.insert("stone-600", hex("57534e"));
    m.insert("stone-700", hex("44403c"));
    m.insert("stone-800", hex("292524"));
    m.insert("stone-900", hex("1c1917"));
    m.insert("stone-950", hex("0c0a09"));

    // Red
    m.insert("red-50", hex("fef2f2"));
    m.insert("red-100", hex("fee2e2"));
    m.insert("red-200", hex("fecaca"));
    m.insert("red-300", hex("fca5a5"));
    m.insert("red-400", hex("f87171"));
    m.insert("red-500", hex("ef4444"));
    m.insert("red-600", hex("dc2626"));
    m.insert("red-700", hex("b91c1c"));
    m.insert("red-800", hex("991b1b"));
    m.insert("red-900", hex("7f1d1d"));
    m.insert("red-950", hex("450a0a"));

    // Orange
    m.insert("orange-50", hex("fff7ed"));
    m.insert("orange-100", hex("ffedd5"));
    m.insert("orange-200", hex("fed7aa"));
    m.insert("orange-300", hex("fdba74"));
    m.insert("orange-400", hex("fb923c"));
    m.insert("orange-500", hex("f97316"));
    m.insert("orange-600", hex("ea580c"));
    m.insert("orange-700", hex("c2410c"));
    m.insert("orange-800", hex("9a3412"));
    m.insert("orange-900", hex("7c2d12"));
    m.insert("orange-950", hex("431407"));

    // Amber
    m.insert("amber-50", hex("fffbeb"));
    m.insert("amber-100", hex("fef3c7"));
    m.insert("amber-200", hex("fde68a"));
    m.insert("amber-300", hex("fcd34d"));
    m.insert("amber-400", hex("fbbf24"));
    m.insert("amber-500", hex("f59e0b"));
    m.insert("amber-600", hex("d97706"));
    m.insert("amber-700", hex("b45309"));
    m.insert("amber-800", hex("92400e"));
    m.insert("amber-900", hex("78350f"));
    m.insert("amber-950", hex("451a03"));

    // Yellow
    m.insert("yellow-50", hex("fefce8"));
    m.insert("yellow-100", hex("fef9c3"));
    m.insert("yellow-200", hex("fef08a"));
    m.insert("yellow-300", hex("fde047"));
    m.insert("yellow-400", hex("facc15"));
    m.insert("yellow-500", hex("eab308"));
    m.insert("yellow-600", hex("ca8a04"));
    m.insert("yellow-700", hex("a16207"));
    m.insert("yellow-800", hex("854d0e"));
    m.insert("yellow-900", hex("713f12"));
    m.insert("yellow-950", hex("422006"));

    // Lime
    m.insert("lime-50", hex("f7fee7"));
    m.insert("lime-100", hex("ecfccb"));
    m.insert("lime-200", hex("d9f99d"));
    m.insert("lime-300", hex("bef264"));
    m.insert("lime-400", hex("a3e635"));
    m.insert("lime-500", hex("84cc16"));
    m.insert("lime-600", hex("65a30d"));
    m.insert("lime-700", hex("4d7c0f"));
    m.insert("lime-800", hex("3f6212"));
    m.insert("lime-900", hex("365314"));
    m.insert("lime-950", hex("1a2e05"));

    // Green
    m.insert("green-50", hex("f0fdf4"));
    m.insert("green-100", hex("dcfce7"));
    m.insert("green-200", hex("bbf7d0"));
    m.insert("green-300", hex("86efac"));
    m.insert("green-400", hex("4ade80"));
    m.insert("green-500", hex("22c55e"));
    m.insert("green-600", hex("16a34a"));
    m.insert("green-700", hex("15803d"));
    m.insert("green-800", hex("166534"));
    m.insert("green-900", hex("14532d"));
    m.insert("green-950", hex("052e16"));

    // Emerald
    m.insert("emerald-50", hex("ecfdf5"));
    m.insert("emerald-100", hex("d1fae5"));
    m.insert("emerald-200", hex("a7f3d0"));
    m.insert("emerald-300", hex("6ee7b7"));
    m.insert("emerald-400", hex("34d399"));
    m.insert("emerald-500", hex("10b981"));
    m.insert("emerald-600", hex("059669"));
    m.insert("emerald-700", hex("047857"));
    m.insert("emerald-800", hex("065f46"));
    m.insert("emerald-900", hex("064e3b"));
    m.insert("emerald-950", hex("022c22"));

    // Teal
    m.insert("teal-50", hex("f0fdfa"));
    m.insert("teal-100", hex("ccfbf1"));
    m.insert("teal-200", hex("99f6e4"));
    m.insert("teal-300", hex("5eead4"));
    m.insert("teal-400", hex("2dd4bf"));
    m.insert("teal-500", hex("14b8a6"));
    m.insert("teal-600", hex("0d9488"));
    m.insert("teal-700", hex("0f766e"));
    m.insert("teal-800", hex("115e59"));
    m.insert("teal-900", hex("134e4a"));
    m.insert("teal-950", hex("042f2e"));

    // Cyan
    m.insert("cyan-50", hex("ecfeff"));
    m.insert("cyan-100", hex("cffafe"));
    m.insert("cyan-200", hex("a5f3fc"));
    m.insert("cyan-300", hex("67e8f9"));
    m.insert("cyan-400", hex("22d3ee"));
    m.insert("cyan-500", hex("06b6d4"));
    m.insert("cyan-600", hex("0891b2"));
    m.insert("cyan-700", hex("0e7490"));
    m.insert("cyan-800", hex("155e75"));
    m.insert("cyan-900", hex("164e63"));
    m.insert("cyan-950", hex("083344"));

    // Sky
    m.insert("sky-50", hex("f0f9ff"));
    m.insert("sky-100", hex("e0f2fe"));
    m.insert("sky-200", hex("bae6fd"));
    m.insert("sky-300", hex("7dd3fc"));
    m.insert("sky-400", hex("38bdf8"));
    m.insert("sky-500", hex("0ea5e9"));
    m.insert("sky-600", hex("0284c7"));
    m.insert("sky-700", hex("0369a1"));
    m.insert("sky-800", hex("075985"));
    m.insert("sky-900", hex("0c4a6e"));
    m.insert("sky-950", hex("082f49"));

    // Blue
    m.insert("blue-50", hex("eff6ff"));
    m.insert("blue-100", hex("dbeafe"));
    m.insert("blue-200", hex("bfdbfe"));
    m.insert("blue-300", hex("93c5fd"));
    m.insert("blue-400", hex("60a5fa"));
    m.insert("blue-500", hex("3b82f6"));
    m.insert("blue-600", hex("2563eb"));
    m.insert("blue-700", hex("1d4ed8"));
    m.insert("blue-800", hex("1e40af"));
    m.insert("blue-900", hex("1e3a8a"));
    m.insert("blue-950", hex("172554"));

    // Indigo
    m.insert("indigo-50", hex("eef2ff"));
    m.insert("indigo-100", hex("e0e7ff"));
    m.insert("indigo-200", hex("c7d2fe"));
    m.insert("indigo-300", hex("a5b4fc"));
    m.insert("indigo-400", hex("818cf8"));
    m.insert("indigo-500", hex("6366f1"));
    m.insert("indigo-600", hex("4f46e5"));
    m.insert("indigo-700", hex("4338ca"));
    m.insert("indigo-800", hex("3730a3"));
    m.insert("indigo-900", hex("312e81"));
    m.insert("indigo-950", hex("1e1b4b"));

    // Violet
    m.insert("violet-50", hex("f5f3ff"));
    m.insert("violet-100", hex("ede9fe"));
    m.insert("violet-200", hex("ddd6fe"));
    m.insert("violet-300", hex("c4b5fd"));
    m.insert("violet-400", hex("a78bfa"));
    m.insert("violet-500", hex("8b5cf6"));
    m.insert("violet-600", hex("7c3aed"));
    m.insert("violet-700", hex("6d28d9"));
    m.insert("violet-800", hex("5b21b6"));
    m.insert("violet-900", hex("4c1d95"));
    m.insert("violet-950", hex("2e1065"));

    // Purple
    m.insert("purple-50", hex("faf5ff"));
    m.insert("purple-100", hex("f3e8ff"));
    m.insert("purple-200", hex("e9d5ff"));
    m.insert("purple-300", hex("d8b4fe"));
    m.insert("purple-400", hex("c084fc"));
    m.insert("purple-500", hex("a855f7"));
    m.insert("purple-600", hex("9333ea"));
    m.insert("purple-700", hex("7e22ce"));
    m.insert("purple-800", hex("6b21a8"));
    m.insert("purple-900", hex("581c87"));
    m.insert("purple-950", hex("3b0764"));

    // Fuchsia
    m.insert("fuchsia-50", hex("fdf4ff"));
    m.insert("fuchsia-100", hex("fae8ff"));
    m.insert("fuchsia-200", hex("f5d0fe"));
    m.insert("fuchsia-300", hex("f0abfc"));
    m.insert("fuchsia-400", hex("e879f9"));
    m.insert("fuchsia-500", hex("d946ef"));
    m.insert("fuchsia-600", hex("c026d3"));
    m.insert("fuchsia-700", hex("a21caf"));
    m.insert("fuchsia-800", hex("86198f"));
    m.insert("fuchsia-900", hex("701a75"));
    m.insert("fuchsia-950", hex("4a044e"));

    // Pink
    m.insert("pink-50", hex("fdf2f8"));
    m.insert("pink-100", hex("fce7f3"));
    m.insert("pink-200", hex("fbcfe8"));
    m.insert("pink-300", hex("f9a8d4"));
    m.insert("pink-400", hex("f472b6"));
    m.insert("pink-500", hex("ec4899"));
    m.insert("pink-600", hex("db2777"));
    m.insert("pink-700", hex("be185d"));
    m.insert("pink-800", hex("9d174d"));
    m.insert("pink-900", hex("831843"));
    m.insert("pink-950", hex("500724"));

    // Rose
    m.insert("rose-50", hex("fff1f2"));
    m.insert("rose-100", hex("ffe4e6"));
    m.insert("rose-200", hex("fecdd3"));
    m.insert("rose-300", hex("fda4af"));
    m.insert("rose-400", hex("fb7185"));
    m.insert("rose-500", hex("f43f5e"));
    m.insert("rose-600", hex("e11d48"));
    m.insert("rose-700", hex("be123c"));
    m.insert("rose-800", hex("9f1239"));
    m.insert("rose-900", hex("881337"));
    m.insert("rose-950", hex("4c0519"));

    // Special
    m.insert("white", hex("ffffff"));
    m.insert("black", hex("000000"));

    m
}

/// Look up a Tailwind default palette color by its name (e.g. "red-500").
pub fn lookup_default(name: &str) -> Option<&'static Rgba> {
    DEFAULT_PALETTE.get(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lookup_red_500() {
        let c = lookup_default("red-500").unwrap();
        assert_eq!(c.to_hex(), "#ef4444");
    }

    #[test]
    fn test_lookup_white() {
        let c = lookup_default("white").unwrap();
        assert_eq!((c.r, c.g, c.b), (255, 255, 255));
    }

    #[test]
    fn test_lookup_missing() {
        assert!(lookup_default("imaginary-500").is_none());
    }
}

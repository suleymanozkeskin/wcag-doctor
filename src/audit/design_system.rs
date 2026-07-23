use std::collections::HashMap;

use crate::color::Rgba;
use crate::contrast::levels::ConformanceLevel;
use crate::contrast::wcag::{contrast_over_backdrops, contrast_ratio};
use crate::wcag_config::SurfaceRule;

/// A contrast check result for a design system color pair.
#[derive(Debug)]
pub struct DesignSystemPair {
    pub theme: String,
    pub foreground_name: String,
    pub foreground_color: Rgba,
    pub background_name: String,
    pub background_color: Rgba,
    pub ratio: f64,
    pub level: ConformanceLevel,
    /// The (translucent) background was composited over the configured backdrop
    /// samples rather than evaluated as an opaque or unknown surface.
    pub over_backdrop: bool,
}

/// Audit all semantic color pairs in the design system.
///
/// Checks foreground/background pairs that are conventionally used together, plus
/// any explicit `surfaces` from the project config. Translucent backgrounds are
/// composited over the theme's backdrop samples (worst-case) when samples exist.
pub fn audit_design_system(
    light_vars: &HashMap<String, Rgba>,
    dark_vars: &HashMap<String, Rgba>,
    check_light: bool,
    check_dark: bool,
    light_backdrops: &[Rgba],
    dark_backdrops: &[Rgba],
    surfaces: &[SurfaceRule],
) -> Vec<DesignSystemPair> {
    let mut results = Vec::new();

    if check_light {
        audit_theme(light_vars, "light", light_backdrops, surfaces, &mut results);
    }
    if check_dark {
        audit_theme(dark_vars, "dark", dark_backdrops, surfaces, &mut results);
    }

    results
}

/// Push a foreground/background pair. A translucent background is composited over
/// the backdrop samples (worst-case); an opaque background, or an empty backdrop,
/// uses the plain black/white-safe ratio.
fn push_pair(
    results: &mut Vec<DesignSystemPair>,
    theme_name: &str,
    fg_name: String,
    fg: &Rgba,
    bg_name: String,
    bg: &Rgba,
    backdrops: &[Rgba],
) {
    let over_backdrop = bg.a < 1.0 && !backdrops.is_empty();
    let ratio = if over_backdrop {
        contrast_over_backdrops(fg, bg, backdrops)
    } else {
        contrast_ratio(fg, bg)
    };
    results.push(DesignSystemPair {
        theme: theme_name.to_string(),
        foreground_name: fg_name,
        foreground_color: *fg,
        background_name: bg_name,
        background_color: *bg,
        ratio,
        level: ConformanceLevel::from_ratio(ratio),
        over_backdrop,
    });
}

/// Push a pair by variable name, resolving both sides in `vars` first.
fn push_named(
    results: &mut Vec<DesignSystemPair>,
    theme_name: &str,
    vars: &HashMap<String, Rgba>,
    fg_key: &str,
    bg_key: &str,
    backdrops: &[Rgba],
) {
    if let (Some(fg), Some(bg)) = (vars.get(fg_key), vars.get(bg_key)) {
        push_pair(
            results,
            theme_name,
            fg_key.to_string(),
            fg,
            bg_key.to_string(),
            bg,
            backdrops,
        );
    }
}

fn audit_theme(
    vars: &HashMap<String, Rgba>,
    theme_name: &str,
    backdrops: &[Rgba],
    surfaces: &[SurfaceRule],
    results: &mut Vec<DesignSystemPair>,
) {
    // 1. Explicit foreground/background pairs: --X-foreground on --X
    let mut semantic_bases: Vec<String> = vars
        .keys()
        .filter_map(|k| {
            let stripped = k.strip_prefix("--")?;
            let base = stripped.strip_suffix("-foreground")?;
            Some(base.to_string())
        })
        .collect();
    semantic_bases.sort(); // deterministic ordering (HashMap iteration is not)

    for base in &semantic_bases {
        push_named(
            results,
            theme_name,
            vars,
            &format!("--{base}-foreground"),
            &format!("--{base}"),
            backdrops,
        );
    }

    // 2. Implicit pair: --foreground on --background
    push_named(results, theme_name, vars, "--foreground", "--background", backdrops);

    // 3. Status colors on background
    for status in ["success", "error", "warning", "info"] {
        push_named(
            results,
            theme_name,
            vars,
            &format!("--{status}"),
            "--background",
            backdrops,
        );
    }

    // 4-9. Common semantic pairs the naming convention does not pair automatically.
    for (fg_key, bg_key) in [
        ("--muted-foreground", "--background"),
        ("--muted-foreground", "--muted"),
        ("--border", "--background"),
        ("--ring", "--background"),
        ("--sidebar-foreground", "--sidebar"),
        ("--table-header-fg", "--table-header-bg"),
    ] {
        push_named(results, theme_name, vars, fg_key, bg_key, backdrops);
    }

    // 10. Config-declared surfaces (frosted glass tiers, chrome, fields) that the
    //     naming convention misses. Composited over the backdrop unless opted out.
    for surface in surfaces {
        let Some(bg) = vars.get(&surface.background) else {
            continue;
        };
        let surface_backdrops: &[Rgba] = if surface.over_backdrop { backdrops } else { &[] };
        for fg_name in &surface.foregrounds {
            if let Some(fg) = vars.get(fg_name) {
                push_pair(
                    results,
                    theme_name,
                    fg_name.clone(),
                    fg,
                    surface.background.clone(),
                    bg,
                    surface_backdrops,
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_audit() {
        let mut light = HashMap::new();
        light.insert("--background".to_string(), Rgba::opaque(255, 255, 255));
        light.insert("--foreground".to_string(), Rgba::opaque(0, 0, 0));
        light.insert("--primary".to_string(), Rgba::opaque(0, 100, 200));
        light.insert(
            "--primary-foreground".to_string(),
            Rgba::opaque(255, 255, 255),
        );

        let dark = HashMap::new();

        let results = audit_design_system(&light, &dark, true, false, &[], &[], &[]);
        assert!(results.len() >= 2); // At least foreground/background + primary pair
    }

    #[test]
    fn test_max_contrast_passes() {
        let mut light = HashMap::new();
        light.insert("--background".to_string(), Rgba::opaque(255, 255, 255));
        light.insert("--foreground".to_string(), Rgba::opaque(0, 0, 0));

        let results = audit_design_system(&light, &HashMap::new(), true, false, &[], &[], &[]);
        let fg_bg = results
            .iter()
            .find(|p| p.foreground_name == "--foreground" && p.background_name == "--background");
        assert!(fg_bg.is_some());
        assert_eq!(fg_bg.unwrap().level, ConformanceLevel::Aaa);
    }

    #[test]
    fn test_configured_surface_composited_over_backdrop() {
        // A near-white translucent glass surface over a dark backdrop, with dark
        // text. Over the dark backdrop the surface stays light, so dark text on it
        // reads with high contrast — and the pair is marked over_backdrop.
        let mut light = HashMap::new();
        light.insert("--glass-surface".to_string(), Rgba::new(255, 255, 255, 0.62));
        light.insert("--foreground".to_string(), Rgba::opaque(15, 20, 25));

        let dark_backdrop = [Rgba::opaque(20, 30, 40)];
        let surfaces = vec![SurfaceRule {
            background: "--glass-surface".to_string(),
            foregrounds: vec!["--foreground".to_string()],
            over_backdrop: true,
        }];

        let results =
            audit_design_system(&light, &HashMap::new(), true, false, &dark_backdrop, &[], &surfaces);
        let pair = results
            .iter()
            .find(|p| p.background_name == "--glass-surface")
            .expect("surface pair present");
        assert!(pair.over_backdrop, "translucent surface should note over_backdrop");
        assert!(
            pair.ratio > 4.5,
            "dark text on a light glass over dark backdrop should pass AA, got {}",
            pair.ratio
        );
    }

    #[test]
    fn test_translucent_conventional_pair_uses_backdrop() {
        // The --sidebar-foreground / --sidebar pair with a translucent --sidebar
        // should composite over the backdrop when samples are provided.
        let mut light = HashMap::new();
        light.insert("--sidebar".to_string(), Rgba::new(250, 250, 250, 0.55));
        light.insert("--sidebar-foreground".to_string(), Rgba::opaque(60, 60, 70));

        let backdrop = [Rgba::opaque(200, 210, 220)];
        let results =
            audit_design_system(&light, &HashMap::new(), true, false, &backdrop, &[], &[]);
        let pair = results
            .iter()
            .find(|p| p.background_name == "--sidebar")
            .expect("sidebar pair present");
        assert!(pair.over_backdrop);
    }
}

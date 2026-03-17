use std::collections::HashMap;

use crate::color::Rgba;
use crate::contrast::levels::ConformanceLevel;
use crate::contrast::wcag::contrast_ratio;

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
}

/// Audit all semantic color pairs in the design system.
/// Checks foreground/background pairs that are conventionally used together.
pub fn audit_design_system(
    light_vars: &HashMap<String, Rgba>,
    dark_vars: &HashMap<String, Rgba>,
    check_light: bool,
    check_dark: bool,
) -> Vec<DesignSystemPair> {
    let mut results = Vec::new();

    if check_light {
        audit_theme(light_vars, "light", &mut results);
    }
    if check_dark {
        audit_theme(dark_vars, "dark", &mut results);
    }

    results
}

fn audit_theme(vars: &HashMap<String, Rgba>, theme_name: &str, results: &mut Vec<DesignSystemPair>) {
    // 1. Explicit foreground/background pairs: --X-foreground on --X
    let semantic_bases: Vec<String> = vars
        .keys()
        .filter_map(|k| {
            let stripped = k.strip_prefix("--")?;
            if stripped.ends_with("-foreground") {
                let base = stripped.strip_suffix("-foreground")?;
                Some(base.to_string())
            } else {
                None
            }
        })
        .collect();

    for base in &semantic_bases {
        let fg_key = format!("--{base}-foreground");
        let bg_key = format!("--{base}");

        if let (Some(fg), Some(bg)) = (vars.get(&fg_key), vars.get(&bg_key)) {
            let ratio = contrast_ratio(fg, bg);
            results.push(DesignSystemPair {
                theme: theme_name.to_string(),
                foreground_name: fg_key,
                foreground_color: *fg,
                background_name: bg_key,
                background_color: *bg,
                ratio,
                level: ConformanceLevel::from_ratio(ratio),
            });
        }
    }

    // 2. Implicit pair: --foreground on --background
    if let (Some(fg), Some(bg)) = (vars.get("--foreground"), vars.get("--background")) {
        let ratio = contrast_ratio(fg, bg);
        results.push(DesignSystemPair {
            theme: theme_name.to_string(),
            foreground_name: "--foreground".to_string(),
            foreground_color: *fg,
            background_name: "--background".to_string(),
            background_color: *bg,
            ratio,
            level: ConformanceLevel::from_ratio(ratio),
        });
    }

    // 3. Status colors on background: --success, --error, --warning, --info on --background
    let status_colors = ["success", "error", "warning", "info"];
    if let Some(bg) = vars.get("--background") {
        for status in &status_colors {
            let key = format!("--{status}");
            if let Some(fg) = vars.get(&key) {
                let ratio = contrast_ratio(fg, bg);
                results.push(DesignSystemPair {
                    theme: theme_name.to_string(),
                    foreground_name: key,
                    foreground_color: *fg,
                    background_name: "--background".to_string(),
                    background_color: *bg,
                    ratio,
                    level: ConformanceLevel::from_ratio(ratio),
                });
            }
        }
    }

    // 4. Muted foreground on background (common for secondary text)
    if let (Some(fg), Some(bg)) = (vars.get("--muted-foreground"), vars.get("--background")) {
        let ratio = contrast_ratio(fg, bg);
        results.push(DesignSystemPair {
            theme: theme_name.to_string(),
            foreground_name: "--muted-foreground".to_string(),
            foreground_color: *fg,
            background_name: "--background".to_string(),
            background_color: *bg,
            ratio,
            level: ConformanceLevel::from_ratio(ratio),
        });
    }

    // 5. Muted foreground on muted background
    if let (Some(fg), Some(bg)) = (vars.get("--muted-foreground"), vars.get("--muted")) {
        let ratio = contrast_ratio(fg, bg);
        results.push(DesignSystemPair {
            theme: theme_name.to_string(),
            foreground_name: "--muted-foreground".to_string(),
            foreground_color: *fg,
            background_name: "--muted".to_string(),
            background_color: *bg,
            ratio,
            level: ConformanceLevel::from_ratio(ratio),
        });
    }

    // 6. Border on background (for visibility)
    if let (Some(border), Some(bg)) = (vars.get("--border"), vars.get("--background")) {
        let ratio = contrast_ratio(border, bg);
        results.push(DesignSystemPair {
            theme: theme_name.to_string(),
            foreground_name: "--border".to_string(),
            foreground_color: *border,
            background_name: "--background".to_string(),
            background_color: *bg,
            ratio,
            level: ConformanceLevel::from_ratio(ratio),
        });
    }

    // 7. Ring on background (focus indicator visibility)
    if let (Some(ring), Some(bg)) = (vars.get("--ring"), vars.get("--background")) {
        let ratio = contrast_ratio(ring, bg);
        results.push(DesignSystemPair {
            theme: theme_name.to_string(),
            foreground_name: "--ring".to_string(),
            foreground_color: *ring,
            background_name: "--background".to_string(),
            background_color: *bg,
            ratio,
            level: ConformanceLevel::from_ratio(ratio),
        });
    }

    // 8. Sidebar pairs
    if let (Some(fg), Some(bg)) = (vars.get("--sidebar-foreground"), vars.get("--sidebar")) {
        let ratio = contrast_ratio(fg, bg);
        results.push(DesignSystemPair {
            theme: theme_name.to_string(),
            foreground_name: "--sidebar-foreground".to_string(),
            foreground_color: *fg,
            background_name: "--sidebar".to_string(),
            background_color: *bg,
            ratio,
            level: ConformanceLevel::from_ratio(ratio),
        });
    }

    // 9. Table header foreground on table header background
    if let (Some(fg), Some(bg)) = (vars.get("--table-header-fg"), vars.get("--table-header-bg")) {
        let ratio = contrast_ratio(fg, bg);
        results.push(DesignSystemPair {
            theme: theme_name.to_string(),
            foreground_name: "--table-header-fg".to_string(),
            foreground_color: *fg,
            background_name: "--table-header-bg".to_string(),
            background_color: *bg,
            ratio,
            level: ConformanceLevel::from_ratio(ratio),
        });
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

        let results = audit_design_system(&light, &dark, true, false);
        assert!(results.len() >= 2); // At least foreground/background + primary pair
    }

    #[test]
    fn test_max_contrast_passes() {
        let mut light = HashMap::new();
        light.insert("--background".to_string(), Rgba::opaque(255, 255, 255));
        light.insert("--foreground".to_string(), Rgba::opaque(0, 0, 0));

        let results = audit_design_system(&light, &HashMap::new(), true, false);
        let fg_bg = results
            .iter()
            .find(|p| p.foreground_name == "--foreground" && p.background_name == "--background");
        assert!(fg_bg.is_some());
        assert_eq!(fg_bg.unwrap().level, ConformanceLevel::Aaa);
    }
}

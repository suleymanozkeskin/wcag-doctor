//! Optional project audit configuration (`wcag-doctor.json5`).
//!
//! Declares the backdrop that translucent surfaces are composited over and any
//! foreground/background surface pairs the `--x-foreground`/`--x` naming
//! convention misses (frosted "glass" tiers, chrome, fields). Without a config
//! file the audit behaves exactly as before.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::color::parser::{parse_color, ColorParseResult};
use crate::color::Rgba;

/// Parsed `wcag-doctor.json5` contents.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct WcagConfig {
    /// Backdrop sample colors per theme (what translucent surfaces sit over).
    #[serde(default)]
    pub backdrops: Backdrops,
    /// Extra surface pairs to audit beyond the naming convention.
    #[serde(default)]
    pub surfaces: Vec<SurfaceRule>,
}

/// Backdrop sample colors per theme.
///
/// Each entry is a literal color (`#rrggbb`, `oklch(...)`, …) or a
/// `var(--token)` / bare `--token` reference resolved against that theme. The
/// audit takes the worst-case contrast across every sample, because a surface
/// floating over a varying/animated backdrop must stay legible at every point.
#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Backdrops {
    #[serde(default)]
    pub light: Vec<String>,
    #[serde(default)]
    pub dark: Vec<String>,
}

/// A translucent surface audited against a set of foreground tokens.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SurfaceRule {
    /// Background custom property, e.g. `--glass-surface`.
    pub background: String,
    /// Foreground custom properties placed on the surface, e.g. `--foreground`.
    pub foregrounds: Vec<String>,
    /// Composite the surface over the theme's backdrop samples (default `true`).
    #[serde(default = "default_true")]
    pub over_backdrop: bool,
}

fn default_true() -> bool {
    true
}

/// Config file names searched for, in priority order.
const CONFIG_NAMES: [&str; 3] = [
    "wcag-doctor.json5",
    "wcag-doctor.json",
    ".wcag-doctor.json5",
];

/// Locate a config file. An explicit path is returned verbatim (so a missing
/// file surfaces as a clear load error); otherwise `dir` and its ancestors are
/// searched for a known config name.
pub fn find_config(explicit: Option<&Path>, dir: &Path) -> Option<PathBuf> {
    if let Some(path) = explicit {
        return Some(path.to_path_buf());
    }
    for base in dir.ancestors() {
        for name in CONFIG_NAMES {
            let path = base.join(name);
            if path.exists() {
                return Some(path);
            }
        }
    }
    None
}

/// Read and parse a config file, returning an explicit error message on failure.
pub fn load_config(path: &Path) -> Result<WcagConfig, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("could not read config {}: {e}", path.display()))?;
    json5::from_str::<WcagConfig>(&content)
        .map_err(|e| format!("invalid config {}: {e}", path.display()))
}

/// Resolve a backdrop sample string against a theme's resolved variable map.
/// Accepts a literal color, a `var(--token)` reference, or a bare `--token`.
pub fn resolve_sample(sample: &str, vars: &HashMap<String, Rgba>) -> Result<Rgba, String> {
    let trimmed = sample.trim();

    if trimmed.starts_with("--") {
        return vars
            .get(trimmed)
            .copied()
            .ok_or_else(|| format!("unknown token '{trimmed}'"));
    }

    match parse_color(trimmed) {
        ColorParseResult::Resolved(rgba) => Ok(rgba),
        ColorParseResult::VarReference(name) => vars
            .get(&name)
            .copied()
            .ok_or_else(|| format!("unknown token '{name}'")),
        ColorParseResult::WrappedVarReference { var_name, .. } => vars
            .get(&var_name)
            .copied()
            .ok_or_else(|| format!("unknown token '{var_name}'")),
        ColorParseResult::Unresolvable(msg) => {
            Err(format!("unresolvable backdrop sample '{trimmed}': {msg}"))
        }
    }
}

/// Resolve every sample for one theme, collecting the resolved colors and any
/// per-sample errors (unknown tokens, unparsable literals) for diagnostics.
pub fn resolve_samples(
    samples: &[String],
    vars: &HashMap<String, Rgba>,
) -> (Vec<Rgba>, Vec<String>) {
    let mut resolved = Vec::new();
    let mut errors = Vec::new();
    for sample in samples {
        match resolve_sample(sample, vars) {
            Ok(rgba) => resolved.push(rgba),
            Err(err) => errors.push(err),
        }
    }
    (resolved, errors)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_backdrops_and_surfaces() {
        let src = r##"{
            // sampled from the mesh backdrop
            backdrops: {
                light: ["var(--mesh-1)", "#204050"],
                dark: ["var(--mesh-1)"],
            },
            surfaces: [
                { background: "--glass-surface", foregrounds: ["--foreground", "--muted-foreground"] },
                { background: "--glass-field", foregrounds: ["--foreground"], over_backdrop: false },
            ],
        }"##;
        let config: WcagConfig = json5::from_str(src).unwrap();
        assert_eq!(config.backdrops.light.len(), 2);
        assert_eq!(config.backdrops.dark.len(), 1);
        assert_eq!(config.surfaces.len(), 2);
        assert!(config.surfaces[0].over_backdrop, "default is true");
        assert!(!config.surfaces[1].over_backdrop);
    }

    #[test]
    fn empty_config_is_valid() {
        let config: WcagConfig = json5::from_str("{}").unwrap();
        assert!(config.backdrops.light.is_empty());
        assert!(config.surfaces.is_empty());
    }

    #[test]
    fn unknown_field_is_rejected() {
        let err = json5::from_str::<WcagConfig>(r#"{ "backdropz": {} }"#);
        assert!(err.is_err(), "deny_unknown_fields should reject typos");
    }

    #[test]
    fn resolves_var_ref_and_literal_and_bare_token() {
        let mut vars = HashMap::new();
        vars.insert("--mesh-1".to_string(), Rgba::opaque(10, 20, 30));

        assert_eq!(
            resolve_sample("var(--mesh-1)", &vars).unwrap(),
            Rgba::opaque(10, 20, 30)
        );
        assert_eq!(
            resolve_sample("--mesh-1", &vars).unwrap(),
            Rgba::opaque(10, 20, 30)
        );
        assert_eq!(
            resolve_sample("#0a141e", &vars).unwrap(),
            Rgba::opaque(10, 20, 30)
        );
    }

    #[test]
    fn unknown_token_is_an_error() {
        let vars = HashMap::new();
        assert!(resolve_sample("var(--nope)", &vars).is_err());
        assert!(resolve_sample("--nope", &vars).is_err());
    }
}

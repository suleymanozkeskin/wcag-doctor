use std::path::{Path, PathBuf, MAIN_SEPARATOR};

use serde_json::Value;

/// Check if a path should be excluded from scanning (node_modules, .next, dist, build).
/// Uses both forward-slash and platform-native separator for cross-platform support.
pub fn is_excluded_path(path_str: &str) -> bool {
    let sep = MAIN_SEPARATOR;
    path_str.contains("node_modules")
        || path_str.contains(".next")
        || path_str.contains(&format!("{sep}dist{sep}"))
        || path_str.contains(&format!("{sep}build{sep}"))
        || path_str.contains("/dist/")
        || path_str.contains("/build/")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TailwindVersion {
    V3,
    V4,
}

/// Auto-detected project configuration.
#[derive(Debug)]
pub struct ProjectConfig {
    pub css_file: Option<PathBuf>,
    pub tailwind_config: Option<PathBuf>,
    pub tailwind_version: Option<TailwindVersion>,
    pub source_dirs: Vec<PathBuf>,
}

/// Auto-detect project structure starting from the given directory.
pub fn detect_project(dir: &Path) -> ProjectConfig {
    let css_file = find_css_file(dir);
    let tailwind_config = find_tailwind_config(dir);
    let tailwind_version = detect_tailwind_version(dir);
    let source_dirs = find_source_dirs(dir);

    ProjectConfig {
        css_file,
        tailwind_config,
        tailwind_version,
        source_dirs,
    }
}

fn find_css_file(dir: &Path) -> Option<PathBuf> {
    // Common locations for global CSS with custom properties
    let candidates = [
        "src/styles/globals.css",
        "src/app/globals.css",
        "app/globals.css",
        "styles/globals.css",
        "src/global.css",
        "src/index.css",
        "globals.css",
        "global.css",
    ];

    for candidate in &candidates {
        let path = dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    // Fallback: search for any CSS file containing custom properties
    let walker = globwalk::GlobWalkerBuilder::from_patterns(dir, &["**/*.css"])
        .max_depth(5)
        .build()
        .ok()?;

    for entry in walker.filter_map(|e| e.ok()) {
        let path = entry.path().to_path_buf();
        let path_str = path.to_string_lossy();
        if is_excluded_path(&path_str) {
            continue;
        }
        // Skip known vendor/library CSS files that commonly contain :root
        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with("normalize")
                || name.starts_with("reset")
                || name.starts_with("preflight")
                || name.contains(".min.")
            {
                continue;
            }
        }
        if let Ok(content) = std::fs::read_to_string(&path) {
            if content.contains(":root") && content.contains("--") {
                return Some(path);
            }
        }
    }

    None
}

fn find_tailwind_config(dir: &Path) -> Option<PathBuf> {
    let candidates = [
        "tailwind.config.ts",
        "tailwind.config.js",
        "tailwind.config.mjs",
        "tailwind.config.cjs",
    ];

    for candidate in &candidates {
        let path = dir.join(candidate);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

fn find_source_dirs(dir: &Path) -> Vec<PathBuf> {
    let candidates = ["src", "app", "pages", "components"];

    let dirs: Vec<PathBuf> = candidates
        .iter()
        .map(|c| dir.join(c))
        .filter(|p| p.exists() && p.is_dir())
        .collect();

    if dirs.is_empty() {
        // Default to the project directory itself
        vec![dir.to_path_buf()]
    } else {
        dirs
    }
}

fn detect_tailwind_version(dir: &Path) -> Option<TailwindVersion> {
    let package_json = find_in_ancestors(dir, "package.json")?;
    let content = std::fs::read_to_string(package_json).ok()?;
    let package: Value = serde_json::from_str(&content).ok()?;

    let version = package
        .get("dependencies")
        .and_then(|deps| deps.get("tailwindcss"))
        .or_else(|| {
            package
                .get("devDependencies")
                .and_then(|deps| deps.get("tailwindcss"))
        })
        .and_then(Value::as_str)?;

    match extract_major_version(version) {
        Some(3) => Some(TailwindVersion::V3),
        Some(4) => Some(TailwindVersion::V4),
        _ => None,
    }
}

fn find_in_ancestors(dir: &Path, filename: &str) -> Option<PathBuf> {
    for candidate in dir.ancestors() {
        let path = candidate.join(filename);
        if path.exists() {
            return Some(path);
        }
    }

    None
}

fn extract_major_version(version: &str) -> Option<u8> {
    let start = version.find(|c: char| c.is_ascii_digit())?;
    let digits: String = version[start..]
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::{TailwindVersion, detect_project, extract_major_version};

    #[test]
    fn extract_major_version_handles_common_ranges() {
        assert_eq!(extract_major_version("^4.0.0"), Some(4));
        assert_eq!(extract_major_version("~3.4.17"), Some(3));
        assert_eq!(extract_major_version(">=4"), Some(4));
        assert_eq!(extract_major_version("workspace:*"), None);
    }

    #[test]
    fn detect_project_reads_tailwind_version_from_ancestor_package_json() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("src/components");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            r#"{ "devDependencies": { "tailwindcss": "^4.1.0" } }"#,
        )
        .unwrap();

        let project = detect_project(&nested);
        assert_eq!(project.tailwind_version, Some(TailwindVersion::V4));
    }
}

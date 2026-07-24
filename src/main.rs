use std::path::PathBuf;
use std::process;

use clap::Parser;
use rayon::prelude::*;

use wcag_doctor::audit::design_system::audit_design_system;
use wcag_doctor::config::{TailwindVersion, detect_project, is_excluded_path};
use wcag_doctor::contrast::levels::{ConformanceLevel, MinimumLevel};
use wcag_doctor::contrast::wcag::contrast_over_backdrops;
use wcag_doctor::report::json::build_json_report;
use wcag_doctor::report::terminal::{print_component_report, print_design_system_report};
use wcag_doctor::resolver::css_vars::{
    extract_css_vars_with_diagnostics, find_missing_dark_overrides,
    load_css_vars_from_file_with_diagnostics, resolve_var_colors,
};
use wcag_doctor::resolver::tailwind::{TailwindColorConfig, parse_tailwind_config};
use wcag_doctor::scanner::component::{
    BACKDROP_BACKGROUND, ColorPair, Theme, dedup_color_pairs, scan_component,
};
use wcag_doctor::scanner::graph::build_component_graph;
use wcag_doctor::scanner::propagation::propagate_and_check;
use wcag_doctor::wcag_config;

#[derive(Parser, Debug)]
#[command(
    name = "wcag-doctor",
    version,
    about = "WCAG 2.1 color contrast compliance checker for frontend projects"
)]
struct Cli {
    /// Audit design system color pairs from CSS custom properties.
    #[arg(long)]
    system: bool,

    /// Scan a specific component file for contrast issues.
    #[arg(long, value_name = "PATH", conflicts_with = "dir")]
    file: Option<PathBuf>,

    /// Scan all components in a directory recursively.
    #[arg(long, value_name = "PATH", conflicts_with = "file")]
    dir: Option<PathBuf>,

    /// Path to CSS file with custom properties (auto-detected if omitted).
    #[arg(long, value_name = "PATH")]
    css: Option<PathBuf>,

    /// Path to Tailwind config file (auto-detected if omitted).
    #[arg(long = "tailwind-config", value_name = "PATH")]
    tailwind_config: Option<PathBuf>,

    /// Path to a wcag-doctor.json5 audit config (auto-detected if omitted).
    #[arg(long, value_name = "PATH")]
    config: Option<PathBuf>,

    /// Which theme to check.
    #[arg(long, default_value = "both", value_parser = parse_theme)]
    theme: ThemeChoice,

    /// Minimum passing conformance level.
    #[arg(long, default_value = "aa")]
    level: MinimumLevel,

    /// Output results as JSON.
    #[arg(long)]
    json: bool,

    /// Show verbose diagnostic information.
    #[arg(long)]
    verbose: bool,
}

#[derive(Debug, Clone, Copy)]
enum ThemeChoice {
    Light,
    Dark,
    Both,
}

fn parse_theme(s: &str) -> Result<ThemeChoice, String> {
    match s.to_lowercase().as_str() {
        "light" => Ok(ThemeChoice::Light),
        "dark" => Ok(ThemeChoice::Dark),
        "both" => Ok(ThemeChoice::Both),
        other => Err(format!("unknown theme '{other}', expected 'light', 'dark', or 'both'")),
    }
}

fn main() {
    let cli = Cli::parse();

    // Determine the working directory
    let work_dir = if let Some(ref dir) = cli.dir {
        dir.clone()
    } else if let Some(ref file) = cli.file {
        file.parent()
            .map(|p| p.to_path_buf())
            .unwrap_or_else(|| PathBuf::from("."))
    } else {
        PathBuf::from(".")
    };

    // Auto-detect project config
    let project = detect_project(&work_dir);

    if cli.verbose {
        eprintln!("[wcag-doctor] Project detected:");
        eprintln!("  CSS file: {:?}", project.css_file);
        eprintln!("  Tailwind config: {:?}", project.tailwind_config);
        eprintln!("  Tailwind version: {:?}", project.tailwind_version);
        eprintln!("  Source dirs: {:?}", project.source_dirs);
    }

    // Load CSS variables
    let css_path = cli.css.as_ref().or(project.css_file.as_ref());
    let var_map = match css_path {
        Some(path) => match load_css_vars_from_file_with_diagnostics(path) {
            Ok((map, diagnostics)) => {
                if cli.verbose {
                    eprintln!(
                        "  Loaded {} light vars, {} dark vars from {}",
                        map.light.len(),
                        map.dark.len(),
                        path.display()
                    );
                    for diagnostic in diagnostics {
                        eprintln!("  Warning: CSS parse issue: {diagnostic}");
                    }
                }
                map
            }
            Err(e) => {
                eprintln!("Error reading CSS file {}: {e}", path.display());
                process::exit(1);
            }
        },
        None => {
            if cli.system {
                eprintln!("No CSS file found. Use --css to specify the path to your globals.css.");
                process::exit(1);
            }
            let (map, diagnostics) = extract_css_vars_with_diagnostics("");
            if cli.verbose {
                for diagnostic in diagnostics {
                    eprintln!("  Warning: CSS parse issue: {diagnostic}");
                }
            }
            map
        }
    };

    let resolved_vars = resolve_var_colors(&var_map);

    // Load Tailwind config
    let tw_config = match cli
        .tailwind_config
        .as_ref()
        .or(project.tailwind_config.as_ref())
    {
        Some(path) => match std::fs::read_to_string(path) {
            Ok(content) => parse_tailwind_config(&content),
            Err(e) => {
                if cli.verbose {
                    eprintln!("  Warning: could not read Tailwind config: {e}");
                }
                TailwindColorConfig::default()
            }
        },
        None => {
            if cli.verbose {
                eprintln!("  No Tailwind config found, using default palette only.");
            }
            TailwindColorConfig::default()
        }
    };

    if cli.verbose && matches!(project.tailwind_version, Some(TailwindVersion::V4)) {
        eprintln!(
            "  Warning: Tailwind v4 detected; the built-in fallback palette is based on Tailwind v3 defaults and may be incomplete."
        );
    }

    let check_light = matches!(cli.theme, ThemeChoice::Light | ThemeChoice::Both);
    let check_dark = matches!(cli.theme, ThemeChoice::Dark | ThemeChoice::Both);

    // Compute missing dark override warnings
    let mut warnings: Vec<String> = Vec::new();
    if check_dark {
        let missing = find_missing_dark_overrides(&var_map);
        if !missing.is_empty() {
            if cli.verbose {
                eprintln!(
                    "  {} CSS variables have no dark theme override (light value used in both themes):",
                    missing.len()
                );
                for var in &missing {
                    eprintln!("    - {var}");
                }
            }
            for var in &missing {
                warnings.push(format!(
                    "CSS variable {var} has no dark theme override. \
                     Light mode value will be used in dark mode, which may cause contrast issues."
                ));
            }
        }
    }

    // Load optional audit config (backdrop samples + explicit surfaces).
    let config_path = wcag_config::find_config(cli.config.as_deref(), &work_dir);
    let wcag_cfg = match config_path {
        Some(ref path) => match wcag_config::load_config(path) {
            Ok(cfg) => {
                if cli.verbose {
                    eprintln!(
                        "  Loaded audit config from {} ({} surface rule(s))",
                        path.display(),
                        cfg.surfaces.len()
                    );
                }
                cfg
            }
            Err(err) => {
                eprintln!("Error: {err}");
                process::exit(1);
            }
        },
        None => wcag_config::WcagConfig::default(),
    };

    // Resolve backdrop samples per theme; surface any unresolvable samples.
    let (light_backdrops, light_backdrop_errors) =
        wcag_config::resolve_samples(&wcag_cfg.backdrops.light, &resolved_vars.light);
    let (dark_backdrops, dark_backdrop_errors) =
        wcag_config::resolve_samples(&wcag_cfg.backdrops.dark, &resolved_vars.dark);
    for err in light_backdrop_errors
        .iter()
        .chain(dark_backdrop_errors.iter())
    {
        warnings.push(format!("backdrop sample could not be resolved: {err}"));
        if cli.verbose {
            eprintln!("  Warning: backdrop sample: {err}");
        }
    }
    if cli.verbose && (!light_backdrops.is_empty() || !dark_backdrops.is_empty()) {
        eprintln!(
            "  Backdrop samples: {} light, {} dark",
            light_backdrops.len(),
            dark_backdrops.len()
        );
    }

    // Run the requested modes
    let mut design_pairs = Vec::new();
    let mut component_pairs = Vec::new();

    // Design system audit
    if cli.system {
        design_pairs = audit_design_system(
            &resolved_vars.light,
            &resolved_vars.dark,
            check_light,
            check_dark,
            &light_backdrops,
            &dark_backdrops,
            &wcag_cfg.surfaces,
        );
    }

    // Component scanning
    if let Some(ref file) = cli.file {
        let themes = get_themes(check_light, check_dark);
        for theme in &themes {
            let mut pairs = scan_component(file, &tw_config, &resolved_vars, *theme);
            component_pairs.append(&mut pairs);
        }
    }

    if cli.dir.is_some() {
        let scan_dirs: Vec<&PathBuf> = if let Some(ref dir) = cli.dir {
            vec![dir]
        } else {
            project.source_dirs.iter().collect()
        };

        // Single-file scans
        let themes = get_themes(check_light, check_dark);
        let files: Vec<PathBuf> = scan_dirs
            .iter()
            .flat_map(|dir| discover_component_files(dir))
            .collect();

        if cli.verbose {
            eprintln!("  Scanning {} component files...", files.len());
        }

        let file_pairs: Vec<ColorPair> = files
            .par_iter()
            .flat_map(|file| {
                themes
                    .iter()
                    .flat_map(|theme| scan_component(file, &tw_config, &resolved_vars, *theme))
                    .collect::<Vec<_>>()
            })
            .collect();
        component_pairs.extend(file_pairs);

        // Cross-file graph analysis
        if cli.verbose {
            eprintln!("  Building component graph...");
        }
        for scan_dir in &scan_dirs {
            let graph = build_component_graph(scan_dir);
            if cli.verbose {
                eprintln!("  Graph contains {} files.", graph.len());
            }

            for theme in &themes {
                let mut propagated =
                    propagate_and_check(&graph, &tw_config, &resolved_vars, *theme);
                component_pairs.append(&mut propagated);
            }
        }

        component_pairs = dedup_color_pairs(component_pairs);
    }

    // Text with no surface in scope is reported against `<backdrop>`. That is only
    // meaningful when the project declared what the backdrop is; otherwise the
    // contrast is unknowable, so drop those rather than report a guess.
    if light_backdrops.is_empty() && dark_backdrops.is_empty() {
        component_pairs.retain(|p| p.background_name != BACKDROP_BACKGROUND);
    }

    // A component pair with a translucent background is scanned with a black/white
    // worst-case (the scanner has no backdrop context). Recompute those over the
    // theme's configured backdrop so component findings match the --system audit.
    // This also resolves `<backdrop>` pairs: a transparent background composited
    // over a backdrop sample is that sample, i.e. text directly on the wallpaper.
    if !light_backdrops.is_empty() || !dark_backdrops.is_empty() {
        for pair in &mut component_pairs {
            if pair.background_color.a >= 1.0 {
                continue;
            }
            let backdrops = if pair.theme == "dark" {
                &dark_backdrops
            } else {
                &light_backdrops
            };
            if backdrops.is_empty() {
                continue;
            }
            pair.ratio =
                contrast_over_backdrops(&pair.foreground_color, &pair.background_color, backdrops);
            pair.level = ConformanceLevel::from_ratio(pair.ratio);
        }
    }

    // Output results
    if cli.json {
        let json = build_json_report(&design_pairs, &component_pairs, cli.level, &warnings);
        println!("{json}");
    } else {
        if !design_pairs.is_empty() {
            print_design_system_report(&design_pairs, cli.level);
        }
        if !component_pairs.is_empty() {
            print_component_report(&component_pairs, cli.level);
        }
        if design_pairs.is_empty() && component_pairs.is_empty() {
            eprintln!("Nothing to check. Use --system, --file, or --dir to specify what to scan.");
            process::exit(1);
        }
        if !warnings.is_empty() {
            eprintln!(
                "  Note: {} CSS variable(s) have no dark theme override (use --verbose to list them).",
                warnings.len()
            );
        }
    }

    // Exit with non-zero code if there are failures
    let has_failures = design_pairs
        .iter()
        .any(|p| !p.level.meets_minimum(cli.level))
        || component_pairs
            .iter()
            .any(|p| !p.level.meets_minimum(cli.level));

    if has_failures {
        process::exit(1);
    }
}

fn get_themes(check_light: bool, check_dark: bool) -> Vec<Theme> {
    let mut themes = Vec::new();
    if check_light {
        themes.push(Theme::Light);
    }
    if check_dark {
        themes.push(Theme::Dark);
    }
    themes
}

fn discover_component_files(dir: &PathBuf) -> Vec<PathBuf> {
    let walker = match globwalk::GlobWalkerBuilder::from_patterns(
        dir,
        &["**/*.tsx", "**/*.jsx", "**/*.ts", "**/*.js"],
    )
    .max_depth(20)
    .build()
    {
        Ok(w) => w,
        Err(_) => return Vec::new(),
    };

    walker
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_path_buf())
        .filter(|p| !is_excluded_path(&p.to_string_lossy()))
        .collect()
}

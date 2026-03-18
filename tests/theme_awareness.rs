//! Integration tests for theme-aware scanning (improvements 1–4).
//!
//! Tests cover:
//! 1. Variant-aware class extraction (dark: excluded in light, light: excluded in dark)
//! 2. Dark override logic (dark:bg-X replaces bg-Y in dark mode)
//! 3. Automatic dual-theme mode (--theme both checks both)
//! 4. Dark theme inherits light vars (CSS cascading)
//! 5. Missing dark override detection

use std::fs;
use std::path::Path;
use std::process::Command;

use serde_json::Value;
use tempfile::TempDir;

fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
}

fn run_wcag_doctor(args: &[String]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_wcag-doctor"))
        .args(args)
        .output()
        .unwrap()
}

// ============================================================================
// 1. Variant-aware class extraction
// ============================================================================

#[test]
fn dark_classes_excluded_in_light_mode_scan() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #000000;
}
"#,
    );

    // Component with dark: variant classes that should be excluded in light mode
    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return (
        <div className="bg-white dark:bg-slate-900 text-black dark:text-white">
            Hello
        </div>
    );
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let components = report["components"].as_array().unwrap();

    // Should only find one pair: text-black on bg-white (light mode)
    assert_eq!(components.len(), 1, "light mode should have exactly 1 pair");
    assert_eq!(components[0]["foreground"]["name"], "text-black");
    assert_eq!(components[0]["background"]["name"], "bg-white");
    assert_eq!(components[0]["theme"], "light");
}

#[test]
fn dark_override_replaces_base_classes_in_dark_mode_scan() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #000000;
}
"#,
    );

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return (
        <div className="bg-white dark:bg-slate-900 text-black dark:text-white">
            Hello
        </div>
    );
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let components = report["components"].as_array().unwrap();

    // In dark mode: dark:text-white on dark:bg-slate-900 (overrides bg-white & text-black)
    assert_eq!(components.len(), 1, "dark mode should have exactly 1 pair");
    assert_eq!(components[0]["foreground"]["name"], "text-white");
    assert_eq!(components[0]["background"]["name"], "bg-slate-900");
    assert_eq!(components[0]["theme"], "dark");
}

#[test]
fn partial_dark_override_uses_base_for_missing() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
}
"#,
    );

    // Only foreground has dark: override, background does not
    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return (
        <div className="bg-white text-gray-800 dark:text-gray-200">
            Hello
        </div>
    );
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let components = report["components"].as_array().unwrap();

    assert_eq!(components.len(), 1);
    // dark:text-gray-200 overrides text-gray-800, but bg-white has no dark: override
    assert_eq!(components[0]["foreground"]["name"], "text-gray-200");
    assert_eq!(components[0]["background"]["name"], "bg-white");
}

// ============================================================================
// 2. Automatic dual-theme mode (--theme both)
// ============================================================================

#[test]
fn both_theme_produces_pairs_for_each_theme() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
}
"#,
    );

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return (
        <div className="bg-white dark:bg-slate-900 text-black dark:text-white">
            Hello
        </div>
    );
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "both".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let components = report["components"].as_array().unwrap();

    // Should have 2 pairs: one light, one dark
    assert_eq!(components.len(), 2, "both mode should produce 2 pairs");

    let light_pair = components.iter().find(|c| c["theme"] == "light").unwrap();
    let dark_pair = components.iter().find(|c| c["theme"] == "dark").unwrap();

    assert_eq!(light_pair["foreground"]["name"], "text-black");
    assert_eq!(light_pair["background"]["name"], "bg-white");

    assert_eq!(dark_pair["foreground"]["name"], "text-white");
    assert_eq!(dark_pair["background"]["name"], "bg-slate-900");
}

#[test]
fn default_theme_is_both() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
}
"#,
    );

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <div className="bg-white dark:bg-black text-black dark:text-white">Hello</div>;
}
"#,
    );

    // No --theme flag = default "both"
    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let components = report["components"].as_array().unwrap();

    let themes: Vec<&str> = components.iter()
        .map(|c| c["theme"].as_str().unwrap())
        .collect();
    assert!(themes.contains(&"light"), "default should include light");
    assert!(themes.contains(&"dark"), "default should include dark");
}

// ============================================================================
// 3. Dark theme inherits light CSS vars (CSS cascading)
// ============================================================================

#[test]
fn dark_theme_inherits_light_vars_for_system_audit() {
    let dir = TempDir::new().unwrap();

    // Only :root vars, no .dark overrides
    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #000000;
    --primary: #1d4ed8;
    --primary-foreground: #ffffff;
}
"#,
    );

    let args = vec![
        "--system".to_string(),
        "--dir".to_string(),
        dir.path().display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let design_system = report["design_system"].as_array().unwrap();

    // Dark theme should inherit light vars and produce results
    assert!(
        !design_system.is_empty(),
        "dark theme should inherit light vars and produce system audit results"
    );

    // All results should be dark theme
    for item in design_system {
        assert_eq!(item["theme"], "dark");
    }
}

#[test]
fn dark_theme_overrides_take_precedence_over_inherited() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #000000;
}
.dark {
    --background: #0a0a0a;
    --foreground: #fafafa;
}
"#,
    );

    let args = vec![
        "--system".to_string(),
        "--dir".to_string(),
        dir.path().display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let design_system = report["design_system"].as_array().unwrap();

    let fg_bg = design_system.iter().find(|p| {
        p["foreground"]["name"] == "--foreground" && p["background"]["name"] == "--background"
    });
    assert!(fg_bg.is_some(), "should have foreground/background pair in dark mode");

    // Verify colors are dark theme values, not light
    let pair = fg_bg.unwrap();
    assert_eq!(pair["foreground"]["hex"], "#fafafa", "should use dark --foreground");
    assert_eq!(pair["background"]["hex"], "#0a0a0a", "should use dark --background");
}

#[test]
fn css_var_colors_resolve_correctly_per_theme_in_component_scan() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --primary: 220 80% 50%;
    --primary-foreground: 0 0% 100%;
}
.dark {
    --primary: 220 70% 30%;
    --primary-foreground: 0 0% 95%;
}
"#,
    );

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <div className="bg-primary text-primary-foreground">Hello</div>;
}
"#,
    );

    // Light mode
    let light_args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
    ];

    let light_output = run_wcag_doctor(&light_args);
    let light_report: Value = serde_json::from_slice(&light_output.stdout).unwrap();
    let light_components = light_report["components"].as_array().unwrap();

    // Dark mode
    let dark_args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let dark_output = run_wcag_doctor(&dark_args);
    let dark_report: Value = serde_json::from_slice(&dark_output.stdout).unwrap();
    let dark_components = dark_report["components"].as_array().unwrap();

    assert_eq!(light_components.len(), 1);
    assert_eq!(dark_components.len(), 1);

    // Colors should be different between themes since CSS vars are different
    let light_bg_hex = light_components[0]["background"]["hex"].as_str().unwrap();
    let dark_bg_hex = dark_components[0]["background"]["hex"].as_str().unwrap();
    assert_ne!(
        light_bg_hex, dark_bg_hex,
        "light and dark bg-primary should resolve to different colors"
    );
}

// ============================================================================
// 4. Missing dark override detection
// ============================================================================

#[test]
fn missing_dark_overrides_reported_in_warnings() {
    let dir = TempDir::new().unwrap();

    // Only :root vars, no .dark overrides
    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #000000;
    --custom-accent: #ff6600;
}
"#,
    );

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <div className="bg-white text-black">Hello</div>;
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let warnings = report["warnings"].as_array().unwrap();

    // Should warn about all three vars having no dark override
    assert!(
        warnings.len() >= 3,
        "should have at least 3 warnings for missing dark overrides, got: {}",
        warnings.len()
    );

    let warning_text: Vec<&str> = warnings.iter().map(|w| w.as_str().unwrap()).collect();
    assert!(
        warning_text.iter().any(|w| w.contains("--background")),
        "should warn about --background, got: {warning_text:?}"
    );
    assert!(
        warning_text.iter().any(|w| w.contains("--custom-accent")),
        "should warn about --custom-accent, got: {warning_text:?}"
    );
}

#[test]
fn no_warnings_when_dark_overrides_exist() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #000000;
}
.dark {
    --background: #0a0a0a;
    --foreground: #fafafa;
}
"#,
    );

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <div className="bg-white text-black">Hello</div>;
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();

    // warnings should be absent (skip_serializing_if = empty)
    assert!(
        report.get("warnings").is_none(),
        "should have no warnings when all vars have dark overrides"
    );
}

#[test]
fn no_warnings_in_light_only_mode() {
    let dir = TempDir::new().unwrap();

    // No dark overrides, but we're only checking light mode
    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #000000;
}
"#,
    );

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <div className="bg-white text-black">Hello</div>;
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();

    // No warnings in light-only mode (missing dark overrides are irrelevant)
    assert!(
        report.get("warnings").is_none(),
        "should have no warnings in light-only mode"
    );
}

// ============================================================================
// 5. Real-world component patterns
// ============================================================================

#[test]
fn shadcn_style_component_with_theme_variants() {
    // Simulates the exact pattern that triggered the original issue:
    // bg-aiPurple-50 (light) / dark:bg-aiPurple-800 where the var has no dark override
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --ai-purple-50: 274 100% 95%;
    --ai-purple-600: 274 62% 50%;
    --ai-purple-800: 274 62% 41%;
}
"#,
    );

    write_file(
        dir.path(),
        "tailwind.config.ts",
        r#"
module.exports = {
    theme: {
        extend: {
            colors: {
                aiPurple: {
                    '50': 'hsl(var(--ai-purple-50))',
                    '600': 'hsl(var(--ai-purple-600))',
                    '800': 'hsl(var(--ai-purple-800))',
                },
            },
        },
    },
}
"#,
    );

    write_file(
        dir.path(),
        "profile-card.tsx",
        r#"
export function ProfileCard() {
    return (
        <div className="bg-aiPurple-50 dark:bg-aiPurple-800">
            <p className="text-aiPurple-600">Qualifikation</p>
        </div>
    );
}
"#,
    );

    // Check dark mode - this is where the issue would manifest
    let args = vec![
        "--file".to_string(),
        dir.path().join("profile-card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--tailwind-config".to_string(),
        dir.path().join("tailwind.config.ts").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let components = report["components"].as_array().unwrap();

    assert_eq!(components.len(), 1, "should find exactly 1 pair in dark mode");

    // In dark mode: text-aiPurple-600 on bg-aiPurple-800 (dark: override for bg)
    assert_eq!(components[0]["foreground"]["name"], "text-aiPurple-600");
    assert_eq!(components[0]["background"]["name"], "bg-aiPurple-800");
    assert_eq!(components[0]["theme"], "dark");
}

#[test]
fn nested_dark_variant_with_responsive_prefix() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
}
"#,
    );

    // sm:dark:bg-slate-900 should still be recognized as dark-only
    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return (
        <div className="bg-white sm:dark:bg-slate-900 text-black md:dark:text-white">
            Hello
        </div>
    );
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let components = report["components"].as_array().unwrap();

    assert_eq!(components.len(), 1);
    // sm:dark:bg-slate-900 overrides bg-white, md:dark:text-white overrides text-black
    assert_eq!(components[0]["foreground"]["name"], "text-white");
    assert_eq!(components[0]["background"]["name"], "bg-slate-900");
}

#[test]
fn hover_variant_not_treated_as_theme() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
}
"#,
    );

    // hover: is not a theme variant, should be treated as Base
    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <div className="bg-white hover:bg-blue-500 text-black">Hello</div>;
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let components = report["components"].as_array().unwrap();

    // hover:bg-blue-500 is Base context (not dark/light), so both bg-white and
    // bg-blue-500 are included. text-black pairs with both.
    assert_eq!(components.len(), 2, "hover: variant should produce 2 pairs (base bg + hover bg)");
}

#[test]
fn child_inherits_dark_background_override() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "globals.css",
        r#"
:root {
    --background: #ffffff;
}
"#,
    );

    // Parent has dark: bg override, child should inherit it in dark mode
    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return (
        <div className="bg-white dark:bg-slate-900">
            <p className="text-red-500">Danger</p>
        </div>
    );
}
"#,
    );

    let dark_args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "dark".to_string(),
    ];

    let dark_output = run_wcag_doctor(&dark_args);
    let dark_report: Value = serde_json::from_slice(&dark_output.stdout).unwrap();
    let dark_components = dark_report["components"].as_array().unwrap();

    assert_eq!(dark_components.len(), 1);
    // Child inherits dark:bg-slate-900 from parent in dark mode
    assert_eq!(dark_components[0]["foreground"]["name"], "text-red-500");
    assert_eq!(dark_components[0]["background"]["name"], "bg-slate-900");

    // Compare with light mode where child inherits bg-white
    let light_args = vec![
        "--file".to_string(),
        dir.path().join("card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
    ];

    let light_output = run_wcag_doctor(&light_args);
    let light_report: Value = serde_json::from_slice(&light_output.stdout).unwrap();
    let light_components = light_report["components"].as_array().unwrap();

    assert_eq!(light_components.len(), 1);
    assert_eq!(light_components[0]["foreground"]["name"], "text-red-500");
    assert_eq!(light_components[0]["background"]["name"], "bg-white");
}

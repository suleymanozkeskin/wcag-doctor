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

#[test]
fn system_mode_supports_explicit_css_path() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "styles/tokens.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #111111;
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
        dir.path().join("styles/tokens.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    assert!(
        output.status.success(),
        "expected success, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let design_system = report["design_system"].as_array().unwrap();
    assert!(
        !design_system.is_empty(),
        "explicit css path should produce design-system results"
    );

    let has_primary_pair = design_system.iter().any(|item| {
        item["foreground"]["name"] == "--primary-foreground"
            && item["background"]["name"] == "--primary"
    });
    assert!(has_primary_pair, "expected primary pair in explicit css report");
}

#[test]
fn explicit_css_overrides_autodetected_css() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "app/globals.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #ffffff;
}
"#,
    );

    write_file(
        dir.path(),
        "fixtures/explicit.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #111111;
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
        dir.path().join("fixtures/explicit.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    assert!(
        output.status.success(),
        "expected success with explicit css override, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let summary = &report["summary"];
    assert_eq!(
        summary["fail"].as_u64().unwrap(),
        0,
        "explicit css should override the autodetected failing globals.css"
    );
}

#[test]
fn aa_large_level_is_accepted_by_cli() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "styles/tokens.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #111111;
    --primary: #3b82f6;
    --primary-foreground: #ffffff;
}
"#,
    );

    let args = vec![
        "--system".to_string(),
        "--dir".to_string(),
        dir.path().display().to_string(),
        "--css".to_string(),
        dir.path().join("styles/tokens.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
        "--level".to_string(),
        "aa-large".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    assert!(
        output.status.success(),
        "expected aa-large run to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(report["minimum_level"], "AA Large");
    assert!(
        report["design_system"]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["level"] == "AA Large" && item["passes"] == true),
        "expected aa-large pair to be accepted by the CLI minimum level"
    );
}

#[test]
fn file_and_dir_flags_conflict() {
    let dir = TempDir::new().unwrap();

    let args = vec![
        "--file".to_string(),
        dir.path().join("component.tsx").display().to_string(),
        "--dir".to_string(),
        dir.path().display().to_string(),
    ];

    let output = run_wcag_doctor(&args);
    assert!(!output.status.success(), "expected clap conflict failure");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflicts with"),
        "expected conflict error in stderr, got: {stderr}"
    );
}

#[test]
fn verbose_mode_reports_css_parse_diagnostics() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "styles/broken.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #111111;
}

:root {
    broken;
}
"#,
    );

    let args = vec![
        "--system".to_string(),
        "--dir".to_string(),
        dir.path().display().to_string(),
        "--css".to_string(),
        dir.path().join("styles/broken.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
        "--verbose".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    assert!(
        output.status.success(),
        "expected verbose malformed CSS run to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Warning: CSS parse issue:"),
        "expected CSS parse diagnostic in verbose stderr, got: {stderr}"
    );
}

#[test]
fn verbose_mode_warns_for_tailwind_v4_palette_fallbacks() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "package.json",
        r#"
{
  "devDependencies": {
    "tailwindcss": "^4.0.0"
  }
}
"#,
    );

    write_file(
        dir.path(),
        "styles/globals.css",
        r#"
:root {
    --background: #ffffff;
    --foreground: #111111;
}
"#,
    );

    write_file(
        dir.path(),
        "src/Card.tsx",
        r#"
export function Card() {
    return <div className="bg-white text-black">Hello</div>;
}
"#,
    );

    let args = vec![
        "--file".to_string(),
        dir.path().join("src/Card.tsx").display().to_string(),
        "--css".to_string(),
        dir.path().join("styles/globals.css").display().to_string(),
        "--json".to_string(),
        "--theme".to_string(),
        "light".to_string(),
        "--verbose".to_string(),
    ];

    let output = run_wcag_doctor(&args);
    assert!(
        output.status.success(),
        "expected verbose Tailwind v4 run to succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Warning: Tailwind v4 detected; the built-in fallback palette is based on Tailwind v3 defaults"),
        "expected Tailwind v4 fallback warning in stderr, got: {stderr}"
    );
}

//! Integration tests for cross-file propagation.
//!
//! Each test creates a synthetic mini-project on disk (temp dir), builds
//! the component graph, runs propagation, and asserts on the resulting
//! ColorPair findings. This exercises the real pipeline end-to-end:
//! file discovery → AST parse → graph build → propagation → findings.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use tempfile::TempDir;

use wcag_doctor::color::Rgba;
use wcag_doctor::resolver::css_vars::{ResolvedVarColors, extract_css_vars, resolve_var_colors};
use wcag_doctor::resolver::tailwind::TailwindColorConfig;
use wcag_doctor::scanner::component::Theme;
use wcag_doctor::scanner::graph::build_component_graph;
use wcag_doctor::scanner::propagation::propagate_and_check;

/// Helper: write a file into the temp dir, creating subdirectories as needed.
fn write_file(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&path, content).unwrap();
}

/// Helper: build default resolved vars with white/black so bg-white resolves.
fn default_vars() -> ResolvedVarColors {
    ResolvedVarColors::default()
}

/// Helper: returns foreground class names from propagation results.
fn fg_names(pairs: &[wcag_doctor::scanner::component::ColorPair]) -> Vec<String> {
    pairs
        .iter()
        .map(|p| {
            // Strip the " (in filename.tsx)" suffix to get the bare class name
            p.foreground_name
                .split(" (in ")
                .next()
                .unwrap_or(&p.foreground_name)
                .to_string()
        })
        .collect()
}

// ============================================================================
// 1. Core propagation happy paths
// ============================================================================

#[test]
fn named_export_basic_propagation() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export function Sidebar() {
    return <p className="text-red-500">hello</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Sidebar } from "./sidebar";

export function Page() {
    return (
        <div className="bg-white">
            <Sidebar />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(!pairs.is_empty(), "should find at least one propagated pair");
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500, got: {names:?}"
    );
}

#[test]
fn default_named_export_propagation() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export default function Sidebar() {
    return <p className="text-red-500">hello</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import Sidebar from "./sidebar";

export function Page() {
    return (
        <div className="bg-white">
            <Sidebar />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(!pairs.is_empty(), "should find propagated pair for default named export");
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500, got: {names:?}"
    );
}

#[test]
fn const_arrow_export_propagation() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export const Sidebar = () => {
    return <p className="text-red-500">hello</p>;
};
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Sidebar } from "./sidebar";

export function Page() {
    return (
        <div className="bg-white">
            <Sidebar />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(!pairs.is_empty(), "should find pair for const arrow component");
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500, got: {names:?}"
    );
}

#[test]
fn memo_wrapped_export_propagation() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
import { memo } from "react";

export const Sidebar = memo(() => {
    return <span className="text-blue-500">memoized</span>;
});
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Sidebar } from "./sidebar";

export function Page() {
    return (
        <div className="bg-white">
            <Sidebar />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(!pairs.is_empty(), "should find pair through memo() wrapper");
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-blue-500".to_string()),
        "should find text-blue-500, got: {names:?}"
    );
}

// ============================================================================
// 2. Import/export identity
// ============================================================================

#[test]
fn default_import_aliased_falls_back() {
    // import Nav from "./sidebar" where file exports `export default function Sidebar()`
    // The local name "Nav" won't match the function name "Sidebar", so
    // find_component_body returns None and the full-file fallback fires.
    // The test asserts the outcome (findings exist) — it does NOT assert HOW.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export default function Sidebar() {
    return <p className="text-red-500">hello</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import Nav from "./sidebar";

export function Page() {
    return (
        <div className="bg-white">
            <Nav />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    // Findings should exist — either via scoped lookup or fallback.
    assert!(!pairs.is_empty(), "aliased default import should still produce findings (via fallback)");
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 even through aliased import, got: {names:?}"
    );
}

#[test]
fn tsconfig_paths_support_at_alias() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "tsconfig.json",
        r#"
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "@/*": ["./src/*"]
    }
  }
}
"#,
    );

    write_file(
        dir.path(),
        "src/components/sidebar.tsx",
        r#"
export function Sidebar() {
    return <p className="text-red-500">hello</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "src/page.tsx",
        r#"
import { Sidebar } from "@/components/sidebar";

export function Page() {
    return (
        <div className="bg-white">
            <Sidebar />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(!pairs.is_empty(), "@/ alias should resolve through tsconfig paths");
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 through @/ alias, got: {names:?}"
    );
}

#[test]
fn tsconfig_paths_support_tilde_alias() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "tsconfig.json",
        r#"
{
  "compilerOptions": {
    "baseUrl": ".",
    "paths": {
      "~/*": ["./*"]
    }
  }
}
"#,
    );

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export default function Sidebar() {
    return <p className="text-blue-500">hello</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import Sidebar from "~/sidebar";

export function Page() {
    return (
        <div className="bg-white">
            <Sidebar />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(!pairs.is_empty(), "~/ alias should resolve through tsconfig paths");
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-blue-500".to_string()),
        "should find text-blue-500 through ~/ alias, got: {names:?}"
    );
}

#[test]
fn multi_component_file_only_reports_used_component() {
    // File exports both A and B. Only A is used in the parent.
    // Propagation should report only A's text colors, not B's.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "components.tsx",
        r#"
export function ComponentA() {
    return <p className="text-red-500">A content</p>;
}

export function ComponentB() {
    return <p className="text-green-500">B content</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { ComponentA } from "./components";

export function Page() {
    return (
        <div className="bg-white">
            <ComponentA />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 from ComponentA, got: {names:?}"
    );
    assert!(
        !names.contains(&"text-green-500".to_string()),
        "should NOT find text-green-500 from ComponentB, got: {names:?}"
    );
}

#[test]
fn barrel_reexport_follows_chain() {
    // Barrel re-export: export { Sidebar } from "./sidebar"
    // The graph should follow the re-export chain and find the real source.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export function Sidebar() {
    return <p className="text-red-500">hello</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "index.tsx",
        r#"
export { Sidebar } from "./sidebar";
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Sidebar } from "./index";

export function Page() {
    return (
        <div className="bg-white">
            <Sidebar />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(
        !pairs.is_empty(),
        "barrel re-export should follow chain and find findings"
    );
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 through barrel re-export, got: {names:?}"
    );
}

#[test]
fn aliased_default_import_scoped_to_default_export() {
    // import Nav from "./sidebar" where sidebar.tsx has two components
    // but only one is the default export. Should only report the default
    // export's text colors, not the other component's.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export function HelperWidget() {
    return <span className="text-green-500">widget</span>;
}

export default function Sidebar() {
    return <p className="text-red-500">sidebar</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import Nav from "./sidebar";

export function Page() {
    return (
        <div className="bg-white">
            <Nav />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 from default export, got: {names:?}"
    );
    assert!(
        !names.contains(&"text-green-500".to_string()),
        "should NOT find text-green-500 from HelperWidget, got: {names:?}"
    );
}

// ============================================================================
// 3. Background discovery
// ============================================================================

#[test]
fn literal_classname_bg_discovered() {
    // Baseline: literal className="bg-white" should produce findings.
    // This is covered by tests above, but explicit for the matrix.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <span className="text-red-500">text</span>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Card } from "./card";

export function Page() {
    return <div className="bg-white"><Card /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(!pairs.is_empty(), "literal bg-white should be discovered");
}

#[test]
fn cn_call_bg_discovered_in_graph() {
    // className={cn("bg-white", "rounded-lg")} — the graph builder extracts
    // bg classes from expression containers (cn, clsx, etc.).
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <span className="text-red-500">text</span>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Card } from "./card";
import { cn } from "./utils";

export function Page() {
    return <div className={cn("bg-white", "rounded-lg")}><Card /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(
        !pairs.is_empty(),
        "cn() bg classes should be extracted by graph builder"
    );
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 with cn() bg parent, got: {names:?}"
    );
}

#[test]
fn clsx_and_ternary_bg_discovered() {
    // className={clsx("bg-white", isActive && "bg-blue-500")}
    // Both string literal bg classes should be extracted.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <span className="text-red-500">text</span>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Card } from "./card";

export function Page({ isActive }: { isActive: boolean }) {
    return (
        <div className={clsx("bg-white", isActive && "bg-blue-500")}>
            <Card />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(
        !pairs.is_empty(),
        "clsx() bg classes should be extracted"
    );
}

#[test]
fn template_literal_bg_not_discovered() {
    // className={`bg-${dynamic}`} — unresolvable, should produce no findings.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return <span className="text-red-500">text</span>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Card } from "./card";

export function Page() {
    const theme = "primary";
    return <div className={`bg-${theme}`}><Card /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(
        pairs.is_empty(),
        "template literal bg should not produce findings"
    );
}

// ============================================================================
// 4. Foreground extraction scoping
// ============================================================================

#[test]
fn multiple_text_colors_in_branches() {
    // Component has text-red-500 in if branch and text-blue-500 in else.
    // Both should be found.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "alert.tsx",
        r#"
export function Alert({ danger }: { danger: boolean }) {
    if (danger) {
        return <p className="text-red-500">danger!</p>;
    }
    return <p className="text-blue-500">info</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Alert } from "./alert";

export function Page() {
    return <div className="bg-white"><Alert danger={false} /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 from if branch, got: {names:?}"
    );
    assert!(
        names.contains(&"text-blue-500".to_string()),
        "should find text-blue-500 from else branch, got: {names:?}"
    );
}

#[test]
fn inner_helper_not_leaked_to_exported_component() {
    // HelperBadge defines text-yellow-500 but is NOT the exported component.
    // Only Sidebar's text-red-500 should appear.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
function HelperBadge() {
    return <span className="text-yellow-500">badge</span>;
}

export function Sidebar() {
    return <p className="text-red-500">sidebar content</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Sidebar } from "./sidebar";

export function Page() {
    return <div className="bg-white"><Sidebar /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 from Sidebar, got: {names:?}"
    );
    assert!(
        !names.contains(&"text-yellow-500".to_string()),
        "should NOT find text-yellow-500 from HelperBadge, got: {names:?}"
    );
}

#[test]
fn comment_tailwind_class_not_reported() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export function Sidebar() {
    // text-green-500 should not be treated as a real class
    return <p className="text-red-500">sidebar content</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Sidebar } from "./sidebar";

export function Page() {
    return <div className="bg-white"><Sidebar /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 from real className, got: {names:?}"
    );
    assert!(
        !names.contains(&"text-green-500".to_string()),
        "should not match Tailwind classes from comments, got: {names:?}"
    );
}

#[test]
fn plain_string_tailwind_class_not_reported() {
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export function Sidebar() {
    const label = "text-green-500";
    const metadata = { hint: "text-blue-500" };
    return <p className="text-red-500">{label}{metadata.hint}</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Sidebar } from "./sidebar";

export function Page() {
    return <div className="bg-white"><Sidebar /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 from real className, got: {names:?}"
    );
    assert!(
        !names.contains(&"text-green-500".to_string()),
        "should not match plain string literals, got: {names:?}"
    );
    assert!(
        !names.contains(&"text-blue-500".to_string()),
        "should not match object string literals, got: {names:?}"
    );
}

#[test]
fn fragment_return_text_colors_found() {
    // Component returns a fragment: <><p className="text-blue-500">a</p></>
    // Text colors inside the fragment should be found.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "card.tsx",
        r#"
export function Card() {
    return (
        <>
            <p className="text-blue-500">first</p>
            <p className="text-red-500">second</p>
        </>
    );
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Card } from "./card";

export function Page() {
    return <div className="bg-white"><Card /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-blue-500".to_string()),
        "should find text-blue-500 through fragment, got: {names:?}"
    );
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 through fragment, got: {names:?}"
    );
}

// ============================================================================
// 5. Fallback & non-goals
// ============================================================================

#[test]
fn anonymous_default_export_uses_fallback() {
    // export default () => <p className="text-red-500">x</p>
    // No named function → find_component_body returns None → full-file fallback.
    // Should still produce findings.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export default () => {
    return <p className="text-red-500">anonymous</p>;
};
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import Sidebar from "./sidebar";

export function Page() {
    return <div className="bg-white"><Sidebar /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    // Fallback should still produce findings
    assert!(
        !pairs.is_empty(),
        "anonymous default export should produce findings via fallback"
    );
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "should find text-red-500 via fallback, got: {names:?}"
    );
}

#[test]
fn hoc_wrapped_export_no_crash() {
    // export default withAuth(SidebarInner) — HOC pattern.
    // find_component_body won't match because name lookup fails.
    // Should not crash; findings come from full-file fallback.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
function SidebarInner() {
    return <p className="text-red-500">inner</p>;
}

export default withAuth(SidebarInner);
"#,
    );

    // We need a dummy withAuth so it doesn't error, but since we're doing
    // static analysis (not execution), the file just needs to parse.
    write_file(
        dir.path(),
        "page.tsx",
        r#"
import Sidebar from "./sidebar";

export function Page() {
    return <div className="bg-white"><Sidebar /></div>;
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    // Must not panic
    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    // Fallback will scan the entire file, finding text-red-500 from SidebarInner.
    // This is over-reporting (SidebarInner may not be the exported component),
    // but it's the expected fallback behavior.
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-red-500".to_string()),
        "HOC fallback should find text-red-500, got: {names:?}"
    );
}

// ============================================================================
// 6. CSS variable resolution through propagation
// ============================================================================

#[test]
fn css_var_resolved_colors_propagate() {
    // Test the full pipeline with CSS variables:
    // globals.css defines --primary-foreground, tailwind.config maps
    // "primary-foreground" -> "hsl(var(--primary-foreground))",
    // and the component uses text-primary-foreground.
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export function Sidebar() {
    return <p className="text-primary-foreground">hello</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Sidebar } from "./sidebar";

export function Page() {
    return <div className="bg-white"><Sidebar /></div>;
}
"#,
    );

    // Set up resolved vars with --primary-foreground
    let mut light_vars = HashMap::new();
    light_vars.insert("--primary-foreground".to_string(), Rgba::opaque(255, 255, 255));

    let resolved = ResolvedVarColors {
        light: light_vars,
        dark: HashMap::new(),
    };

    // Set up TailwindColorConfig so "primary-foreground" maps to the var
    let mut tw = TailwindColorConfig::default();
    tw.colors.insert(
        "primary-foreground".to_string(),
        "hsl(var(--primary-foreground))".to_string(),
    );

    let graph = build_component_graph(dir.path());
    let pairs = propagate_and_check(&graph, &tw, &resolved, Theme::Light);

    assert!(
        !pairs.is_empty(),
        "CSS var colors should propagate through the pipeline"
    );
    let names = fg_names(&pairs);
    assert!(
        names.contains(&"text-primary-foreground".to_string()),
        "should find text-primary-foreground, got: {names:?}"
    );

    // Verify the resolved color is white
    let pair = pairs.iter().find(|p| p.foreground_name.starts_with("text-primary-foreground")).unwrap();
    assert_eq!(
        (pair.foreground_color.r, pair.foreground_color.g, pair.foreground_color.b),
        (255, 255, 255),
        "resolved color should be white"
    );
}

// ============================================================================
// 7. Report consistency
// ============================================================================

#[test]
fn propagation_reports_parent_file_and_line() {
    // The reported file should be the parent (page.tsx where <Sidebar /> is used),
    // not the child file (sidebar.tsx where the text color is defined).
    let dir = TempDir::new().unwrap();

    write_file(
        dir.path(),
        "sidebar.tsx",
        r#"
export function Sidebar() {
    return <p className="text-red-500">hello</p>;
}
"#,
    );

    write_file(
        dir.path(),
        "page.tsx",
        r#"
import { Sidebar } from "./sidebar";

export function Page() {
    return (
        <div className="bg-white">
            <Sidebar />
        </div>
    );
}
"#,
    );

    let graph = build_component_graph(dir.path());
    let tw = TailwindColorConfig::default();
    let vars = default_vars();

    let pairs = propagate_and_check(&graph, &tw, &vars, Theme::Light);

    assert!(!pairs.is_empty());

    for pair in &pairs {
        assert!(
            pair.file.contains("page.tsx"),
            "file should be parent (page.tsx), got: {}",
            pair.file
        );
        assert!(pair.line > 0, "line should be > 0");
        assert_eq!(pair.element, "Sidebar", "element should be the component name");
    }
}

// ============================================================================
// 8. Tailwind v4 @theme support
// ============================================================================

#[test]
fn tailwind_v4_theme_block_parsed() {
    // Tailwind v4 uses @theme { --color-primary: #3b82f6; } instead of
    // :root with bare HSL values.
    let css = r#"
        @theme {
            --color-primary: #3b82f6;
            --color-primary-foreground: #ffffff;
            --color-background: #0a0a0a;
        }
    "#;

    let map = extract_css_vars(css);

    assert_eq!(
        map.light.get("--color-primary").unwrap(),
        "#3b82f6",
        "@theme vars should parse into light theme"
    );
    assert_eq!(
        map.light.get("--color-primary-foreground").unwrap(),
        "#ffffff"
    );
    assert_eq!(map.light.get("--color-background").unwrap(), "#0a0a0a");

    // Verify resolution works
    let resolved = resolve_var_colors(&map);
    let primary = resolved.light.get("--color-primary").unwrap();
    assert_eq!((primary.r, primary.g, primary.b), (59, 130, 246));
}

#[test]
fn tailwind_v4_theme_inline_parsed() {
    // @theme inline { ... } is also valid in Tailwind v4.
    let css = r#"
        @theme inline {
            --color-accent: oklch(0.7 0.15 200);
            --color-accent-foreground: #000000;
        }
    "#;

    let map = extract_css_vars(css);

    assert!(
        map.light.contains_key("--color-accent"),
        "@theme inline vars should be parsed"
    );
    assert_eq!(
        map.light.get("--color-accent-foreground").unwrap(),
        "#000000"
    );
}

#[test]
fn tailwind_v4_theme_coexists_with_root() {
    // A project might have both @theme (v4) and :root (v3-style) blocks.
    let css = r#"
        :root {
            --background: 0 0% 100%;
            --foreground: 0 0% 0%;
        }
        .dark {
            --background: 0 0% 0%;
        }
        @theme {
            --color-primary: #3b82f6;
        }
    "#;

    let map = extract_css_vars(css);

    // :root vars should still work
    assert_eq!(map.light.get("--background").unwrap(), "0 0% 100%");
    assert_eq!(map.dark.get("--background").unwrap(), "0 0% 0%");

    // @theme vars should also be present
    assert_eq!(map.light.get("--color-primary").unwrap(), "#3b82f6");
}

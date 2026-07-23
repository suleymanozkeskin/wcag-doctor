use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use swc_ecma_ast::*;

use crate::color::Rgba;
use crate::contrast::levels::ConformanceLevel;
use crate::contrast::wcag::contrast_ratio;
use crate::resolver::css_vars::ResolvedVarColors;
use crate::resolver::tailwind::{
    TailwindColorConfig, parse_utility_class, resolve_tailwind_color,
};
use crate::scanner::component::{
    ColorPair, Theme, ThemeContext, collect_foreground_colors_from_block,
    collect_foreground_colors_from_expr, parse_variant_context,
};
use crate::scanner::graph::ComponentNode;

/// Propagate inherited background colors across the component graph
/// and detect contrast violations in child components.
pub fn propagate_and_check(
    graph: &HashMap<PathBuf, ComponentNode>,
    tw_config: &TailwindColorConfig,
    resolved_vars: &ResolvedVarColors,
    theme: Theme,
) -> Vec<ColorPair> {
    let vars = match theme {
        Theme::Light => &resolved_vars.light,
        Theme::Dark => &resolved_vars.dark,
    };

    // Cache: (file, component_name) -> text colors for that component only
    let mut text_color_cache: HashMap<(PathBuf, String), Vec<(String, Rgba)>> = HashMap::new();
    let mut pairs = Vec::new();

    let mut sorted_files: Vec<_> = graph.keys().collect();
    sorted_files.sort();

    let theme_label = theme.label().to_string();

    for file in sorted_files {
        let node = &graph[file];
        for usage in &node.usages {
            let bg_colors = resolve_bg_classes(&usage.parent_bg_classes, tw_config, vars, theme);

            if bg_colors.is_empty() {
                continue;
            }

            let direct_target = match node.imports.get(&usage.component_name) {
                Some(f) => f,
                None => continue,
            };

            if !graph.contains_key(direct_target) {
                continue;
            }

            // Follow re-export chains: if the target file re-exports this
            // component (has it in its imports), resolve to the actual source.
            let target_file = resolve_reexport_chain(
                direct_target,
                &usage.component_name,
                graph,
            );

            let cache_key = (target_file.clone(), usage.component_name.clone());
            let text_colors = text_color_cache
                .entry(cache_key)
                .or_insert_with(|| {
                    extract_text_colors_for_component(
                        target_file,
                        &usage.component_name,
                        tw_config,
                        vars,
                        theme,
                    )
                });

            let parent_filename = file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();
            let child_filename = target_file
                .file_name()
                .unwrap_or_default()
                .to_string_lossy();

            for (bg_name, bg_rgba) in &bg_colors {
                for (fg_name, fg_rgba) in text_colors.iter() {
                    let ratio = contrast_ratio(fg_rgba, bg_rgba);
                    let level = ConformanceLevel::from_ratio(ratio);
                    pairs.push(ColorPair {
                        foreground_name: format!("{fg_name} (in {child_filename})"),
                        foreground_color: *fg_rgba,
                        background_name: format!("{bg_name} (on {parent_filename})"),
                        background_color: *bg_rgba,
                        ratio,
                        level,
                        file: file.to_string_lossy().to_string(),
                        line: usage.line,
                        element: usage.component_name.clone(),
                        theme: theme_label.clone(),
                        state: "base".to_string(),
                    });
                }
            }
        }
    }

    pairs
}

/// Follow re-export chains to find the actual source file for a component.
/// E.g. index.tsx re-exports Sidebar from sidebar.tsx — this resolves
/// index.tsx → sidebar.tsx.
fn resolve_reexport_chain<'a>(
    start: &'a PathBuf,
    component_name: &str,
    graph: &'a HashMap<PathBuf, ComponentNode>,
) -> &'a PathBuf {
    let mut current = start;
    let mut visited = HashSet::new();

    while visited.insert(current.clone()) {
        if let Some(node) = graph.get(current) {
            // If this file has the component in its imports, it's a re-export.
            // Follow to the actual source.
            if let Some(next) = node.imports.get(component_name) {
                if graph.contains_key(next) {
                    current = next;
                    continue;
                }
            }
        }
        break;
    }

    current
}

/// Resolve background classes with theme-aware filtering and override.
///
/// Classes may include variant prefixes (e.g. "dark:bg-slate-900").
/// In dark mode, `dark:bg-*` overrides unprefixed `bg-*` (matching
/// Tailwind's CSS specificity). Same symmetry for `light:` in light mode.
fn resolve_bg_classes(
    classes: &[String],
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    theme: Theme,
) -> Vec<(String, Rgba)> {
    let mut base_bgs: Vec<(String, Rgba)> = Vec::new();
    let mut override_bgs: Vec<(String, Rgba)> = Vec::new();

    for class in classes {
        let (context, base_class) = parse_variant_context(class);

        // Skip classes that don't apply to this theme
        match (theme, context) {
            (Theme::Light, ThemeContext::DarkOnly) => continue,
            (Theme::Dark, ThemeContext::LightOnly) => continue,
            _ => {}
        }

        if let Some((kind, color_name)) = parse_utility_class(base_class) {
            if kind.is_background() {
                if let Some(rgba) = resolve_tailwind_color(&color_name, tw_config, vars) {
                    let is_override = match theme {
                        Theme::Light => context == ThemeContext::LightOnly,
                        Theme::Dark => context == ThemeContext::DarkOnly,
                    };
                    if is_override {
                        override_bgs.push((base_class.to_string(), rgba));
                    } else {
                        base_bgs.push((base_class.to_string(), rgba));
                    }
                }
            }
        }
    }

    if !override_bgs.is_empty() { override_bgs } else { base_bgs }
}

/// Extract text-color classes from a specific exported component in a file.
///
/// Parses the file's AST, finds the function matching `component_name`
/// (by named export or default export), and walks only that function's
/// JSX tree for foreground-color Tailwind classes.
///
/// If the component can't be found (e.g. re-exported from elsewhere),
/// falls back to scanning the entire file.
fn extract_text_colors_for_component(
    file: &PathBuf,
    component_name: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    theme: Theme,
) -> Vec<(String, Rgba)> {
    let content = match std::fs::read_to_string(file) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let module = match parse_module_for_propagation(&content, file) {
        Some(m) => m,
        None => return Vec::new(),
    };

    // Try to find the specific component function body
    if let Some(body) = find_component_body(&module, component_name) {
        return match body {
            ComponentBodyRef::Block(block) => {
                collect_foreground_colors_from_block(block, &content, tw_config, vars, theme)
            }
            ComponentBodyRef::Expr(expr) => {
                collect_foreground_colors_from_expr(expr, &content, tw_config, vars, theme)
            }
        };
    }

    // Fallback: couldn't find the named component, walk the full module AST.
    // This covers re-exports, HOCs, and other patterns we can't statically resolve
    // without matching comments or arbitrary strings.
    collect_foreground_colors_from_module(&module, &content, tw_config, vars, theme)
}

enum ComponentBodyRef<'a> {
    Block(&'a BlockStmt),
    Expr(&'a Expr),
}

fn find_component_body<'a>(module: &'a Module, name: &str) -> Option<ComponentBodyRef<'a>> {
    // Pass 1: Try exact name match
    for item in &module.body {
        if let ModuleItem::ModuleDecl(decl) = item {
            match decl {
                // export function Sidebar() { ... }
                ModuleDecl::ExportDecl(export) => {
                    if let Some(span) = exported_fn_body_span(&export.decl, name) {
                        return Some(span);
                    }
                }
                // export default function Sidebar() { ... }
                ModuleDecl::ExportDefaultDecl(export) => {
                    if let DefaultDecl::Fn(f) = &export.decl {
                        let matches = f
                            .ident
                            .as_ref()
                            .is_some_and(|id| id.sym.as_str() == name);
                        if matches {
                            if let Some(body) = &f.function.body {
                                return Some(ComponentBodyRef::Block(body));
                            }
                        }
                    }
                }
                _ => {}
            }
        }

        // Also check top-level const declarations:
        // const Sidebar = () => { ... }
        if let ModuleItem::Stmt(Stmt::Decl(decl)) | ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(ExportDecl { decl, .. })) = item {
            if let Decl::Var(var_decl) = decl {
                for d in &var_decl.decls {
                    if let Pat::Ident(ident) = &d.name {
                        if ident.sym.as_str() == name {
                            if let Some(init) = &d.init {
                                if let Some(span) = arrow_or_fn_body_ref(init) {
                                    return Some(span);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    // Pass 2: Name didn't match — this happens with aliased default imports
    // (e.g. `import Nav from "./sidebar"` where the function is named `Sidebar`).
    // Fall back to the sole default export regardless of its name.
    find_default_export_body(module)
}

/// Find the body of the default export, regardless of its name.
/// Used as a fallback when the import name doesn't match the function name.
fn find_default_export_body<'a>(module: &'a Module) -> Option<ComponentBodyRef<'a>> {
    for item in &module.body {
        if let ModuleItem::ModuleDecl(decl) = item {
            match decl {
                ModuleDecl::ExportDefaultDecl(export) => {
                    if let DefaultDecl::Fn(f) = &export.decl {
                        if let Some(body) = &f.function.body {
                            return Some(ComponentBodyRef::Block(body));
                        }
                    }
                }
                ModuleDecl::ExportDefaultExpr(export) => {
                    if let Some(body) = arrow_or_fn_body_ref(&export.expr) {
                        return Some(body);
                    }
                }
                _ => {}
            }
        }
    }
    None
}

fn exported_fn_body_span<'a>(decl: &'a Decl, name: &str) -> Option<ComponentBodyRef<'a>> {
    match decl {
        Decl::Fn(f) if f.ident.sym.as_str() == name => {
            f.function.body.as_ref().map(ComponentBodyRef::Block)
        }
        Decl::Var(var_decl) => {
            for d in &var_decl.decls {
                if let Pat::Ident(ident) = &d.name {
                    if ident.sym.as_str() == name {
                        if let Some(init) = &d.init {
                            return arrow_or_fn_body_ref(init);
                        }
                    }
                }
            }
            None
        }
        _ => None,
    }
}

fn arrow_or_fn_body_ref<'a>(expr: &'a Expr) -> Option<ComponentBodyRef<'a>> {
    match expr {
        Expr::Arrow(arrow) => {
            match &*arrow.body {
                BlockStmtOrExpr::BlockStmt(block) => Some(ComponentBodyRef::Block(block)),
                BlockStmtOrExpr::Expr(e) => Some(ComponentBodyRef::Expr(e)),
            }
        }
        Expr::Fn(f) => f.function.body.as_ref().map(ComponentBodyRef::Block),
        // Unwrap common wrappers: memo((...) => ...), forwardRef((...) => ...)
        Expr::Call(call) => {
            for arg in &call.args {
                if let Some(span) = arrow_or_fn_body_ref(&arg.expr) {
                    return Some(span);
                }
            }
            None
        }
        Expr::Paren(p) => arrow_or_fn_body_ref(&p.expr),
        _ => None,
    }
}

fn collect_foreground_colors_from_module(
    module: &Module,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    theme: Theme,
) -> Vec<(String, Rgba)> {
    let mut colors = Vec::new();

    for item in &module.body {
        match item {
            ModuleItem::Stmt(Stmt::Decl(Decl::Fn(f))) => {
                if let Some(body) = &f.function.body {
                    colors.extend(collect_foreground_colors_from_block(
                        body, source, tw_config, vars, theme,
                    ));
                }
            }
            ModuleItem::Stmt(Stmt::Decl(Decl::Var(var_decl))) => {
                for decl in &var_decl.decls {
                    if let Some(init) = &decl.init {
                        colors.extend(collect_foreground_colors_from_expr(
                            init, source, tw_config, vars, theme,
                        ));
                    }
                }
            }
            ModuleItem::Stmt(Stmt::Expr(expr)) => {
                colors.extend(collect_foreground_colors_from_expr(
                    &expr.expr, source, tw_config, vars, theme,
                ));
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) => match &export.decl {
                Decl::Fn(f) => {
                    if let Some(body) = &f.function.body {
                        colors.extend(collect_foreground_colors_from_block(
                            body, source, tw_config, vars, theme,
                        ));
                    }
                }
                Decl::Var(var_decl) => {
                    for decl in &var_decl.decls {
                        if let Some(init) = &decl.init {
                            colors.extend(collect_foreground_colors_from_expr(
                                init, source, tw_config, vars, theme,
                            ));
                        }
                    }
                }
                _ => {}
            },
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultDecl(export)) => {
                if let DefaultDecl::Fn(f) = &export.decl {
                    if let Some(body) = &f.function.body {
                        colors.extend(collect_foreground_colors_from_block(
                            body, source, tw_config, vars, theme,
                        ));
                    }
                }
            }
            ModuleItem::ModuleDecl(ModuleDecl::ExportDefaultExpr(export)) => {
                colors.extend(collect_foreground_colors_from_expr(
                    &export.expr,
                    source,
                    tw_config,
                    vars,
                    theme,
                ));
            }
            _ => {}
        }
    }

    let mut deduped = Vec::new();
    let mut seen = HashSet::new();
    for (name, rgba) in colors {
        if seen.insert(name.clone()) {
            deduped.push((name, rgba));
        }
    }

    deduped
}

fn parse_module_for_propagation(content: &str, file_path: &Path) -> Option<Module> {
    super::parse_module(content, file_path)
}

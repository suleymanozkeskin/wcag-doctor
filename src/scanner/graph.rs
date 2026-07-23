use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rayon::prelude::*;
use serde_json::Value;
use swc_ecma_ast::*;

use crate::scanner::component::{
    InteractionState, byte_offset_to_line, extract_strings_from_expr, interaction_state_of,
};

/// A node in the component import/usage graph.
#[derive(Debug)]
pub struct ComponentNode {
    /// File path where this component is defined.
    pub file: PathBuf,
    /// Components this file imports (component name -> source file path).
    pub imports: HashMap<String, PathBuf>,
    /// Components exported from this file.
    pub exports: HashSet<String>,
    /// JSX usages: which imported components are rendered, with their parent's bg color class.
    pub usages: Vec<ComponentUsage>,
}

/// Records that a component is used inside a parent element with certain background classes.
#[derive(Debug)]
pub struct ComponentUsage {
    /// The name of the imported component being used (e.g. "Sidebar").
    pub component_name: String,
    /// Background color classes found on the nearest parent wrapper (if any).
    pub parent_bg_classes: Vec<String>,
    /// Line number where the usage occurs.
    pub line: usize,
}

#[derive(Debug)]
struct ImportResolver {
    alias_base_dir: PathBuf,
    aliases: Vec<PathAlias>,
}

#[derive(Debug)]
struct PathAlias {
    pattern: String,
    replacements: Vec<String>,
}

/// Build the component graph for all TSX/JSX files in a directory.
pub fn build_component_graph(dir: &Path) -> HashMap<PathBuf, ComponentNode> {
    let mut graph: HashMap<PathBuf, ComponentNode> = HashMap::new();
    let resolver = ImportResolver::new(dir);

    let walker = match globwalk::GlobWalkerBuilder::from_patterns(
        dir,
        &["**/*.tsx", "**/*.jsx", "**/*.ts", "**/*.js"],
    )
    .max_depth(20)
    .build()
    {
        Ok(w) => w,
        Err(_) => return graph,
    };

    // Filter out node_modules, .next, dist, build
    let files: Vec<PathBuf> = walker
        .filter_map(|e| e.ok())
        .map(|e| e.path().to_path_buf())
        .filter(|p| !crate::config::is_excluded_path(&p.to_string_lossy()))
        .collect();

    let nodes: Vec<(PathBuf, ComponentNode)> = files
        .par_iter()
        .filter_map(|file| {
            analyze_file(file, &resolver).map(|node| (file.clone(), node))
        })
        .collect();

    for (file, node) in nodes {
        graph.insert(file, node);
    }

    graph
}

impl ImportResolver {
    fn new(scan_root: &Path) -> Self {
        let Some(config_path) = find_tsconfig(scan_root) else {
            return Self {
                alias_base_dir: scan_root.to_path_buf(),
                aliases: Vec::new(),
            };
        };

        let config_dir = config_path
            .parent()
            .unwrap_or(scan_root)
            .to_path_buf();
        let content = match std::fs::read_to_string(&config_path) {
            Ok(content) => content,
            Err(_) => {
                return Self {
                    alias_base_dir: config_dir,
                    aliases: Vec::new(),
                };
            }
        };
        let json: Value = match json5::from_str(&content) {
            Ok(json) => json,
            Err(_) => {
                return Self {
                    alias_base_dir: config_dir,
                    aliases: Vec::new(),
                };
            }
        };

        let compiler_options = json
            .get("compilerOptions")
            .and_then(Value::as_object);
        let alias_base_dir = compiler_options
            .and_then(|options| options.get("baseUrl"))
            .and_then(Value::as_str)
            .map(|base_url| config_dir.join(base_url))
            .unwrap_or_else(|| config_dir.clone());

        let aliases = compiler_options
            .and_then(|options| options.get("paths"))
            .and_then(Value::as_object)
            .map(|paths| {
                paths.iter()
                    .filter_map(|(pattern, targets)| {
                        let replacements = targets
                            .as_array()?
                            .iter()
                            .filter_map(Value::as_str)
                            .map(ToOwned::to_owned)
                            .collect::<Vec<_>>();
                        if replacements.is_empty() {
                            None
                        } else {
                            Some(PathAlias {
                                pattern: pattern.clone(),
                                replacements,
                            })
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        Self {
            alias_base_dir,
            aliases,
        }
    }

    fn resolve_import_path(&self, source: &str, file_dir: &Path) -> Option<PathBuf> {
        if source.starts_with('.') || source.starts_with('/') {
            return resolve_candidate_path(&file_dir.join(source));
        }

        for alias in &self.aliases {
            let Some(wildcard) = match_pattern(&alias.pattern, source) else {
                continue;
            };
            for replacement in &alias.replacements {
                let resolved = apply_pattern(replacement, wildcard);
                if let Some(path) = resolve_candidate_path(&self.alias_base_dir.join(resolved)) {
                    return Some(path);
                }
            }
        }

        None
    }
}

fn find_tsconfig(scan_root: &Path) -> Option<PathBuf> {
    let mut current = Some(scan_root);
    while let Some(dir) = current {
        for name in ["tsconfig.json", "jsconfig.json"] {
            let path = dir.join(name);
            if path.exists() {
                return Some(path);
            }
        }
        current = dir.parent();
    }
    None
}

fn match_pattern<'a>(pattern: &str, source: &'a str) -> Option<&'a str> {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => {
            if source.starts_with(prefix)
                && source.ends_with(suffix)
                && source.len() >= prefix.len() + suffix.len()
            {
                let end = source.len() - suffix.len();
                Some(&source[prefix.len()..end])
            } else {
                None
            }
        }
        None if pattern == source => Some(""),
        None => None,
    }
}

fn apply_pattern(pattern: &str, wildcard: &str) -> String {
    match pattern.split_once('*') {
        Some((prefix, suffix)) => format!("{prefix}{wildcard}{suffix}"),
        None => pattern.to_string(),
    }
}

fn analyze_file(file_path: &Path, resolver: &ImportResolver) -> Option<ComponentNode> {
    let content = std::fs::read_to_string(file_path).ok()?;
    let module = parse_module(&content, file_path)?;

    let mut node = ComponentNode {
        file: file_path.to_path_buf(),
        imports: HashMap::new(),
        exports: HashSet::new(),
        usages: Vec::new(),
    };

    let file_dir = file_path.parent().unwrap_or(&resolver.alias_base_dir);

    for item in &module.body {
        if let ModuleItem::ModuleDecl(decl) = item {
            extract_imports_and_exports(decl, file_dir, resolver, &mut node);
        }
    }

    // Find JSX usages of imported components
    for item in &module.body {
        find_jsx_usages(item, &content, &node.imports, &mut node.usages);
    }

    Some(node)
}

fn parse_module(content: &str, file_path: &Path) -> Option<Module> {
    super::parse_module(content, file_path)
}

fn extract_imports_and_exports(
    decl: &ModuleDecl,
    file_dir: &Path,
    resolver: &ImportResolver,
    node: &mut ComponentNode,
) {
    match decl {
        ModuleDecl::Import(import) => {
            let source = import.src.value.as_str().unwrap_or_default().to_owned();
            if let Some(resolved) = resolver.resolve_import_path(&source, file_dir) {
                for spec in &import.specifiers {
                    let name = match spec {
                        ImportSpecifier::Named(named) => named.local.sym.as_str().to_owned(),
                        ImportSpecifier::Default(default) => default.local.sym.as_str().to_owned(),
                        ImportSpecifier::Namespace(ns) => ns.local.sym.as_str().to_owned(),
                    };
                    // Only track PascalCase names (likely components)
                    if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                        node.imports.insert(name, resolved.clone());
                    }
                }
            }
        }
        ModuleDecl::ExportDecl(export) => {
            if let Decl::Fn(f) = &export.decl {
                let name = f.ident.sym.as_str().to_owned();
                if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    node.exports.insert(name);
                }
            }
        }
        ModuleDecl::ExportDefaultDecl(export) => {
            if let DefaultDecl::Fn(f) = &export.decl {
                if let Some(ident) = &f.ident {
                    node.exports.insert(ident.sym.as_str().to_owned());
                }
            }
        }
        // export { Sidebar } from "./sidebar" — barrel re-export
        ModuleDecl::ExportNamed(named) => {
            if let Some(src) = &named.src {
                let source = src.value.as_str().unwrap_or_default().to_owned();
                if let Some(resolved) = resolver.resolve_import_path(&source, file_dir) {
                    for spec in &named.specifiers {
                        if let ExportSpecifier::Named(named_spec) = spec {
                            let name = named_spec
                                .exported
                                .as_ref()
                                .map(|e| match e {
                                    ModuleExportName::Ident(i) => i.sym.as_str().to_owned(),
                                    ModuleExportName::Str(s) => {
                                        s.value.as_str().unwrap_or_default().to_owned()
                                    }
                                })
                                .unwrap_or_else(|| match &named_spec.orig {
                                    ModuleExportName::Ident(i) => i.sym.as_str().to_owned(),
                                    ModuleExportName::Str(s) => {
                                        s.value.as_str().unwrap_or_default().to_owned()
                                    }
                                });
                            if name.chars().next().is_some_and(|c| c.is_uppercase()) {
                                // Track both as import (for resolution) and export
                                node.imports.insert(name.clone(), resolved.clone());
                                node.exports.insert(name);
                            }
                        }
                    }
                }
            }
        }
        _ => {}
    }
}

/// Resolve a source path candidate to a concrete absolute file path.
fn resolve_candidate_path(base: &Path) -> Option<PathBuf> {
    let extensions = ["tsx", "ts", "jsx", "js"];

    // Try exact path
    if base.exists() && base.is_file() {
        return Some(base.to_path_buf());
    }

    // Try with extensions
    for ext in &extensions {
        let with_ext = base.with_extension(ext);
        if with_ext.exists() {
            return Some(with_ext);
        }
    }

    // Try as directory with index file
    for ext in &extensions {
        let index = base.join(format!("index.{ext}"));
        if index.exists() {
            return Some(index);
        }
    }

    None
}

fn find_jsx_usages(
    item: &ModuleItem,
    source: &str,
    imports: &HashMap<String, PathBuf>,
    usages: &mut Vec<ComponentUsage>,
) {
    // Walk through all JSX elements and check if any match imported components
    walk_for_usages_item(item, source, imports, usages, &[]);
}

fn walk_for_usages_item(
    item: &ModuleItem,
    source: &str,
    imports: &HashMap<String, PathBuf>,
    usages: &mut Vec<ComponentUsage>,
    parent_bg_classes: &[String],
) {
    match item {
        ModuleItem::Stmt(stmt) => {
            walk_for_usages_stmt(stmt, source, imports, usages, parent_bg_classes)
        }
        ModuleItem::ModuleDecl(decl) => {
            walk_for_usages_decl(decl, source, imports, usages, parent_bg_classes)
        }
    }
}

fn walk_for_usages_decl(
    decl: &ModuleDecl,
    source: &str,
    imports: &HashMap<String, PathBuf>,
    usages: &mut Vec<ComponentUsage>,
    parent_bg_classes: &[String],
) {
    match decl {
        ModuleDecl::ExportDecl(export) => match &export.decl {
            Decl::Fn(f) => {
                if let Some(body) = &f.function.body {
                    for stmt in &body.stmts {
                        walk_for_usages_stmt(stmt, source, imports, usages, parent_bg_classes);
                    }
                }
            }
            Decl::Var(var_decl) => {
                for d in &var_decl.decls {
                    if let Some(init) = &d.init {
                        walk_for_usages_expr(init, source, imports, usages, parent_bg_classes);
                    }
                }
            }
            _ => {}
        },
        ModuleDecl::ExportDefaultDecl(export) => {
            if let DefaultDecl::Fn(f) = &export.decl {
                if let Some(body) = &f.function.body {
                    for stmt in &body.stmts {
                        walk_for_usages_stmt(stmt, source, imports, usages, parent_bg_classes);
                    }
                }
            }
        }
        ModuleDecl::ExportDefaultExpr(export) => {
            walk_for_usages_expr(&export.expr, source, imports, usages, parent_bg_classes);
        }
        _ => {}
    }
}

fn walk_for_usages_stmt(
    stmt: &Stmt,
    source: &str,
    imports: &HashMap<String, PathBuf>,
    usages: &mut Vec<ComponentUsage>,
    parent_bg_classes: &[String],
) {
    match stmt {
        Stmt::Return(ret) => {
            if let Some(arg) = &ret.arg {
                walk_for_usages_expr(arg, source, imports, usages, parent_bg_classes);
            }
        }
        Stmt::Expr(expr) => {
            walk_for_usages_expr(&expr.expr, source, imports, usages, parent_bg_classes)
        }
        Stmt::Block(block) => {
            for s in &block.stmts {
                walk_for_usages_stmt(s, source, imports, usages, parent_bg_classes);
            }
        }
        Stmt::If(if_stmt) => {
            walk_for_usages_stmt(&if_stmt.cons, source, imports, usages, parent_bg_classes);
            if let Some(alt) = &if_stmt.alt {
                walk_for_usages_stmt(alt, source, imports, usages, parent_bg_classes);
            }
        }
        Stmt::Decl(decl) => match decl {
            Decl::Fn(f) => {
                if let Some(body) = &f.function.body {
                    for s in &body.stmts {
                        walk_for_usages_stmt(s, source, imports, usages, parent_bg_classes);
                    }
                }
            }
            Decl::Var(var_decl) => {
                for d in &var_decl.decls {
                    if let Some(init) = &d.init {
                        walk_for_usages_expr(init, source, imports, usages, parent_bg_classes);
                    }
                }
            }
            _ => {}
        },
        _ => {}
    }
}

fn walk_for_usages_expr(
    expr: &Expr,
    source: &str,
    imports: &HashMap<String, PathBuf>,
    usages: &mut Vec<ComponentUsage>,
    parent_bg_classes: &[String],
) {
    match expr {
        Expr::JSXElement(el) => {
            process_jsx_for_usages(el, source, imports, usages, parent_bg_classes);
        }
        Expr::JSXFragment(frag) => {
            for child in &frag.children {
                walk_jsx_child_for_usages(child, source, imports, usages, parent_bg_classes);
            }
        }
        Expr::Paren(p) => {
            walk_for_usages_expr(&p.expr, source, imports, usages, parent_bg_classes)
        }
        Expr::Arrow(arrow) => match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(block) => {
                for s in &block.stmts {
                    walk_for_usages_stmt(s, source, imports, usages, parent_bg_classes);
                }
            }
            BlockStmtOrExpr::Expr(e) => {
                walk_for_usages_expr(e, source, imports, usages, parent_bg_classes)
            }
        },
        Expr::Call(call) => {
            for arg in &call.args {
                walk_for_usages_expr(&arg.expr, source, imports, usages, parent_bg_classes);
            }
        }
        Expr::Cond(cond) => {
            walk_for_usages_expr(&cond.cons, source, imports, usages, parent_bg_classes);
            walk_for_usages_expr(&cond.alt, source, imports, usages, parent_bg_classes);
        }
        _ => {}
    }
}

fn walk_jsx_child_for_usages(
    child: &JSXElementChild,
    source: &str,
    imports: &HashMap<String, PathBuf>,
    usages: &mut Vec<ComponentUsage>,
    parent_bg_classes: &[String],
) {
    match child {
        JSXElementChild::JSXElement(el) => {
            process_jsx_for_usages(el, source, imports, usages, parent_bg_classes);
        }
        JSXElementChild::JSXFragment(frag) => {
            for c in &frag.children {
                walk_jsx_child_for_usages(c, source, imports, usages, parent_bg_classes);
            }
        }
        JSXElementChild::JSXExprContainer(container) => {
            if let JSXExpr::Expr(expr) = &container.expr {
                walk_for_usages_expr(expr, source, imports, usages, parent_bg_classes);
            }
        }
        _ => {}
    }
}

fn process_jsx_for_usages(
    element: &JSXElement,
    source: &str,
    imports: &HashMap<String, PathBuf>,
    usages: &mut Vec<ComponentUsage>,
    parent_bg_classes: &[String],
) {
    let name = match &element.opening.name {
        JSXElementName::Ident(ident) => ident.sym.as_str().to_owned(),
        JSXElementName::JSXMemberExpr(member) => {
            format!("{}", member.prop.sym)
        }
        _ => String::new(),
    };

    // Extract bg classes from this element
    let mut current_bg_classes: Vec<String> = parent_bg_classes.to_vec();
    for attr in &element.opening.attrs {
        if let JSXAttrOrSpread::JSXAttr(jsx_attr) = attr {
            let attr_name = match &jsx_attr.name {
                JSXAttrName::Ident(i) => i.sym.as_str().to_owned(),
                _ => String::new(),
            };
            if attr_name == "className" || attr_name == "class" {
                if let Some(value) = &jsx_attr.value {
                    let class_str = match value {
                        JSXAttrValue::Str(s) => s.value.as_str().unwrap_or_default().to_owned(),
                        JSXAttrValue::JSXExprContainer(container) => {
                            if let JSXExpr::Expr(expr) = &container.expr {
                                extract_strings_from_expr(expr)
                            } else {
                                String::new()
                            }
                        }
                        _ => String::new(),
                    };
                    for class in class_str.split_whitespace() {
                        let base = match class.rfind(':') {
                            Some(idx) => &class[idx + 1..],
                            None => class,
                        };
                        // Only resting-state backgrounds are always-on ancestors.
                        // A `hover:bg-*`/`focus:bg-*` parent is not a background the
                        // child inherits at rest, so it must not propagate.
                        if base.starts_with("bg-")
                            && interaction_state_of(class) == InteractionState::Base
                        {
                            // Store full class with variant prefixes (e.g. "dark:bg-slate-900")
                            // so propagation can apply theme-aware filtering.
                            current_bg_classes.push(class.to_string());
                        }
                    }
                }
            }
        }
    }

    // Check if this element is an imported component
    if imports.contains_key(&name) {
        let line = byte_offset_to_line(source, element.opening.span.lo.0 as usize);

        usages.push(ComponentUsage {
            component_name: name.clone(),
            parent_bg_classes: current_bg_classes.clone(),
            line,
        });
    }

    // Recurse into children with updated bg context
    for child in &element.children {
        walk_jsx_child_for_usages(child, source, imports, usages, &current_bg_classes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn tsconfig_comments_do_not_break_alias_resolution() {
        let dir = TempDir::new().unwrap();
        let src_dir = dir.path().join("src/components");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(
            dir.path().join("tsconfig.json"),
            r#"
            {
              // standard tsconfig comment
              "compilerOptions": {
                "baseUrl": ".",
                "paths": {
                  "@/*": ["src/*"]
                }
              }
            }
            "#,
        )
        .unwrap();
        fs::write(src_dir.join("Button.tsx"), "export function Button() { return null; }").unwrap();

        let resolver = ImportResolver::new(dir.path());
        let resolved = resolver.resolve_import_path("@/components/Button", dir.path());

        assert_eq!(resolved, Some(src_dir.join("Button.tsx")));
    }
}

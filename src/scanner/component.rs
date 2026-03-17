use std::collections::{HashMap, HashSet};
use std::path::Path;

use swc_common::Spanned;
use swc_ecma_ast::*;

use crate::color::Rgba;
use crate::contrast::levels::ConformanceLevel;
use crate::contrast::wcag::contrast_ratio;
use crate::resolver::css_vars::ResolvedVarColors;
use crate::resolver::inline_styles::extract_inline_colors;
use crate::resolver::tailwind::{
    TailwindColorConfig, parse_utility_class, resolve_tailwind_color,
};

/// A detected color pair within a component.
#[derive(Debug)]
pub struct ColorPair {
    pub foreground_name: String,
    pub foreground_color: Rgba,
    pub background_name: String,
    pub background_color: Rgba,
    pub ratio: f64,
    pub level: ConformanceLevel,
    pub file: String,
    pub line: usize,
    pub element: String,
}

/// Colors found on a single JSX element.
#[derive(Debug, Default)]
struct ElementColors {
    foregrounds: Vec<(String, Rgba)>,
    backgrounds: Vec<(String, Rgba)>,
    line: usize,
    element_name: String,
}

/// Scan a single component file for color pairs.
pub fn scan_component(
    file_path: &Path,
    tw_config: &TailwindColorConfig,
    resolved_vars: &ResolvedVarColors,
    theme: Theme,
) -> Vec<ColorPair> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };

    let vars = match theme {
        Theme::Light => &resolved_vars.light,
        Theme::Dark => &resolved_vars.dark,
    };

    let module = match parse_tsx_module(&content, file_path) {
        Some(m) => m,
        None => return Vec::new(),
    };

    let file_str = file_path.to_string_lossy().to_string();

    let mut pairs = Vec::new();
    let no_inherited_bg: Vec<(String, Rgba)> = Vec::new();
    collect_jsx_pairs(
        &module,
        &content,
        tw_config,
        vars,
        &file_str,
        &mut pairs,
        &no_inherited_bg,
    );

    pairs
}

#[derive(Debug, Clone, Copy)]
pub enum Theme {
    Light,
    Dark,
}

/// Deduplicate component findings while preserving their original order.
/// Uses the reporting identity tuple `(file, line, element, foreground, background)`.
pub fn dedup_color_pairs(pairs: Vec<ColorPair>) -> Vec<ColorPair> {
    let mut seen = HashSet::new();
    let mut deduped = Vec::new();

    for pair in pairs {
        let key = (
            pair.file.clone(),
            pair.line,
            pair.element.clone(),
            pair.foreground_name.clone(),
            pair.background_name.clone(),
        );
        if seen.insert(key) {
            deduped.push(pair);
        }
    }

    deduped
}

/// Collect only foreground colors from a block body.
pub fn collect_foreground_colors_from_block(
    block: &BlockStmt,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
) -> Vec<(String, Rgba)> {
    let mut colors = Vec::new();
    let mut seen = HashSet::new();
    collect_foregrounds_block(block, source, tw_config, vars, &mut colors, &mut seen);
    colors
}

/// Collect only foreground colors from an expression body.
pub fn collect_foreground_colors_from_expr(
    expr: &Expr,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
) -> Vec<(String, Rgba)> {
    let mut colors = Vec::new();
    let mut seen = HashSet::new();
    collect_foregrounds_expr(expr, source, tw_config, vars, &mut colors, &mut seen);
    colors
}

fn parse_tsx_module(content: &str, file_path: &Path) -> Option<Module> {
    super::parse_module(content, file_path)
}

/// Walk the AST and collect color pairs from JSX elements.
fn collect_jsx_pairs(
    module: &Module,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    for item in &module.body {
        walk_module_item(item, source, tw_config, vars, file, pairs, inherited_bg);
    }
}

fn walk_module_item(
    item: &ModuleItem,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    match item {
        ModuleItem::Stmt(stmt) => {
            walk_stmt(stmt, source, tw_config, vars, file, pairs, inherited_bg)
        }
        ModuleItem::ModuleDecl(decl) => {
            walk_module_decl(decl, source, tw_config, vars, file, pairs, inherited_bg)
        }
    }
}

fn walk_module_decl(
    decl: &ModuleDecl,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    match decl {
        ModuleDecl::ExportDecl(export) => {
            walk_decl(&export.decl, source, tw_config, vars, file, pairs, inherited_bg)
        }
        ModuleDecl::ExportDefaultDecl(export) => {
            if let DefaultDecl::Fn(f) = &export.decl {
                if let Some(body) = &f.function.body {
                    walk_block_stmt(body, source, tw_config, vars, file, pairs, inherited_bg);
                }
            }
        }
        ModuleDecl::ExportDefaultExpr(export) => {
            walk_expr(&export.expr, source, tw_config, vars, file, pairs, inherited_bg);
        }
        _ => {}
    }
}

fn walk_decl(
    decl: &Decl,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    match decl {
        Decl::Fn(f) => {
            if let Some(body) = &f.function.body {
                walk_block_stmt(body, source, tw_config, vars, file, pairs, inherited_bg);
            }
        }
        Decl::Var(var_decl) => {
            for decl in &var_decl.decls {
                if let Some(init) = &decl.init {
                    walk_expr(init, source, tw_config, vars, file, pairs, inherited_bg);
                }
            }
        }
        _ => {}
    }
}

fn walk_stmt(
    stmt: &Stmt,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    match stmt {
        Stmt::Block(block) => {
            walk_block_stmt(block, source, tw_config, vars, file, pairs, inherited_bg)
        }
        Stmt::Return(ret) => {
            if let Some(arg) = &ret.arg {
                walk_expr(arg, source, tw_config, vars, file, pairs, inherited_bg);
            }
        }
        Stmt::Expr(expr) => {
            walk_expr(&expr.expr, source, tw_config, vars, file, pairs, inherited_bg)
        }
        Stmt::If(if_stmt) => {
            walk_stmt(
                &if_stmt.cons,
                source,
                tw_config,
                vars,
                file,
                pairs,
                inherited_bg,
            );
            if let Some(alt) = &if_stmt.alt {
                walk_stmt(alt, source, tw_config, vars, file, pairs, inherited_bg);
            }
        }
        Stmt::For(for_stmt) => {
            if let Some(init) = &for_stmt.init {
                walk_var_decl_or_expr(init, source, tw_config, vars, file, pairs, inherited_bg);
            }
            if let Some(test) = &for_stmt.test {
                walk_expr(test, source, tw_config, vars, file, pairs, inherited_bg);
            }
            if let Some(update) = &for_stmt.update {
                walk_expr(update, source, tw_config, vars, file, pairs, inherited_bg);
            }
            walk_stmt(&for_stmt.body, source, tw_config, vars, file, pairs, inherited_bg);
        }
        Stmt::ForIn(for_in_stmt) => {
            walk_var_decl_or_pat(&for_in_stmt.left, source, tw_config, vars, file, pairs, inherited_bg);
            walk_expr(&for_in_stmt.right, source, tw_config, vars, file, pairs, inherited_bg);
            walk_stmt(&for_in_stmt.body, source, tw_config, vars, file, pairs, inherited_bg);
        }
        Stmt::ForOf(for_of_stmt) => {
            walk_var_decl_or_pat(&for_of_stmt.left, source, tw_config, vars, file, pairs, inherited_bg);
            walk_expr(&for_of_stmt.right, source, tw_config, vars, file, pairs, inherited_bg);
            walk_stmt(&for_of_stmt.body, source, tw_config, vars, file, pairs, inherited_bg);
        }
        Stmt::While(while_stmt) => {
            walk_expr(&while_stmt.test, source, tw_config, vars, file, pairs, inherited_bg);
            walk_stmt(&while_stmt.body, source, tw_config, vars, file, pairs, inherited_bg);
        }
        Stmt::DoWhile(do_while_stmt) => {
            walk_stmt(&do_while_stmt.body, source, tw_config, vars, file, pairs, inherited_bg);
            walk_expr(&do_while_stmt.test, source, tw_config, vars, file, pairs, inherited_bg);
        }
        Stmt::Switch(switch_stmt) => {
            walk_expr(&switch_stmt.discriminant, source, tw_config, vars, file, pairs, inherited_bg);
            for case in &switch_stmt.cases {
                if let Some(test) = &case.test {
                    walk_expr(test, source, tw_config, vars, file, pairs, inherited_bg);
                }
                for stmt in &case.cons {
                    walk_stmt(stmt, source, tw_config, vars, file, pairs, inherited_bg);
                }
            }
        }
        Stmt::Try(try_stmt) => {
            walk_block_stmt(&try_stmt.block, source, tw_config, vars, file, pairs, inherited_bg);
            if let Some(handler) = &try_stmt.handler {
                walk_block_stmt(&handler.body, source, tw_config, vars, file, pairs, inherited_bg);
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                walk_block_stmt(finalizer, source, tw_config, vars, file, pairs, inherited_bg);
            }
        }
        Stmt::Decl(decl) => walk_decl(decl, source, tw_config, vars, file, pairs, inherited_bg),
        _ => {}
    }
}

fn walk_var_decl_or_expr(
    init: &VarDeclOrExpr,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    match init {
        VarDeclOrExpr::VarDecl(var_decl) => {
            walk_decl(&Decl::Var(var_decl.clone()), source, tw_config, vars, file, pairs, inherited_bg);
        }
        VarDeclOrExpr::Expr(expr) => {
            walk_expr(expr, source, tw_config, vars, file, pairs, inherited_bg);
        }
    }
}

fn walk_var_decl_or_pat(
    left: &ForHead,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    match left {
        ForHead::VarDecl(var_decl) => {
            walk_decl(&Decl::Var(var_decl.clone()), source, tw_config, vars, file, pairs, inherited_bg);
        }
        ForHead::UsingDecl(_) => {}
        ForHead::Pat(_) => {}
    }
}

fn walk_block_stmt(
    block: &BlockStmt,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    for stmt in &block.stmts {
        walk_stmt(stmt, source, tw_config, vars, file, pairs, inherited_bg);
    }
}

fn collect_foregrounds_block(
    block: &BlockStmt,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    colors: &mut Vec<(String, Rgba)>,
    seen: &mut HashSet<String>,
) {
    for stmt in &block.stmts {
        collect_foregrounds_stmt(stmt, source, tw_config, vars, colors, seen);
    }
}

fn collect_foregrounds_stmt(
    stmt: &Stmt,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    colors: &mut Vec<(String, Rgba)>,
    seen: &mut HashSet<String>,
) {
    match stmt {
        Stmt::Block(block) => collect_foregrounds_block(block, source, tw_config, vars, colors, seen),
        Stmt::Return(ret) => {
            if let Some(arg) = &ret.arg {
                collect_foregrounds_expr(arg, source, tw_config, vars, colors, seen);
            }
        }
        Stmt::Expr(expr) => {
            collect_foregrounds_expr(&expr.expr, source, tw_config, vars, colors, seen)
        }
        Stmt::If(if_stmt) => {
            collect_foregrounds_stmt(&if_stmt.cons, source, tw_config, vars, colors, seen);
            if let Some(alt) = &if_stmt.alt {
                collect_foregrounds_stmt(alt, source, tw_config, vars, colors, seen);
            }
        }
        Stmt::For(for_stmt) => {
            if let Some(init) = &for_stmt.init {
                collect_foregrounds_var_decl_or_expr(init, source, tw_config, vars, colors, seen);
            }
            if let Some(test) = &for_stmt.test {
                collect_foregrounds_expr(test, source, tw_config, vars, colors, seen);
            }
            if let Some(update) = &for_stmt.update {
                collect_foregrounds_expr(update, source, tw_config, vars, colors, seen);
            }
            collect_foregrounds_stmt(&for_stmt.body, source, tw_config, vars, colors, seen);
        }
        Stmt::ForIn(for_in_stmt) => {
            collect_foregrounds_for_head(&for_in_stmt.left, source, tw_config, vars, colors, seen);
            collect_foregrounds_expr(&for_in_stmt.right, source, tw_config, vars, colors, seen);
            collect_foregrounds_stmt(&for_in_stmt.body, source, tw_config, vars, colors, seen);
        }
        Stmt::ForOf(for_of_stmt) => {
            collect_foregrounds_for_head(&for_of_stmt.left, source, tw_config, vars, colors, seen);
            collect_foregrounds_expr(&for_of_stmt.right, source, tw_config, vars, colors, seen);
            collect_foregrounds_stmt(&for_of_stmt.body, source, tw_config, vars, colors, seen);
        }
        Stmt::While(while_stmt) => {
            collect_foregrounds_expr(&while_stmt.test, source, tw_config, vars, colors, seen);
            collect_foregrounds_stmt(&while_stmt.body, source, tw_config, vars, colors, seen);
        }
        Stmt::DoWhile(do_while_stmt) => {
            collect_foregrounds_stmt(&do_while_stmt.body, source, tw_config, vars, colors, seen);
            collect_foregrounds_expr(&do_while_stmt.test, source, tw_config, vars, colors, seen);
        }
        Stmt::Switch(switch_stmt) => {
            collect_foregrounds_expr(&switch_stmt.discriminant, source, tw_config, vars, colors, seen);
            for case in &switch_stmt.cases {
                if let Some(test) = &case.test {
                    collect_foregrounds_expr(test, source, tw_config, vars, colors, seen);
                }
                for stmt in &case.cons {
                    collect_foregrounds_stmt(stmt, source, tw_config, vars, colors, seen);
                }
            }
        }
        Stmt::Try(try_stmt) => {
            collect_foregrounds_block(&try_stmt.block, source, tw_config, vars, colors, seen);
            if let Some(handler) = &try_stmt.handler {
                collect_foregrounds_block(&handler.body, source, tw_config, vars, colors, seen);
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                collect_foregrounds_block(finalizer, source, tw_config, vars, colors, seen);
            }
        }
        Stmt::Decl(Decl::Fn(f)) => {
            if let Some(body) = &f.function.body {
                collect_foregrounds_block(body, source, tw_config, vars, colors, seen);
            }
        }
        Stmt::Decl(Decl::Var(var_decl)) => {
            for decl in &var_decl.decls {
                if let Some(init) = &decl.init {
                    collect_foregrounds_expr(init, source, tw_config, vars, colors, seen);
                }
            }
        }
        _ => {}
    }
}

fn collect_foregrounds_var_decl_or_expr(
    init: &VarDeclOrExpr,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    colors: &mut Vec<(String, Rgba)>,
    seen: &mut HashSet<String>,
) {
    match init {
        VarDeclOrExpr::VarDecl(var_decl) => {
            for decl in &var_decl.decls {
                if let Some(init) = &decl.init {
                    collect_foregrounds_expr(init, source, tw_config, vars, colors, seen);
                }
            }
        }
        VarDeclOrExpr::Expr(expr) => {
            collect_foregrounds_expr(expr, source, tw_config, vars, colors, seen);
        }
    }
}

fn collect_foregrounds_for_head(
    head: &ForHead,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    colors: &mut Vec<(String, Rgba)>,
    seen: &mut HashSet<String>,
) {
    if let ForHead::VarDecl(var_decl) = head {
        for decl in &var_decl.decls {
            if let Some(init) = &decl.init {
                collect_foregrounds_expr(init, source, tw_config, vars, colors, seen);
            }
        }
    }
}

fn collect_foregrounds_expr(
    expr: &Expr,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    colors: &mut Vec<(String, Rgba)>,
    seen: &mut HashSet<String>,
) {
    match expr {
        Expr::JSXElement(el) => {
            collect_foregrounds_jsx_element(el, source, tw_config, vars, colors, seen);
        }
        Expr::JSXFragment(frag) => {
            for child in &frag.children {
                collect_foregrounds_jsx_child(child, source, tw_config, vars, colors, seen);
            }
        }
        Expr::Paren(p) => collect_foregrounds_expr(&p.expr, source, tw_config, vars, colors, seen),
        Expr::Arrow(arrow) => match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(block) => {
                collect_foregrounds_block(block, source, tw_config, vars, colors, seen)
            }
            BlockStmtOrExpr::Expr(expr) => {
                collect_foregrounds_expr(expr, source, tw_config, vars, colors, seen)
            }
        },
        Expr::Call(call) => {
            if let Callee::Expr(callee) = &call.callee {
                collect_foregrounds_expr(callee, source, tw_config, vars, colors, seen);
            }
            for arg in &call.args {
                collect_foregrounds_expr(&arg.expr, source, tw_config, vars, colors, seen);
            }
        }
        Expr::Cond(cond) => {
            collect_foregrounds_expr(&cond.cons, source, tw_config, vars, colors, seen);
            collect_foregrounds_expr(&cond.alt, source, tw_config, vars, colors, seen);
        }
        Expr::Bin(bin) => {
            collect_foregrounds_expr(&bin.left, source, tw_config, vars, colors, seen);
            collect_foregrounds_expr(&bin.right, source, tw_config, vars, colors, seen);
        }
        _ => {}
    }
}

fn collect_foregrounds_jsx_child(
    child: &JSXElementChild,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    colors: &mut Vec<(String, Rgba)>,
    seen: &mut HashSet<String>,
) {
    match child {
        JSXElementChild::JSXElement(el) => {
            collect_foregrounds_jsx_element(el, source, tw_config, vars, colors, seen);
        }
        JSXElementChild::JSXFragment(frag) => {
            for child in &frag.children {
                collect_foregrounds_jsx_child(child, source, tw_config, vars, colors, seen);
            }
        }
        JSXElementChild::JSXExprContainer(container) => {
            if let JSXExpr::Expr(expr) = &container.expr {
                collect_foregrounds_expr(expr, source, tw_config, vars, colors, seen);
            }
        }
        _ => {}
    }
}

fn collect_foregrounds_jsx_element(
    element: &JSXElement,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    colors: &mut Vec<(String, Rgba)>,
    seen: &mut HashSet<String>,
) {
    let mut elem_colors = ElementColors::default();

    for attr in &element.opening.attrs {
        if let JSXAttrOrSpread::JSXAttr(jsx_attr) = attr {
            let attr_name = jsx_attr_name(&jsx_attr.name);
            match attr_name.as_str() {
                "className" | "class" => {
                    if let Some(value) = &jsx_attr.value {
                        let class_str = extract_jsx_attr_string(value, source);
                        extract_colors_from_classes(&class_str, tw_config, vars, &mut elem_colors);
                    }
                }
                "style" => {
                    if let Some(value) = &jsx_attr.value {
                        let style_str = extract_jsx_attr_raw(value, source);
                        extract_colors_from_inline_style(&style_str, vars, &mut elem_colors);
                    }
                }
                _ => {}
            }
        }
    }

    for (name, rgba) in elem_colors.foregrounds {
        if seen.insert(name.clone()) {
            colors.push((name, rgba));
        }
    }

    for child in &element.children {
        collect_foregrounds_jsx_child(child, source, tw_config, vars, colors, seen);
    }
}

fn walk_expr(
    expr: &Expr,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    match expr {
        Expr::JSXElement(el) => {
            process_jsx_element(el, source, tw_config, vars, file, pairs, inherited_bg);
        }
        Expr::JSXFragment(frag) => {
            for child in &frag.children {
                walk_jsx_child(child, source, tw_config, vars, file, pairs, inherited_bg);
            }
        }
        Expr::Paren(p) => {
            walk_expr(&p.expr, source, tw_config, vars, file, pairs, inherited_bg)
        }
        Expr::Arrow(arrow) => match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(block) => {
                walk_block_stmt(block, source, tw_config, vars, file, pairs, inherited_bg)
            }
            BlockStmtOrExpr::Expr(expr) => {
                walk_expr(expr, source, tw_config, vars, file, pairs, inherited_bg)
            }
        },
        Expr::Call(call) => {
            if let Callee::Expr(callee) = &call.callee {
                walk_expr(callee, source, tw_config, vars, file, pairs, inherited_bg);
            }
            for arg in &call.args {
                walk_expr(&arg.expr, source, tw_config, vars, file, pairs, inherited_bg);
            }
        }
        Expr::Cond(cond) => {
            walk_expr(&cond.cons, source, tw_config, vars, file, pairs, inherited_bg);
            walk_expr(&cond.alt, source, tw_config, vars, file, pairs, inherited_bg);
        }
        Expr::Bin(bin) => {
            walk_expr(&bin.left, source, tw_config, vars, file, pairs, inherited_bg);
            walk_expr(&bin.right, source, tw_config, vars, file, pairs, inherited_bg);
        }
        _ => {}
    }
}

fn walk_jsx_child(
    child: &JSXElementChild,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    match child {
        JSXElementChild::JSXElement(el) => {
            process_jsx_element(el, source, tw_config, vars, file, pairs, inherited_bg);
        }
        JSXElementChild::JSXFragment(frag) => {
            for child in &frag.children {
                walk_jsx_child(child, source, tw_config, vars, file, pairs, inherited_bg);
            }
        }
        JSXElementChild::JSXExprContainer(container) => {
            if let JSXExpr::Expr(expr) = &container.expr {
                walk_expr(expr, source, tw_config, vars, file, pairs, inherited_bg);
            }
        }
        _ => {}
    }
}

fn process_jsx_element(
    element: &JSXElement,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    inherited_bg: &[(String, Rgba)],
) {
    let element_name = jsx_element_name(&element.opening.name);
    let line = byte_offset_to_line(source, element.opening.span.lo.0 as usize);

    let mut elem_colors = ElementColors {
        line,
        element_name: element_name.clone(),
        ..Default::default()
    };

    // Extract colors from attributes
    for attr in &element.opening.attrs {
        if let JSXAttrOrSpread::JSXAttr(jsx_attr) = attr {
            let attr_name = jsx_attr_name(&jsx_attr.name);
            match attr_name.as_str() {
                "className" | "class" => {
                    if let Some(value) = &jsx_attr.value {
                        let class_str = extract_jsx_attr_string(value, source);
                        extract_colors_from_classes(
                            &class_str,
                            tw_config,
                            vars,
                            &mut elem_colors,
                        );
                    }
                }
                "style" => {
                    if let Some(value) = &jsx_attr.value {
                        let style_str = extract_jsx_attr_raw(value, source);
                        extract_colors_from_inline_style(&style_str, vars, &mut elem_colors);
                    }
                }
                _ => {}
            }
        }
    }

    // Determine the effective backgrounds for this element and its children.
    // If this element defines its own backgrounds, those take precedence.
    // Otherwise, inherit from the parent.
    let effective_bg: Vec<(String, Rgba)> = if !elem_colors.backgrounds.is_empty() {
        elem_colors.backgrounds.clone()
    } else {
        inherited_bg.to_vec()
    };

    // Generate pairs: foregrounds on this element vs. effective backgrounds
    // (own backgrounds if present, otherwise inherited from parent)
    let bg_source = if !elem_colors.backgrounds.is_empty() {
        &elem_colors.backgrounds
    } else {
        inherited_bg
    };

    for (fg_name, fg_color) in &elem_colors.foregrounds {
        for (bg_name, bg_color) in bg_source {
            let ratio = contrast_ratio(fg_color, bg_color);
            let level = ConformanceLevel::from_ratio(ratio);
            pairs.push(ColorPair {
                foreground_name: fg_name.clone(),
                foreground_color: *fg_color,
                background_name: bg_name.clone(),
                background_color: *bg_color,
                ratio,
                level,
                file: file.to_string(),
                line: elem_colors.line,
                element: elem_colors.element_name.clone(),
            });
        }
    }

    // Recurse into children with the effective background context
    for child in &element.children {
        walk_jsx_child(child, source, tw_config, vars, file, pairs, &effective_bg);
    }
}

fn extract_colors_from_classes(
    class_str: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    elem: &mut ElementColors,
) {
    for class in class_str.split_whitespace() {
        // Strip dark: or responsive prefixes
        let base_class = strip_variant_prefixes(class);

        if let Some((kind, color_name)) = parse_utility_class(base_class) {
            if let Some(rgba) = resolve_tailwind_color(&color_name, tw_config, vars) {
                let display_name = base_class.to_string();
                if kind.is_foreground() {
                    elem.foregrounds.push((display_name, rgba));
                } else if kind.is_background() {
                    elem.backgrounds.push((display_name, rgba));
                }
            }
        }
    }
}

fn extract_colors_from_inline_style(
    style_str: &str,
    vars: &HashMap<String, Rgba>,
    elem: &mut ElementColors,
) {
    let inline = extract_inline_colors(style_str);

    let resolve = |value: &str| -> Option<Rgba> {
        use crate::color::parser::{ColorParseResult, parse_color};
        match parse_color(value) {
            ColorParseResult::Resolved(rgba) => Some(rgba),
            ColorParseResult::VarReference(name) => vars.get(&name).copied(),
            ColorParseResult::WrappedVarReference { var_name, .. } => {
                vars.get(&var_name).copied()
            }
            ColorParseResult::Unresolvable(_) => None,
        }
    };

    if let Some(ref val) = inline.color {
        if let Some(rgba) = resolve(val) {
            elem.foregrounds.push((format!("style.color:{val}"), rgba));
        }
    }
    if let Some(ref val) = inline.background_color {
        if let Some(rgba) = resolve(val) {
            elem.backgrounds.push((format!("style.bg:{val}"), rgba));
        }
    }
    if let Some(ref val) = inline.fill {
        if let Some(rgba) = resolve(val) {
            elem.foregrounds.push((format!("style.fill:{val}"), rgba));
        }
    }
}

/// Convert a SWC Wtf8Atom to a String (WTF-8 -> UTF-8 lossy conversion).
fn wtf8_to_string(atom: &swc_ecma_ast::Str) -> String {
    atom.value
        .as_str()
        .map(|s| s.to_owned())
        .unwrap_or_else(|| atom.value.to_string_lossy().into_owned())
}

/// Convert an Atom (Ident.sym) to String — Atom derefs to str.
fn atom_to_string(atom: &swc_atoms::Atom) -> String {
    atom.as_str().to_owned()
}

/// Strip Tailwind variant prefixes (e.g. "dark:", "hover:", "sm:", "focus:").
fn strip_variant_prefixes(class: &str) -> &str {
    match class.rfind(':') {
        Some(idx) => &class[idx + 1..],
        None => class,
    }
}

fn jsx_element_name(name: &JSXElementName) -> String {
    match name {
        JSXElementName::Ident(ident) => atom_to_string(&ident.sym),
        JSXElementName::JSXMemberExpr(member) => {
            format!(
                "{}.{}",
                jsx_object_name(&member.obj),
                atom_to_string(&member.prop.sym)
            )
        }
        JSXElementName::JSXNamespacedName(ns) => {
            format!(
                "{}:{}",
                atom_to_string(&ns.ns.sym),
                atom_to_string(&ns.name.sym)
            )
        }
    }
}

fn jsx_object_name(obj: &JSXObject) -> String {
    match obj {
        JSXObject::Ident(ident) => atom_to_string(&ident.sym),
        JSXObject::JSXMemberExpr(member) => {
            format!(
                "{}.{}",
                jsx_object_name(&member.obj),
                atom_to_string(&member.prop.sym)
            )
        }
    }
}

fn jsx_attr_name(name: &JSXAttrName) -> String {
    match name {
        JSXAttrName::Ident(ident) => atom_to_string(&ident.sym),
        JSXAttrName::JSXNamespacedName(ns) => {
            format!(
                "{}:{}",
                atom_to_string(&ns.ns.sym),
                atom_to_string(&ns.name.sym)
            )
        }
    }
}

/// Extract a string value from a JSX attribute value.
fn extract_jsx_attr_string(value: &JSXAttrValue, _source: &str) -> String {
    match value {
        JSXAttrValue::Str(s) => wtf8_to_string(s),
        JSXAttrValue::JSXExprContainer(container) => {
            extract_strings_from_jsx_expr(&container.expr)
        }
        _ => String::new(),
    }
}

/// Extract raw source text from a JSX attribute value (for style objects).
fn extract_jsx_attr_raw(value: &JSXAttrValue, source: &str) -> String {
    match value {
        JSXAttrValue::JSXExprContainer(container) => {
            let span = match &container.expr {
                JSXExpr::Expr(expr) => expr.span(),
                JSXExpr::JSXEmptyExpr(empty) => empty.span,
            };
            let lo = span.lo.0 as usize;
            let hi = span.hi.0 as usize;
            if lo > 0 && hi > lo {
                safe_byte_slice(source, lo - 1, hi - 1)
            } else {
                String::new()
            }
        }
        _ => String::new(),
    }
}

/// Extract string literals from a JSX expression (handles cn(), clsx(), template literals).
fn extract_strings_from_jsx_expr(expr: &JSXExpr) -> String {
    match expr {
        JSXExpr::Expr(e) => extract_strings_from_expr(e),
        _ => String::new(),
    }
}

/// Extract all string literals reachable from an expression.
/// Handles cn(), clsx(), template literals, ternaries, string concatenation.
pub fn extract_strings_from_expr(expr: &Expr) -> String {
    match expr {
        Expr::Lit(Lit::Str(s)) => wtf8_to_string(s),
        Expr::Tpl(tpl) => {
            tpl.quasis
                .iter()
                .filter_map(|q| {
                    q.cooked.as_ref().map(|s| {
                        s.as_str()
                            .map(|s| s.to_owned())
                            .unwrap_or_else(|| s.to_string_lossy().into_owned())
                    })
                })
                .collect::<Vec<_>>()
                .join(" ")
        }
        Expr::Call(call) => {
            let mut parts = Vec::new();
            for arg in &call.args {
                let s = extract_strings_from_expr(&arg.expr);
                if !s.is_empty() {
                    parts.push(s);
                }
            }
            parts.join(" ")
        }
        Expr::Bin(bin) if matches!(bin.op, BinaryOp::Add) => {
            let left = extract_strings_from_expr(&bin.left);
            let right = extract_strings_from_expr(&bin.right);
            format!("{left} {right}")
        }
        Expr::Cond(cond) => {
            let cons = extract_strings_from_expr(&cond.cons);
            let alt = extract_strings_from_expr(&cond.alt);
            format!("{cons} {alt}")
        }
        Expr::Paren(p) => extract_strings_from_expr(&p.expr),
        _ => String::new(),
    }
}

pub(crate) fn byte_offset_to_line(source: &str, offset: usize) -> usize {
    let actual_offset = if offset > 0 { offset - 1 } else { 0 };
    source
        .as_bytes()
        .iter()
        .take(actual_offset.min(source.len()))
        .filter(|b| **b == b'\n')
        .count()
        + 1
}

pub(crate) fn safe_byte_slice(source: &str, start: usize, end: usize) -> String {
    if start >= end || start >= source.len() {
        return String::new();
    }

    let end = end.min(source.len());
    if let Some(slice) = source.get(start..end) {
        return slice.to_string();
    }

    let start = ceil_char_boundary(source, start);
    let end = floor_char_boundary(source, end);
    if start >= end {
        return String::new();
    }

    source.get(start..end).unwrap_or_default().to_string()
}

fn ceil_char_boundary(source: &str, index: usize) -> usize {
    let mut index = index.min(source.len());
    while index < source.len() && !source.is_char_boundary(index) {
        index += 1;
    }
    index
}

fn floor_char_boundary(source: &str, index: usize) -> usize {
    let mut index = index.min(source.len());
    while index > 0 && !source.is_char_boundary(index) {
        index -= 1;
    }
    index
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn parse_test_module(source: &str) -> Module {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("component.tsx");
        fs::write(&path, source).unwrap();
        parse_tsx_module(source, &path).unwrap()
    }

    #[test]
    fn test_strip_variant_prefixes() {
        assert_eq!(strip_variant_prefixes("dark:bg-primary"), "bg-primary");
        assert_eq!(strip_variant_prefixes("hover:text-white"), "text-white");
        assert_eq!(strip_variant_prefixes("sm:md:bg-red-500"), "bg-red-500");
        assert_eq!(strip_variant_prefixes("bg-primary"), "bg-primary");
    }

    #[test]
    fn test_byte_offset_to_line() {
        let source = "line1\nline2\nline3";
        assert_eq!(byte_offset_to_line(source, 1), 1);
        assert_eq!(byte_offset_to_line(source, 7), 2);
        assert_eq!(byte_offset_to_line(source, 13), 3);
    }

    #[test]
    fn test_byte_offset_to_line_mid_utf8_does_not_panic() {
        let source = "é\nline2";
        assert_eq!(byte_offset_to_line(source, 2), 1);
        assert_eq!(byte_offset_to_line(source, 3), 1);
        assert_eq!(byte_offset_to_line(source, 4), 2);
    }

    #[test]
    fn test_safe_byte_slice_mid_utf8_does_not_panic() {
        let source = "aéb";
        assert_eq!(safe_byte_slice(source, 1, 4), "éb");
        assert_eq!(safe_byte_slice(source, 2, 3), "");
    }

    #[test]
    fn test_walk_stmt_traverses_loop_switch_and_try() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("component.tsx");
        let source = r#"
            export function Demo() {
                for (let i = 0; i < 1; i++) {
                    const a = <div className="bg-white text-black" />;
                }
                switch (1) {
                    case 1:
                        const b = <div className="bg-white text-black" />;
                        break;
                    default:
                        break;
                }
                try {
                    return <div className="bg-white text-black" />;
                } catch (e) {
                    return null;
                }
            }
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Light,
        );

        assert_eq!(pairs.len(), 3);
    }

    #[test]
    fn test_collect_foregrounds_traverses_loop_switch_and_try() {
        let source = r#"
            export function Demo() {
                while (false) {
                    const a = <span className="text-black" />;
                }
                switch (1) {
                    case 1:
                        const b = <span className="text-white" />;
                        break;
                    default:
                        break;
                }
                try {
                    return <span className="text-red-500" />;
                } catch (e) {
                    return null;
                }
            }
        "#;

        let module = parse_test_module(source);
        let body = match &module.body[0] {
            ModuleItem::ModuleDecl(ModuleDecl::ExportDecl(export)) => match &export.decl {
                Decl::Fn(f) => f.function.body.as_ref().unwrap(),
                _ => panic!("expected function export"),
            },
            _ => panic!("expected export declaration"),
        };

        let colors = collect_foreground_colors_from_block(
            body,
            source,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default().light,
        );

        let names: Vec<&str> = colors.iter().map(|(name, _)| name.as_str()).collect();
        assert!(names.contains(&"text-black"));
        assert!(names.contains(&"text-white"));
        assert!(names.contains(&"text-red-500"));
    }

    #[test]
    fn test_dedup_color_pairs_by_reporting_tuple() {
        let pairs = vec![
            ColorPair {
                foreground_name: "text-red-500".to_string(),
                foreground_color: Rgba::opaque(239, 68, 68),
                background_name: "bg-white".to_string(),
                background_color: Rgba::opaque(255, 255, 255),
                ratio: 4.0,
                level: ConformanceLevel::AaLargeOnly,
                file: "page.tsx".to_string(),
                line: 10,
                element: "Sidebar".to_string(),
            },
            ColorPair {
                foreground_name: "text-red-500".to_string(),
                foreground_color: Rgba::opaque(239, 68, 68),
                background_name: "bg-white".to_string(),
                background_color: Rgba::opaque(255, 255, 255),
                ratio: 4.0,
                level: ConformanceLevel::AaLargeOnly,
                file: "page.tsx".to_string(),
                line: 10,
                element: "Sidebar".to_string(),
            },
            ColorPair {
                foreground_name: "text-red-500".to_string(),
                foreground_color: Rgba::opaque(239, 68, 68),
                background_name: "bg-blue-500".to_string(),
                background_color: Rgba::opaque(59, 130, 246),
                ratio: 3.0,
                level: ConformanceLevel::AaLargeOnly,
                file: "page.tsx".to_string(),
                line: 10,
                element: "Sidebar".to_string(),
            },
            ColorPair {
                foreground_name: "text-red-500".to_string(),
                foreground_color: Rgba::opaque(239, 68, 68),
                background_name: "bg-white".to_string(),
                background_color: Rgba::opaque(255, 255, 255),
                ratio: 4.0,
                level: ConformanceLevel::AaLargeOnly,
                file: "page.tsx".to_string(),
                line: 10,
                element: "Button".to_string(),
            },
        ];

        let deduped = dedup_color_pairs(pairs);
        assert_eq!(deduped.len(), 3);
        assert_eq!(deduped[0].background_name, "bg-white");
        assert_eq!(deduped[1].background_name, "bg-blue-500");
        assert_eq!(deduped[2].element, "Button");
    }
}

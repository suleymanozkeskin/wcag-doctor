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

/// Background name used when a foreground has no surface anywhere in scope: the
/// text renders directly on the page backdrop. Emitted with a fully transparent
/// background so backdrop resolution composites it to the real backdrop color.
/// Dropped by the caller when no backdrop is configured, since the contrast is
/// then unknowable rather than failing.
pub const BACKDROP_BACKGROUND: &str = "<backdrop>";

/// Background name marking "a surface component provides the background here".
/// Its color is unknown at the usage site (the component carries its own classes),
/// so pairs against it are not emitted — but its presence correctly prevents
/// descendants from being reported as sitting on the backdrop.
const SURFACE_BACKGROUND: &str = "<surface>";

/// Components that render their own surface. Their usage sites carry no `bg-*`
/// utility, so without this the scanner would treat everything inside a `<Card>`
/// or dialog as having no background at all.
const SURFACE_COMPONENTS: &[&str] = &[
    "Card",
    "DialogContent",
    "AlertDialogContent",
    "SheetContent",
    "DrawerContent",
    "PopoverContent",
    "DropdownMenuContent",
    "ContextMenuContent",
    "MenubarContent",
    "SelectContent",
    "HoverCardContent",
    "TooltipContent",
    "CommandDialog",
];

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
    pub theme: String,
    /// Interaction state this pair applies in ("base", "hover", "focus", …).
    pub state: String,
}

/// Colors found on a single JSX element.
#[derive(Debug, Default)]
struct ElementColors {
    foregrounds: Vec<(String, Rgba)>,
    backgrounds: Vec<(String, Rgba)>,
    /// Per-interaction-state overrides layered on top of the base colors.
    state_layers: Vec<StateLayer>,
    line: usize,
    element_name: String,
}

/// The foreground/background utilities a single interaction state contributes,
/// after resolving the theme-specific override within that state.
#[derive(Debug)]
struct StateLayer {
    state: InteractionState,
    foregrounds: Vec<(String, Rgba)>,
    backgrounds: Vec<(String, Rgba)>,
}

/// Base + theme-override buckets collected for one interaction state.
#[derive(Default)]
struct StateBuckets {
    base_fgs: Vec<(String, Rgba)>,
    base_bgs: Vec<(String, Rgba)>,
    override_fgs: Vec<(String, Rgba)>,
    override_bgs: Vec<(String, Rgba)>,
}

impl StateBuckets {
    /// Resolve to effective (foreground, background) lists: theme-specific classes
    /// win over base classes (higher CSS specificity), matching `.dark &`.
    fn into_effective(self) -> (Vec<(String, Rgba)>, Vec<(String, Rgba)>) {
        let fgs = if self.override_fgs.is_empty() {
            self.base_fgs
        } else {
            self.override_fgs
        };
        let bgs = if self.override_bgs.is_empty() {
            self.base_bgs
        } else {
            self.override_bgs
        };
        (fgs, bgs)
    }
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
        theme,
    );

    pairs
}

#[derive(Debug, Clone, Copy)]
pub enum Theme {
    Light,
    Dark,
}

impl Theme {
    pub fn label(&self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
        }
    }
}

/// Theme context derived from Tailwind variant prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeContext {
    /// No dark:/light: prefix — applies in both themes.
    Base,
    /// Has dark: prefix — applies only in dark theme.
    DarkOnly,
    /// Has light: prefix — applies only in light theme.
    LightOnly,
}

/// Interaction state derived from Tailwind variant prefixes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum InteractionState {
    /// No interaction prefix — the resting state.
    Base,
    /// `hover:` / `group-hover:`
    Hover,
    /// `focus:` / `focus-within:`
    Focus,
    /// `focus-visible:`
    FocusVisible,
}

impl InteractionState {
    pub fn label(&self) -> &'static str {
        match self {
            InteractionState::Base => "base",
            InteractionState::Hover => "hover",
            InteractionState::Focus => "focus",
            InteractionState::FocusVisible => "focus-visible",
        }
    }
}

/// Parse variant prefixes and determine theme context.
/// Returns (theme_context, base_class_without_prefixes).
pub fn parse_variant_context(class: &str) -> (ThemeContext, &str) {
    let (theme, _state, base) = parse_variant(class);
    (theme, base)
}

/// The interaction state a single utility class applies in.
pub fn interaction_state_of(class: &str) -> InteractionState {
    parse_variant(class).1
}

/// Parse variant prefixes into theme context, interaction state, and the base
/// utility class. E.g. `dark:hover:bg-accent` → (DarkOnly, Hover, "bg-accent").
pub fn parse_variant(class: &str) -> (ThemeContext, InteractionState, &str) {
    let base = strip_variant_prefixes(class);
    if base.len() == class.len() {
        return (ThemeContext::Base, InteractionState::Base, base);
    }
    // The prefix portion is everything before the base class.
    // E.g., for "sm:dark:hover:bg-red-500" → prefix = "sm:dark:hover:"
    let prefix = &class[..class.len() - base.len()];

    let theme = if prefix.split(':').any(|p| p == "dark") {
        ThemeContext::DarkOnly
    } else if prefix.split(':').any(|p| p == "light") {
        ThemeContext::LightOnly
    } else {
        ThemeContext::Base
    };

    (theme, interaction_state_from_prefix(prefix), base)
}

/// Map the first recognized interaction segment in a variant prefix to a state.
fn interaction_state_from_prefix(prefix: &str) -> InteractionState {
    for segment in prefix.split(':') {
        match segment {
            "focus-visible" => return InteractionState::FocusVisible,
            "focus" | "focus-within" => return InteractionState::Focus,
            "hover" | "group-hover" => return InteractionState::Hover,
            _ => {}
        }
    }
    InteractionState::Base
}

/// Deduplicate component findings while preserving their original order.
/// Uses the reporting identity tuple `(file, line, element, foreground, background, theme)`.
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
            pair.theme.clone(),
            pair.state.clone(),
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
    theme: Theme,
) -> Vec<(String, Rgba)> {
    let mut colors = Vec::new();
    let mut seen = HashSet::new();
    collect_foregrounds_block(block, source, tw_config, vars, &mut colors, &mut seen, theme);
    colors
}

/// Collect only foreground colors from an expression body.
pub fn collect_foreground_colors_from_expr(
    expr: &Expr,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    theme: Theme,
) -> Vec<(String, Rgba)> {
    let mut colors = Vec::new();
    let mut seen = HashSet::new();
    collect_foregrounds_expr(expr, source, tw_config, vars, &mut colors, &mut seen, theme);
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
    theme: Theme,
) {
    for item in &module.body {
        walk_module_item(item, source, tw_config, vars, file, pairs, inherited_bg, theme);
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
    theme: Theme,
) {
    match item {
        ModuleItem::Stmt(stmt) => {
            walk_stmt(stmt, source, tw_config, vars, file, pairs, inherited_bg, theme)
        }
        ModuleItem::ModuleDecl(decl) => {
            walk_module_decl(decl, source, tw_config, vars, file, pairs, inherited_bg, theme)
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
    theme: Theme,
) {
    match decl {
        ModuleDecl::ExportDecl(export) => {
            walk_decl(&export.decl, source, tw_config, vars, file, pairs, inherited_bg, theme)
        }
        ModuleDecl::ExportDefaultDecl(export) => {
            if let DefaultDecl::Fn(f) = &export.decl {
                if let Some(body) = &f.function.body {
                    walk_block_stmt(body, source, tw_config, vars, file, pairs, inherited_bg, theme);
                }
            }
        }
        ModuleDecl::ExportDefaultExpr(export) => {
            walk_expr(&export.expr, source, tw_config, vars, file, pairs, inherited_bg, theme);
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
    theme: Theme,
) {
    match decl {
        Decl::Fn(f) => {
            if let Some(body) = &f.function.body {
                walk_block_stmt(body, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
        }
        Decl::Var(var_decl) => {
            for decl in &var_decl.decls {
                if let Some(init) = &decl.init {
                    walk_expr(init, source, tw_config, vars, file, pairs, inherited_bg, theme);
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
    theme: Theme,
) {
    match stmt {
        Stmt::Block(block) => {
            walk_block_stmt(block, source, tw_config, vars, file, pairs, inherited_bg, theme)
        }
        Stmt::Return(ret) => {
            if let Some(arg) = &ret.arg {
                walk_expr(arg, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
        }
        Stmt::Expr(expr) => {
            walk_expr(&expr.expr, source, tw_config, vars, file, pairs, inherited_bg, theme)
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
                theme,
            );
            if let Some(alt) = &if_stmt.alt {
                walk_stmt(alt, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
        }
        Stmt::For(for_stmt) => {
            if let Some(init) = &for_stmt.init {
                walk_var_decl_or_expr(init, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
            if let Some(test) = &for_stmt.test {
                walk_expr(test, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
            if let Some(update) = &for_stmt.update {
                walk_expr(update, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
            walk_stmt(&for_stmt.body, source, tw_config, vars, file, pairs, inherited_bg, theme);
        }
        Stmt::ForIn(for_in_stmt) => {
            walk_var_decl_or_pat(&for_in_stmt.left, source, tw_config, vars, file, pairs, inherited_bg, theme);
            walk_expr(&for_in_stmt.right, source, tw_config, vars, file, pairs, inherited_bg, theme);
            walk_stmt(&for_in_stmt.body, source, tw_config, vars, file, pairs, inherited_bg, theme);
        }
        Stmt::ForOf(for_of_stmt) => {
            walk_var_decl_or_pat(&for_of_stmt.left, source, tw_config, vars, file, pairs, inherited_bg, theme);
            walk_expr(&for_of_stmt.right, source, tw_config, vars, file, pairs, inherited_bg, theme);
            walk_stmt(&for_of_stmt.body, source, tw_config, vars, file, pairs, inherited_bg, theme);
        }
        Stmt::While(while_stmt) => {
            walk_expr(&while_stmt.test, source, tw_config, vars, file, pairs, inherited_bg, theme);
            walk_stmt(&while_stmt.body, source, tw_config, vars, file, pairs, inherited_bg, theme);
        }
        Stmt::DoWhile(do_while_stmt) => {
            walk_stmt(&do_while_stmt.body, source, tw_config, vars, file, pairs, inherited_bg, theme);
            walk_expr(&do_while_stmt.test, source, tw_config, vars, file, pairs, inherited_bg, theme);
        }
        Stmt::Switch(switch_stmt) => {
            walk_expr(&switch_stmt.discriminant, source, tw_config, vars, file, pairs, inherited_bg, theme);
            for case in &switch_stmt.cases {
                if let Some(test) = &case.test {
                    walk_expr(test, source, tw_config, vars, file, pairs, inherited_bg, theme);
                }
                for stmt in &case.cons {
                    walk_stmt(stmt, source, tw_config, vars, file, pairs, inherited_bg, theme);
                }
            }
        }
        Stmt::Try(try_stmt) => {
            walk_block_stmt(&try_stmt.block, source, tw_config, vars, file, pairs, inherited_bg, theme);
            if let Some(handler) = &try_stmt.handler {
                walk_block_stmt(&handler.body, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                walk_block_stmt(finalizer, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
        }
        Stmt::Decl(decl) => walk_decl(decl, source, tw_config, vars, file, pairs, inherited_bg, theme),
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
    theme: Theme,
) {
    match init {
        VarDeclOrExpr::VarDecl(var_decl) => {
            walk_decl(&Decl::Var(var_decl.clone()), source, tw_config, vars, file, pairs, inherited_bg, theme);
        }
        VarDeclOrExpr::Expr(expr) => {
            walk_expr(expr, source, tw_config, vars, file, pairs, inherited_bg, theme);
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
    theme: Theme,
) {
    match left {
        ForHead::VarDecl(var_decl) => {
            walk_decl(&Decl::Var(var_decl.clone()), source, tw_config, vars, file, pairs, inherited_bg, theme);
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
    theme: Theme,
) {
    for stmt in &block.stmts {
        walk_stmt(stmt, source, tw_config, vars, file, pairs, inherited_bg, theme);
    }
}

fn collect_foregrounds_block(
    block: &BlockStmt,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    colors: &mut Vec<(String, Rgba)>,
    seen: &mut HashSet<String>,
    theme: Theme,
) {
    for stmt in &block.stmts {
        collect_foregrounds_stmt(stmt, source, tw_config, vars, colors, seen, theme);
    }
}

fn collect_foregrounds_stmt(
    stmt: &Stmt,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    colors: &mut Vec<(String, Rgba)>,
    seen: &mut HashSet<String>,
    theme: Theme,
) {
    match stmt {
        Stmt::Block(block) => collect_foregrounds_block(block, source, tw_config, vars, colors, seen, theme),
        Stmt::Return(ret) => {
            if let Some(arg) = &ret.arg {
                collect_foregrounds_expr(arg, source, tw_config, vars, colors, seen, theme);
            }
        }
        Stmt::Expr(expr) => {
            collect_foregrounds_expr(&expr.expr, source, tw_config, vars, colors, seen, theme)
        }
        Stmt::If(if_stmt) => {
            collect_foregrounds_stmt(&if_stmt.cons, source, tw_config, vars, colors, seen, theme);
            if let Some(alt) = &if_stmt.alt {
                collect_foregrounds_stmt(alt, source, tw_config, vars, colors, seen, theme);
            }
        }
        Stmt::For(for_stmt) => {
            if let Some(init) = &for_stmt.init {
                collect_foregrounds_var_decl_or_expr(init, source, tw_config, vars, colors, seen, theme);
            }
            if let Some(test) = &for_stmt.test {
                collect_foregrounds_expr(test, source, tw_config, vars, colors, seen, theme);
            }
            if let Some(update) = &for_stmt.update {
                collect_foregrounds_expr(update, source, tw_config, vars, colors, seen, theme);
            }
            collect_foregrounds_stmt(&for_stmt.body, source, tw_config, vars, colors, seen, theme);
        }
        Stmt::ForIn(for_in_stmt) => {
            collect_foregrounds_for_head(&for_in_stmt.left, source, tw_config, vars, colors, seen, theme);
            collect_foregrounds_expr(&for_in_stmt.right, source, tw_config, vars, colors, seen, theme);
            collect_foregrounds_stmt(&for_in_stmt.body, source, tw_config, vars, colors, seen, theme);
        }
        Stmt::ForOf(for_of_stmt) => {
            collect_foregrounds_for_head(&for_of_stmt.left, source, tw_config, vars, colors, seen, theme);
            collect_foregrounds_expr(&for_of_stmt.right, source, tw_config, vars, colors, seen, theme);
            collect_foregrounds_stmt(&for_of_stmt.body, source, tw_config, vars, colors, seen, theme);
        }
        Stmt::While(while_stmt) => {
            collect_foregrounds_expr(&while_stmt.test, source, tw_config, vars, colors, seen, theme);
            collect_foregrounds_stmt(&while_stmt.body, source, tw_config, vars, colors, seen, theme);
        }
        Stmt::DoWhile(do_while_stmt) => {
            collect_foregrounds_stmt(&do_while_stmt.body, source, tw_config, vars, colors, seen, theme);
            collect_foregrounds_expr(&do_while_stmt.test, source, tw_config, vars, colors, seen, theme);
        }
        Stmt::Switch(switch_stmt) => {
            collect_foregrounds_expr(&switch_stmt.discriminant, source, tw_config, vars, colors, seen, theme);
            for case in &switch_stmt.cases {
                if let Some(test) = &case.test {
                    collect_foregrounds_expr(test, source, tw_config, vars, colors, seen, theme);
                }
                for stmt in &case.cons {
                    collect_foregrounds_stmt(stmt, source, tw_config, vars, colors, seen, theme);
                }
            }
        }
        Stmt::Try(try_stmt) => {
            collect_foregrounds_block(&try_stmt.block, source, tw_config, vars, colors, seen, theme);
            if let Some(handler) = &try_stmt.handler {
                collect_foregrounds_block(&handler.body, source, tw_config, vars, colors, seen, theme);
            }
            if let Some(finalizer) = &try_stmt.finalizer {
                collect_foregrounds_block(finalizer, source, tw_config, vars, colors, seen, theme);
            }
        }
        Stmt::Decl(Decl::Fn(f)) => {
            if let Some(body) = &f.function.body {
                collect_foregrounds_block(body, source, tw_config, vars, colors, seen, theme);
            }
        }
        Stmt::Decl(Decl::Var(var_decl)) => {
            for decl in &var_decl.decls {
                if let Some(init) = &decl.init {
                    collect_foregrounds_expr(init, source, tw_config, vars, colors, seen, theme);
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
    theme: Theme,
) {
    match init {
        VarDeclOrExpr::VarDecl(var_decl) => {
            for decl in &var_decl.decls {
                if let Some(init) = &decl.init {
                    collect_foregrounds_expr(init, source, tw_config, vars, colors, seen, theme);
                }
            }
        }
        VarDeclOrExpr::Expr(expr) => {
            collect_foregrounds_expr(expr, source, tw_config, vars, colors, seen, theme);
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
    theme: Theme,
) {
    if let ForHead::VarDecl(var_decl) = head {
        for decl in &var_decl.decls {
            if let Some(init) = &decl.init {
                collect_foregrounds_expr(init, source, tw_config, vars, colors, seen, theme);
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
    theme: Theme,
) {
    match expr {
        Expr::JSXElement(el) => {
            collect_foregrounds_jsx_element(el, source, tw_config, vars, colors, seen, theme);
        }
        Expr::JSXFragment(frag) => {
            for child in &frag.children {
                collect_foregrounds_jsx_child(child, source, tw_config, vars, colors, seen, theme);
            }
        }
        Expr::Paren(p) => collect_foregrounds_expr(&p.expr, source, tw_config, vars, colors, seen, theme),
        Expr::Arrow(arrow) => match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(block) => {
                collect_foregrounds_block(block, source, tw_config, vars, colors, seen, theme)
            }
            BlockStmtOrExpr::Expr(expr) => {
                collect_foregrounds_expr(expr, source, tw_config, vars, colors, seen, theme)
            }
        },
        Expr::Call(call) => {
            if let Callee::Expr(callee) = &call.callee {
                collect_foregrounds_expr(callee, source, tw_config, vars, colors, seen, theme);
            }
            for arg in &call.args {
                collect_foregrounds_expr(&arg.expr, source, tw_config, vars, colors, seen, theme);
            }
        }
        Expr::Cond(cond) => {
            collect_foregrounds_expr(&cond.cons, source, tw_config, vars, colors, seen, theme);
            collect_foregrounds_expr(&cond.alt, source, tw_config, vars, colors, seen, theme);
        }
        Expr::Bin(bin) => {
            collect_foregrounds_expr(&bin.left, source, tw_config, vars, colors, seen, theme);
            collect_foregrounds_expr(&bin.right, source, tw_config, vars, colors, seen, theme);
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
    theme: Theme,
) {
    match child {
        JSXElementChild::JSXElement(el) => {
            collect_foregrounds_jsx_element(el, source, tw_config, vars, colors, seen, theme);
        }
        JSXElementChild::JSXFragment(frag) => {
            for child in &frag.children {
                collect_foregrounds_jsx_child(child, source, tw_config, vars, colors, seen, theme);
            }
        }
        JSXElementChild::JSXExprContainer(container) => {
            if let JSXExpr::Expr(expr) = &container.expr {
                collect_foregrounds_expr(expr, source, tw_config, vars, colors, seen, theme);
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
    theme: Theme,
) {
    let mut elem_colors = ElementColors::default();

    for attr in &element.opening.attrs {
        if let JSXAttrOrSpread::JSXAttr(jsx_attr) = attr {
            let attr_name = jsx_attr_name(&jsx_attr.name);
            match attr_name.as_str() {
                "className" | "class" => {
                    if let Some(value) = &jsx_attr.value {
                        let class_str = extract_jsx_attr_string(value, source);
                        extract_colors_from_classes(&class_str, tw_config, vars, &mut elem_colors, theme);
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
        collect_foregrounds_jsx_child(child, source, tw_config, vars, colors, seen, theme);
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
    theme: Theme,
) {
    match expr {
        Expr::JSXElement(el) => {
            process_jsx_element(el, source, tw_config, vars, file, pairs, inherited_bg, theme);
        }
        Expr::JSXFragment(frag) => {
            for child in &frag.children {
                walk_jsx_child(child, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
        }
        Expr::Paren(p) => {
            walk_expr(&p.expr, source, tw_config, vars, file, pairs, inherited_bg, theme)
        }
        Expr::Arrow(arrow) => match &*arrow.body {
            BlockStmtOrExpr::BlockStmt(block) => {
                walk_block_stmt(block, source, tw_config, vars, file, pairs, inherited_bg, theme)
            }
            BlockStmtOrExpr::Expr(expr) => {
                walk_expr(expr, source, tw_config, vars, file, pairs, inherited_bg, theme)
            }
        },
        Expr::Call(call) => {
            // `cva(...)` / `tv(...)` variant maps hold class strings in object/array
            // literals that the generic arg walk does not reach. Scan each string
            // independently — a variant's classes co-occur, different variants do not.
            if is_variant_factory_call(call) {
                scan_variant_factory_call(call, source, tw_config, vars, file, pairs, theme);
            }
            if let Callee::Expr(callee) = &call.callee {
                walk_expr(callee, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
            for arg in &call.args {
                walk_expr(&arg.expr, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
        }
        Expr::Cond(cond) => {
            walk_expr(&cond.cons, source, tw_config, vars, file, pairs, inherited_bg, theme);
            walk_expr(&cond.alt, source, tw_config, vars, file, pairs, inherited_bg, theme);
        }
        Expr::Bin(bin) => {
            walk_expr(&bin.left, source, tw_config, vars, file, pairs, inherited_bg, theme);
            walk_expr(&bin.right, source, tw_config, vars, file, pairs, inherited_bg, theme);
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
    theme: Theme,
) {
    match child {
        JSXElementChild::JSXElement(el) => {
            process_jsx_element(el, source, tw_config, vars, file, pairs, inherited_bg, theme);
        }
        JSXElementChild::JSXFragment(frag) => {
            for child in &frag.children {
                walk_jsx_child(child, source, tw_config, vars, file, pairs, inherited_bg, theme);
            }
        }
        JSXElementChild::JSXExprContainer(container) => {
            if let JSXExpr::Expr(expr) = &container.expr {
                walk_expr(expr, source, tw_config, vars, file, pairs, inherited_bg, theme);
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
    theme: Theme,
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
                            theme,
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

    // A surface component (Card, DialogContent, …) supplies a background even
    // though its usage site carries no `bg-*` utility. Record it so descendants
    // are not misreported as sitting directly on the backdrop.
    if elem_colors.backgrounds.is_empty() && SURFACE_COMPONENTS.contains(&element_name.as_str()) {
        elem_colors
            .backgrounds
            .push((SURFACE_BACKGROUND.to_string(), Rgba::opaque(128, 128, 128)));
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
    // (own backgrounds if present, otherwise inherited from parent).
    //
    // With neither, the text has no surface anywhere in scope — it renders on
    // whatever the page backdrop is. Pair it against a fully transparent sentinel
    // so the backdrop resolution downstream composites it to the real backdrop
    // (a transparent background over a backdrop sample *is* that sample). Without
    // this, such text was silently skipped and its contrast never checked.
    let backdrop_bg = [(BACKDROP_BACKGROUND.to_string(), Rgba::new(0, 0, 0, 0.0))];
    let bg_source: &[(String, Rgba)] = if !elem_colors.backgrounds.is_empty() {
        &elem_colors.backgrounds
    } else if !inherited_bg.is_empty() {
        inherited_bg
    } else {
        &backdrop_bg
    };

    let theme_label = theme.label().to_string();
    emit_element_pairs(&elem_colors, bg_source, file, &theme_label, pairs);

    // Recurse into children with the effective background context
    for child in &element.children {
        walk_jsx_child(child, source, tw_config, vars, file, pairs, &effective_bg, theme);
    }
}

/// Emit base + interaction-state contrast pairs for one element's collected colors.
///
/// The resting state pairs each foreground against `bg_source`. Each interaction
/// state overrides the base per the CSS cascade: a state's own foreground/background
/// wins, and any property it does not change carries over from the base.
fn emit_element_pairs(
    elem: &ElementColors,
    bg_source: &[(String, Rgba)],
    file: &str,
    theme_label: &str,
    pairs: &mut Vec<ColorPair>,
) {
    let mut push = |fg: &(String, Rgba), bg: &(String, Rgba), state: &str| {
        // A surface component's color is not knowable at the usage site; it only
        // establishes that the text is surfaced. Emitting a pair here would report
        // a fabricated ratio.
        if bg.0 == SURFACE_BACKGROUND {
            return;
        }
        let ratio = contrast_ratio(&fg.1, &bg.1);
        pairs.push(ColorPair {
            foreground_name: fg.0.clone(),
            foreground_color: fg.1,
            background_name: bg.0.clone(),
            background_color: bg.1,
            ratio,
            level: ConformanceLevel::from_ratio(ratio),
            file: file.to_string(),
            line: elem.line,
            element: elem.element_name.clone(),
            theme: theme_label.to_string(),
            state: state.to_string(),
        });
    };

    for fg in &elem.foregrounds {
        for bg in bg_source {
            push(fg, bg, InteractionState::Base.label());
        }
    }

    for layer in &elem.state_layers {
        let state_fgs: &[(String, Rgba)] = if layer.foregrounds.is_empty() {
            &elem.foregrounds
        } else {
            &layer.foregrounds
        };
        let state_bgs: &[(String, Rgba)] = if layer.backgrounds.is_empty() {
            bg_source
        } else {
            &layer.backgrounds
        };
        for fg in state_fgs {
            for bg in state_bgs {
                push(fg, bg, layer.state.label());
            }
        }
    }
}

/// Whether a call is a class-variant factory (`cva(...)` or `tv(...)`) whose
/// arguments hold Tailwind class strings in object/array literals.
fn is_variant_factory_call(call: &CallExpr) -> bool {
    let Callee::Expr(callee) = &call.callee else {
        return false;
    };
    matches!(&**callee, Expr::Ident(ident) if matches!(ident.sym.as_str(), "cva" | "tv"))
}

/// Scan a `cva()`/`tv()` call: each class string literal is scanned independently
/// (a variant's classes co-occur; separate variants are mutually exclusive), so
/// within-string foreground/background pairs — including `hover:`/`focus:` states —
/// are detected even though the classes never appear on a JSX element directly.
fn scan_variant_factory_call(
    call: &CallExpr,
    source: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    file: &str,
    pairs: &mut Vec<ColorPair>,
    theme: Theme,
) {
    let theme_label = theme.label().to_string();
    let mut strings: Vec<(String, usize)> = Vec::new();
    for arg in &call.args {
        collect_class_string_literals(&arg.expr, source, &mut strings);
    }

    for (class_str, line) in strings {
        let mut elem = ElementColors {
            line,
            element_name: "cva".to_string(),
            ..Default::default()
        };
        extract_colors_from_classes(&class_str, tw_config, vars, &mut elem, theme);
        // A cva string is self-contained: its own backgrounds are the only context.
        let bg_source = elem.backgrounds.clone();
        emit_element_pairs(&elem, &bg_source, file, &theme_label, pairs);
    }
}

/// Collect string-literal class sources (with their line numbers) from a `cva`/`tv`
/// argument, descending through array and object-value literals.
fn collect_class_string_literals(expr: &Expr, source: &str, out: &mut Vec<(String, usize)>) {
    match expr {
        Expr::Lit(Lit::Str(s)) => {
            let line = byte_offset_to_line(source, s.span.lo.0 as usize);
            out.push((wtf8_to_string(s), line));
        }
        Expr::Tpl(tpl) => {
            let text = extract_strings_from_expr(expr);
            if !text.trim().is_empty() {
                let line = byte_offset_to_line(source, tpl.span.lo.0 as usize);
                out.push((text, line));
            }
        }
        Expr::Array(arr) => {
            for elem in arr.elems.iter().flatten() {
                collect_class_string_literals(&elem.expr, source, out);
            }
        }
        Expr::Object(obj) => {
            for prop in &obj.props {
                if let PropOrSpread::Prop(prop) = prop {
                    if let Prop::KeyValue(kv) = &**prop {
                        collect_class_string_literals(&kv.value, source, out);
                    }
                }
            }
        }
        Expr::Paren(p) => collect_class_string_literals(&p.expr, source, out),
        Expr::Cond(cond) => {
            collect_class_string_literals(&cond.cons, source, out);
            collect_class_string_literals(&cond.alt, source, out);
        }
        Expr::Bin(bin) => {
            collect_class_string_literals(&bin.left, source, out);
            collect_class_string_literals(&bin.right, source, out);
        }
        _ => {}
    }
}

/// Extract colors from Tailwind classes with theme- and state-aware handling.
///
/// **Theme filtering**: `dark:` classes are excluded in light mode and vice versa.
///
/// **Theme override**: within one interaction state, a theme-specific class
/// (`dark:bg-*` in dark mode) replaces the unprefixed base class, matching
/// Tailwind's specificity model where `.dark &` outranks the base selector.
///
/// **Interaction state**: `hover:`/`focus:`/`focus-visible:` classes are bucketed
/// per state. The resting state feeds the element's own colors (and inheritance);
/// each interaction state becomes a [`StateLayer`] that overrides the base on
/// `:hover`/`:focus`, mirroring how those pseudo-classes cascade at render time.
fn extract_colors_from_classes(
    class_str: &str,
    tw_config: &TailwindColorConfig,
    vars: &HashMap<String, Rgba>,
    elem: &mut ElementColors,
    theme: Theme,
) {
    let mut by_state: HashMap<InteractionState, StateBuckets> = HashMap::new();

    for class in class_str.split_whitespace() {
        let (context, state, base_class) = parse_variant(class);

        // Skip classes that don't apply to this theme
        match (theme, context) {
            (Theme::Light, ThemeContext::DarkOnly) => continue,
            (Theme::Dark, ThemeContext::LightOnly) => continue,
            _ => {}
        }

        let Some((kind, color_name)) = parse_utility_class(base_class) else {
            continue;
        };
        let Some(rgba) = resolve_tailwind_color(&color_name, tw_config, vars) else {
            continue;
        };

        let display_name = base_class.to_string();
        let is_override = match theme {
            Theme::Light => context == ThemeContext::LightOnly,
            Theme::Dark => context == ThemeContext::DarkOnly,
        };
        let buckets = by_state.entry(state).or_default();

        if kind.is_foreground() {
            if is_override {
                buckets.override_fgs.push((display_name, rgba));
            } else {
                buckets.base_fgs.push((display_name, rgba));
            }
        } else if kind.is_background() {
            if is_override {
                buckets.override_bgs.push((display_name, rgba));
            } else {
                buckets.base_bgs.push((display_name, rgba));
            }
        }
    }

    // The resting state feeds the element's own colors and child inheritance.
    if let Some(base) = by_state.remove(&InteractionState::Base) {
        let (fgs, bgs) = base.into_effective();
        elem.foregrounds.extend(fgs);
        elem.backgrounds.extend(bgs);
    }

    // Interaction states become layers, in a fixed order for deterministic output.
    for state in [
        InteractionState::Hover,
        InteractionState::Focus,
        InteractionState::FocusVisible,
    ] {
        if let Some(buckets) = by_state.remove(&state) {
            let (fgs, bgs) = buckets.into_effective();
            if fgs.is_empty() && bgs.is_empty() {
                continue;
            }
            elem.state_layers.push(StateLayer {
                state,
                foregrounds: fgs,
                backgrounds: bgs,
            });
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
pub fn strip_variant_prefixes(class: &str) -> &str {
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
    fn test_parse_variant_context() {
        assert_eq!(
            parse_variant_context("bg-primary"),
            (ThemeContext::Base, "bg-primary")
        );
        assert_eq!(
            parse_variant_context("dark:bg-primary"),
            (ThemeContext::DarkOnly, "bg-primary")
        );
        assert_eq!(
            parse_variant_context("light:text-white"),
            (ThemeContext::LightOnly, "text-white")
        );
        assert_eq!(
            parse_variant_context("sm:dark:hover:bg-red-500"),
            (ThemeContext::DarkOnly, "bg-red-500")
        );
        assert_eq!(
            parse_variant_context("hover:bg-blue-500"),
            (ThemeContext::Base, "bg-blue-500")
        );
    }

    #[test]
    fn test_dark_classes_excluded_in_light_mode() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("component.tsx");
        let source = r#"
            export function Demo() {
                return <div className="bg-white dark:bg-slate-900 text-black dark:text-white" />;
            }
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Light,
        );

        // In light mode: bg-white + text-black only, dark: classes excluded
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].foreground_name, "text-black");
        assert_eq!(pairs[0].background_name, "bg-white");
    }

    #[test]
    fn test_dark_override_replaces_base_in_dark_mode() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("component.tsx");
        let source = r#"
            export function Demo() {
                return <div className="bg-white dark:bg-slate-900 text-black dark:text-white" />;
            }
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Dark,
        );

        // In dark mode: dark:bg-slate-900 overrides bg-white, dark:text-white overrides text-black
        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].foreground_name, "text-white");
        assert_eq!(pairs[0].background_name, "bg-slate-900");
    }

    #[test]
    fn test_base_classes_used_when_no_dark_override() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("component.tsx");
        let source = r#"
            export function Demo() {
                return <div className="bg-white text-red-500" />;
            }
        "#;
        fs::write(&path, source).unwrap();

        // In dark mode with no dark: override, base classes are used
        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Dark,
        );

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].foreground_name, "text-red-500");
        assert_eq!(pairs[0].background_name, "bg-white");
    }

    #[test]
    fn test_partial_dark_override_fg_only() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("component.tsx");
        // Only foreground has dark: override, background does not
        let source = r#"
            export function Demo() {
                return <div className="bg-white text-black dark:text-white" />;
            }
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Dark,
        );

        assert_eq!(pairs.len(), 1);
        assert_eq!(pairs[0].foreground_name, "text-white"); // dark: override
        assert_eq!(pairs[0].background_name, "bg-white"); // no override, base used
    }

    #[test]
    fn test_color_pair_has_theme_field() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("component.tsx");
        let source = r#"
            export function Demo() {
                return <div className="bg-white text-black" />;
            }
        "#;
        fs::write(&path, source).unwrap();

        let light_pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Light,
        );
        assert_eq!(light_pairs[0].theme, "light");

        let dark_pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Dark,
        );
        assert_eq!(dark_pairs[0].theme, "dark");
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
            Theme::Light,
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
                theme: "light".to_string(),
                state: "base".to_string(),
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
                theme: "light".to_string(),
                state: "base".to_string(),
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
                theme: "light".to_string(),
                state: "base".to_string(),
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
                theme: "light".to_string(),
                state: "base".to_string(),
            },
        ];

        let deduped = dedup_color_pairs(pairs);
        assert_eq!(deduped.len(), 3);
        assert_eq!(deduped[0].background_name, "bg-white");
        assert_eq!(deduped[1].background_name, "bg-blue-500");
        assert_eq!(deduped[2].element, "Button");
    }

    #[test]
    fn test_parse_variant_extracts_state() {
        assert_eq!(
            parse_variant("hover:bg-accent"),
            (ThemeContext::Base, InteractionState::Hover, "bg-accent")
        );
        assert_eq!(
            parse_variant("dark:hover:bg-accent"),
            (ThemeContext::DarkOnly, InteractionState::Hover, "bg-accent")
        );
        assert_eq!(
            parse_variant("focus-visible:ring-2"),
            (ThemeContext::Base, InteractionState::FocusVisible, "ring-2")
        );
        assert_eq!(
            parse_variant("focus:text-white"),
            (ThemeContext::Base, InteractionState::Focus, "text-white")
        );
        assert_eq!(
            parse_variant("bg-primary"),
            (ThemeContext::Base, InteractionState::Base, "bg-primary")
        );
    }

    #[test]
    fn test_hover_state_produces_tagged_pair() {
        // A ghost-button-style element: no resting bg/fg, both set only on hover.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("component.tsx");
        let source = r#"
            export function Ghost() {
                return <button className="hover:bg-blue-500 hover:text-white" />;
            }
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Light,
        );

        // No resting-state pair (no base bg/fg), exactly one hover pair.
        assert!(pairs.iter().all(|p| p.state == "hover"));
        let hover = pairs
            .iter()
            .find(|p| p.foreground_name == "text-white" && p.background_name == "bg-blue-500")
            .expect("hover pair present");
        assert_eq!(hover.state, "hover");
    }

    #[test]
    fn test_hover_bg_pairs_against_base_foreground() {
        // Only the background changes on hover; the resting text color carries over.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("component.tsx");
        let source = r#"
            export function Item() {
                return <div className="text-white hover:bg-blue-500" />;
            }
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Light,
        );

        // Base has a foreground but no background → no base pair; hover pairs the
        // carried-over base foreground against the hover background.
        let hover = pairs
            .iter()
            .find(|p| p.state == "hover")
            .expect("hover pair present");
        assert_eq!(hover.foreground_name, "text-white");
        assert_eq!(hover.background_name, "bg-blue-500");
    }

    #[test]
    fn test_cva_variant_strings_are_scanned() {
        // Class strings live only in a cva() variant map (no JSX element), yet the
        // base and hover pairs inside each variant string must still be found.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("button.tsx");
        let source = r#"
            const button = cva("inline-flex text-sm", {
                variants: {
                    variant: {
                        solid: "bg-blue-500 text-white",
                        ghost: "hover:bg-blue-500 hover:text-white",
                    },
                },
            });
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Light,
        );

        // The solid variant is a resting-state pair.
        assert!(
            pairs.iter().any(|p| p.state == "base"
                && p.foreground_name == "text-white"
                && p.background_name == "bg-blue-500"),
            "solid variant base pair missing"
        );
        // The ghost variant only sets colors on hover.
        assert!(
            pairs.iter().any(|p| p.state == "hover"
                && p.foreground_name == "text-white"
                && p.background_name == "bg-blue-500"),
            "ghost variant hover pair missing"
        );
        assert!(pairs.iter().all(|p| p.element == "cva"));
    }

    #[test]
    fn test_text_without_surface_pairs_against_backdrop() {
        // A page heading with no background anywhere in scope renders on the page
        // backdrop. It must be reported, not silently skipped.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("header.tsx");
        let source = r#"
            export function Header() {
                return <h1 className="text-white">Title</h1>;
            }
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Light,
        );

        let pair = pairs
            .iter()
            .find(|p| p.background_name == BACKDROP_BACKGROUND)
            .expect("unsurfaced text should pair against the backdrop");
        assert_eq!(pair.foreground_name, "text-white");
        assert!(
            pair.background_color.a < 1.0,
            "sentinel must be transparent so backdrop resolution yields the backdrop itself"
        );
    }

    #[test]
    fn test_text_with_surface_does_not_use_backdrop() {
        // Control: an inherited background means the text is surfaced, so no
        // backdrop finding should be produced.
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("surfaced.tsx");
        let source = r#"
            export function Surfaced() {
                return (
                    <div className="bg-white">
                        <h1 className="text-black">Title</h1>
                    </div>
                );
            }
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Light,
        );

        assert!(
            pairs
                .iter()
                .all(|p| p.background_name != BACKDROP_BACKGROUND),
            "surfaced text must not be reported against the backdrop"
        );
        assert!(pairs.iter().any(|p| p.background_name == "bg-white"));
    }

    #[test]
    fn test_surface_component_prevents_backdrop_finding() {
        // `<Card>` carries its background inside the component, so the usage site
        // has no `bg-*` utility. Text inside it is surfaced and must not be
        // reported against the backdrop (nor given a fabricated ratio).
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("carded.tsx");
        let source = r#"
            export function Carded() {
                return (
                    <Card>
                        <h1 className="text-white">Title</h1>
                    </Card>
                );
            }
        "#;
        fs::write(&path, source).unwrap();

        let pairs = scan_component(
            &path,
            &TailwindColorConfig::default(),
            &ResolvedVarColors::default(),
            Theme::Light,
        );

        assert!(
            pairs
                .iter()
                .all(|p| p.background_name != BACKDROP_BACKGROUND),
            "text inside a surface component must not be reported as on the backdrop"
        );
        assert!(
            pairs.is_empty(),
            "no fabricated ratio for an unknown surface color, got {pairs:?}"
        );
    }
}

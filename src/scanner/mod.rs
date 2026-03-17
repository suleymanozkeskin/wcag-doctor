pub mod component;
pub mod graph;
pub mod propagation;

use std::path::Path;

use swc_common::sync::Lrc;
use swc_common::{FileName, SourceMap};
use swc_ecma_ast::*;
use swc_ecma_parser::{EsSyntax, Syntax, TsSyntax, parse_file_as_module};

/// Shared SWC module parser used by component, graph, and propagation scanners.
pub(crate) fn parse_module(content: &str, file_path: &Path) -> Option<Module> {
    let cm: Lrc<SourceMap> = Default::default();
    let ext = file_path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("tsx");

    let syntax = match ext {
        "tsx" | "ts" => Syntax::Typescript(TsSyntax {
            tsx: ext == "tsx",
            ..Default::default()
        }),
        "jsx" | "js" | "mjs" => Syntax::Es(EsSyntax {
            jsx: ext == "jsx" || ext == "js",
            ..Default::default()
        }),
        _ => Syntax::Typescript(TsSyntax {
            tsx: true,
            ..Default::default()
        }),
    };

    let fm = cm.new_source_file(
        FileName::Real(file_path.to_path_buf()).into(),
        content.to_string(),
    );

    let mut errors = Vec::new();
    parse_file_as_module(&fm, syntax, EsVersion::latest(), None, &mut errors).ok()
}

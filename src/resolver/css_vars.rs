use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, RwLock};

use cssparser::{BasicParseErrorKind, CowRcStr, Parser, ParserState};
use lightningcss::declaration::DeclarationBlock;
use lightningcss::error::{ParserError as LightningParserError, PrinterError};
use lightningcss::media_query::{MediaCondition, MediaFeatureId, MediaFeatureName, MediaFeatureValue, MediaList, QueryFeature};
use lightningcss::printer::Printer;
use lightningcss::properties::Property;
use lightningcss::rules::{CssRule, CssRuleList};
use lightningcss::selector::{Component, Selector, SelectorList};
use lightningcss::stylesheet::{ParserOptions, PrinterOptions, StyleSheet};
use lightningcss::traits::{AtRuleParser, ToCss};
use parcel_selectors::attr::{AttrSelectorOperator, ParsedAttrSelectorOperation};

use crate::color::Rgba;
use crate::color::parser::{ColorParseResult, parse_color};

/// A theme-aware mapping of CSS custom property names to their resolved colors.
#[derive(Debug, Default)]
pub struct CssVarMap {
    /// Variables defined in `:root` (light theme).
    pub light: HashMap<String, String>,
    /// Variables defined in `.dark` (dark theme).
    pub dark: HashMap<String, String>,
}

/// Parsed and resolved CSS variable colors per theme.
#[derive(Debug, Default)]
pub struct ResolvedVarColors {
    pub light: HashMap<String, Rgba>,
    pub dark: HashMap<String, Rgba>,
}

#[derive(Debug, Clone)]
struct ThemePrelude {
    inline: bool,
}

#[derive(Debug, Clone)]
struct ThemeAtRule<'i> {
    inline: bool,
    declarations: DeclarationBlock<'i>,
}

#[derive(Default)]
struct ThemeAtRuleParser;

impl<'i> AtRuleParser<'i> for ThemeAtRuleParser {
    type Prelude = ThemePrelude;
    type AtRule = ThemeAtRule<'i>;
    type Error = LightningParserError<'i>;

    fn parse_prelude<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _options: &ParserOptions<'_, 'i>,
    ) -> Result<Self::Prelude, cssparser::ParseError<'i, Self::Error>> {
        if !name.eq_ignore_ascii_case("theme") {
            return Err(input.new_error(BasicParseErrorKind::AtRuleInvalid(name)));
        }

        let inline = input
            .try_parse(|input| input.expect_ident_matching("inline"))
            .is_ok();
        input.expect_exhausted()?;

        Ok(ThemePrelude { inline })
    }

    fn parse_block<'t>(
        &mut self,
        prelude: Self::Prelude,
        _start: &ParserState,
        input: &mut Parser<'i, 't>,
        options: &ParserOptions<'_, 'i>,
        _is_nested: bool,
    ) -> Result<Self::AtRule, cssparser::ParseError<'i, Self::Error>> {
        let declarations = DeclarationBlock::parse(input, options)?;
        Ok(ThemeAtRule {
            inline: prelude.inline,
            declarations,
        })
    }
}

impl<'i> ToCss for ThemeAtRule<'i> {
    fn to_css<W>(&self, dest: &mut Printer<W>) -> Result<(), PrinterError>
    where
        W: std::fmt::Write,
    {
        dest.write_str("@theme")?;
        if self.inline {
            dest.write_str(" inline")?;
        }
        dest.whitespace()?;
        dest.write_char('{')?;
        dest.indent();
        dest.newline()?;
        self.declarations.to_css(dest)?;
        dest.dedent();
        dest.newline()?;
        dest.write_char('}')
    }
}

/// Parse a CSS file and extract custom property declarations from :root and .dark blocks.
pub fn extract_css_vars(css_content: &str) -> CssVarMap {
    extract_css_vars_with_diagnostics(css_content).0
}

/// Parse a CSS file and return extracted variables plus parser diagnostics.
pub fn extract_css_vars_with_diagnostics(css_content: &str) -> (CssVarMap, Vec<String>) {
    let warnings = Arc::new(RwLock::new(Vec::new()));
    let options = ParserOptions {
        error_recovery: true,
        warnings: Some(warnings.clone()),
        ..ParserOptions::default()
    };

    if let Ok(sheet) = StyleSheet::parse_with(css_content, options, &mut ThemeAtRuleParser) {
        let mut map = CssVarMap::default();
        walk_rules(&sheet.rules, &mut map, false);
        return (map, collect_css_diagnostics(&warnings));
    }

    let mut diagnostics = collect_css_diagnostics(&warnings);
    diagnostics.push("failed to parse CSS file".to_string());
    (CssVarMap::default(), diagnostics)
}

/// Resolve all raw CSS variable values to concrete RGBA colors.
/// Handles:
/// - Direct color values (hex, rgb, hsl, bare hsl, named, oklch)
/// - var(--other) references within the same theme
/// - hsl(var(--x)) wrapper patterns (unwrap var, parse as bare HSL)
pub fn resolve_var_colors(var_map: &CssVarMap) -> ResolvedVarColors {
    ResolvedVarColors {
        light: resolve_theme(&var_map.light),
        dark: resolve_theme(&var_map.dark),
    }
}

fn resolve_theme(vars: &HashMap<String, String>) -> HashMap<String, Rgba> {
    let mut resolved: HashMap<String, Rgba> = HashMap::new();

    for (name, raw_value) in vars {
        if let Some(rgba) = resolve_single(name, raw_value, vars, &mut HashSet::new()) {
            resolved.insert(name.clone(), rgba);
        }
    }

    resolved
}

/// Resolve a single CSS variable to a color, following var() references.
/// `visited` prevents infinite loops from circular references.
fn resolve_single(
    name: &str,
    raw_value: &str,
    all_vars: &HashMap<String, String>,
    visited: &mut HashSet<String>,
) -> Option<Rgba> {
    if !visited.insert(name.to_string()) {
        return None; // Circular reference
    }

    match parse_color(raw_value) {
        ColorParseResult::Resolved(rgba) => Some(rgba),
        ColorParseResult::VarReference(ref_name) => {
            let ref_value = all_vars.get(&ref_name)?;
            resolve_single(&ref_name, ref_value, all_vars, visited)
        }
        ColorParseResult::WrappedVarReference {
            wrapper: _,
            var_name,
        } => {
            // E.g. hsl(var(--primary)) — look up --primary, expect bare HSL
            let ref_value = all_vars.get(&var_name)?;
            // The referenced value should be bare HSL like "204 88% 24.1%"
            match parse_color(ref_value) {
                ColorParseResult::Resolved(rgba) => Some(rgba),
                _ => None,
            }
        }
        ColorParseResult::Unresolvable(_) => None,
    }
}

/// Load CSS variables from a file path.
pub fn load_css_vars_from_file(path: &Path) -> Result<CssVarMap, std::io::Error> {
    Ok(load_css_vars_from_file_with_diagnostics(path)?.0)
}

/// Load CSS variables from a file path and return parser diagnostics.
pub fn load_css_vars_from_file_with_diagnostics(
    path: &Path,
) -> Result<(CssVarMap, Vec<String>), std::io::Error> {
    let content = std::fs::read_to_string(path)?;
    Ok(extract_css_vars_with_diagnostics(&content))
}

fn collect_css_diagnostics(
    warnings: &Arc<RwLock<Vec<lightningcss::error::Error<LightningParserError<'_>>>>>,
) -> Vec<String> {
    warnings
        .read()
        .map(|warnings| warnings.iter().map(ToString::to_string).collect())
        .unwrap_or_default()
}

fn walk_rules<'i>(
    rules: &CssRuleList<'i, ThemeAtRule<'i>>,
    map: &mut CssVarMap,
    in_dark_media: bool,
) {
    for rule in &rules.0 {
        match rule {
            CssRule::Style(style) => {
                apply_style_rule(&style.selectors, &style.declarations, map, in_dark_media);
                walk_rules(&style.rules, map, in_dark_media);
            }
            CssRule::Media(media) => {
                walk_rules(
                    &media.rules,
                    map,
                    in_dark_media || media_list_prefers_dark(&media.query),
                );
            }
            CssRule::Supports(rule) => walk_rules(&rule.rules, map, in_dark_media),
            CssRule::LayerBlock(rule) => walk_rules(&rule.rules, map, in_dark_media),
            CssRule::Container(rule) => walk_rules(&rule.rules, map, in_dark_media),
            CssRule::Scope(rule) => walk_rules(&rule.rules, map, in_dark_media),
            CssRule::StartingStyle(rule) => walk_rules(&rule.rules, map, in_dark_media),
            CssRule::Nesting(rule) => {
                apply_style_rule(&rule.style.selectors, &rule.style.declarations, map, in_dark_media);
                walk_rules(&rule.style.rules, map, in_dark_media);
            }
            CssRule::NestedDeclarations(rule) => {
                let target = if in_dark_media {
                    &mut map.dark
                } else {
                    &mut map.light
                };
                insert_declarations(&rule.declarations, target);
            }
            CssRule::Custom(rule) => {
                insert_declarations(&rule.declarations, &mut map.light);
            }
            _ => {}
        }
    }
}

fn apply_style_rule(
    selectors: &SelectorList<'_>,
    declarations: &DeclarationBlock<'_>,
    map: &mut CssVarMap,
    in_dark_media: bool,
) {
    let targets = classify_selectors(selectors, in_dark_media);
    if targets.light {
        insert_declarations(declarations, &mut map.light);
    }
    if targets.dark {
        insert_declarations(declarations, &mut map.dark);
    }
}

fn insert_declarations(declarations: &DeclarationBlock<'_>, target: &mut HashMap<String, String>) {
    for property in declarations
        .declarations
        .iter()
        .chain(declarations.important_declarations.iter())
    {
        if let Property::Custom(custom) = property {
            if let Ok(value) = property.value_to_css_string(PrinterOptions::default()) {
                target.insert(
                    custom.name.as_ref().to_string(),
                    normalize_css_var_value(value.trim()),
                );
            }
        }
    }
}

#[derive(Default, Clone, Copy)]
struct ThemeTargets {
    light: bool,
    dark: bool,
}

fn classify_selectors(selectors: &SelectorList<'_>, in_dark_media: bool) -> ThemeTargets {
    let mut targets = ThemeTargets::default();

    for selector in &selectors.0 {
        let selector_targets = classify_selector(selector, in_dark_media);
        targets.light |= selector_targets.light;
        targets.dark |= selector_targets.dark;
    }

    targets
}

fn classify_selector(selector: &Selector<'_>, in_dark_media: bool) -> ThemeTargets {
    let mut targets = ThemeTargets::default();

    for component in selector.iter_raw_match_order() {
        let component_targets = classify_component(component, in_dark_media);
        targets.light |= component_targets.light;
        targets.dark |= component_targets.dark;
    }

    targets
}

fn classify_component(component: &Component<'_>, in_dark_media: bool) -> ThemeTargets {
    match component {
        Component::Root => ThemeTargets {
            light: !in_dark_media,
            dark: in_dark_media,
        },
        Component::Class(name) => match name.as_ref() {
            "light" => ThemeTargets {
                light: true,
                dark: false,
            },
            "dark" => ThemeTargets {
                light: false,
                dark: true,
            },
            _ => ThemeTargets::default(),
        },
        Component::AttributeInNoNamespace {
            local_name,
            operator,
            value,
            never_matches,
            ..
        } => classify_theme_attribute(local_name.as_ref(), operator, value.as_ref(), *never_matches),
        Component::AttributeOther(attr) => {
            let value = match &attr.operation {
                ParsedAttrSelectorOperation::WithValue {
                    operator,
                    expected_value,
                    ..
                } => Some((operator, expected_value.as_ref())),
                ParsedAttrSelectorOperation::Exists => None,
            };
            classify_theme_attribute_other(attr.local_name.as_ref(), value, attr.never_matches)
        }
        Component::Is(selectors)
        | Component::Where(selectors)
        | Component::Any(_, selectors)
        | Component::Has(selectors) => classify_selector_group(selectors, in_dark_media),
        Component::Negation(selectors) => negate_theme_targets(
            classify_selector_group(selectors, in_dark_media),
        ),
        Component::Host(Some(selector)) | Component::Slotted(selector) => {
            classify_selector(selector, in_dark_media)
        }
        _ => ThemeTargets::default(),
    }
}

fn classify_selector_group(selectors: &[Selector<'_>], in_dark_media: bool) -> ThemeTargets {
    let mut targets = ThemeTargets::default();
    for selector in selectors {
        let nested = classify_selector(selector, in_dark_media);
        targets.light |= nested.light;
        targets.dark |= nested.dark;
    }
    targets
}

fn negate_theme_targets(targets: ThemeTargets) -> ThemeTargets {
    match (targets.light, targets.dark) {
        (true, false) => ThemeTargets {
            light: false,
            dark: true,
        },
        (false, true) => ThemeTargets {
            light: true,
            dark: false,
        },
        _ => ThemeTargets::default(),
    }
}

fn classify_theme_attribute(
    local_name: &str,
    operator: &AttrSelectorOperator,
    value: &str,
    never_matches: bool,
) -> ThemeTargets {
    if never_matches || *operator != AttrSelectorOperator::Equal {
        return ThemeTargets::default();
    }

    match (local_name, value) {
        ("data-theme", "light") | ("data-mode", "light") => ThemeTargets {
            light: true,
            dark: false,
        },
        ("data-theme", "dark") | ("data-mode", "dark") => ThemeTargets {
            light: false,
            dark: true,
        },
        _ => ThemeTargets::default(),
    }
}

fn classify_theme_attribute_other(
    local_name: &str,
    value: Option<(&AttrSelectorOperator, &str)>,
    never_matches: bool,
) -> ThemeTargets {
    let Some((operator, value)) = value else {
        return ThemeTargets::default();
    };

    classify_theme_attribute(local_name, operator, value, never_matches)
}

fn normalize_css_var_value(value: &str) -> String {
    if let Some(expanded) = expand_shorthand_hex(value) {
        return expanded;
    }

    value.to_string()
}

fn expand_shorthand_hex(value: &str) -> Option<String> {
    let hex = value.strip_prefix('#')?;
    if !(hex.len() == 3 || hex.len() == 4) || !hex.chars().all(|ch| ch.is_ascii_hexdigit()) {
        return None;
    }

    let mut expanded = String::with_capacity(1 + hex.len() * 2);
    expanded.push('#');
    for ch in hex.chars() {
        expanded.push(ch);
        expanded.push(ch);
    }
    Some(expanded)
}

fn media_list_prefers_dark(media_list: &MediaList<'_>) -> bool {
    media_list
        .media_queries
        .iter()
        .any(|query| query.condition.as_ref().is_some_and(media_condition_prefers_dark))
}

fn media_condition_prefers_dark(condition: &MediaCondition<'_>) -> bool {
    match condition {
        MediaCondition::Feature(feature) => media_feature_prefers_dark(feature),
        MediaCondition::Operation { conditions, .. } => {
            conditions.iter().any(media_condition_prefers_dark)
        }
        MediaCondition::Not(condition) => media_condition_prefers_dark(condition),
        MediaCondition::Unknown(_) => false,
    }
}

fn media_feature_prefers_dark(feature: &QueryFeature<'_, MediaFeatureId>) -> bool {
    match feature {
        QueryFeature::Plain { name, value } | QueryFeature::Range { name, value, .. } => {
            matches!(name, MediaFeatureName::Standard(MediaFeatureId::PrefersColorScheme))
                && matches!(value, MediaFeatureValue::Ident(ident) if ident.as_ref() == "dark")
        }
        QueryFeature::Boolean { .. } | QueryFeature::Interval { .. } => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_root_vars() {
        let css = r#"
            :root {
                --background: 0 0% 100%;
                --foreground: 222.2 84% 4.9%;
                --primary: 204 88% 24.1%;
            }
        "#;
        let map = extract_css_vars(css);
        assert_eq!(map.light.len(), 3);
        assert_eq!(map.light.get("--background").unwrap(), "0 0% 100%");
        assert_eq!(map.light.get("--primary").unwrap(), "204 88% 24.1%");
    }

    #[test]
    fn test_extract_dark_vars() {
        let css = r#"
            :root {
                --background: 0 0% 100%;
            }
            .dark {
                --background: 0 0% 0%;
            }
        "#;
        let map = extract_css_vars(css);
        assert_eq!(map.light.get("--background").unwrap(), "0 0% 100%");
        assert_eq!(map.dark.get("--background").unwrap(), "0 0% 0%");
    }

    #[test]
    fn test_resolve_bare_hsl() {
        let css = r#"
            :root {
                --background: 0 0% 100%;
                --foreground: 0 0% 0%;
            }
        "#;
        let map = extract_css_vars(css);
        let resolved = resolve_var_colors(&map);

        let bg = resolved.light.get("--background").unwrap();
        assert_eq!((bg.r, bg.g, bg.b), (255, 255, 255));

        let fg = resolved.light.get("--foreground").unwrap();
        assert_eq!((fg.r, fg.g, fg.b), (0, 0, 0));
    }

    #[test]
    fn test_resolve_full_hsl() {
        let css = r#"
            :root {
                --chart-1: hsl(203.8863 88.2845% 53.1373%);
            }
        "#;
        let map = extract_css_vars(css);
        let resolved = resolve_var_colors(&map);
        assert!(resolved.light.contains_key("--chart-1"));
    }

    #[test]
    fn test_data_theme_selectors() {
        let css = r#"
            :root {
                --background: 0 0% 100%;
            }
            [data-theme="dark"] {
                --background: 0 0% 0%;
            }
        "#;
        let map = extract_css_vars(css);
        assert_eq!(map.light.get("--background").unwrap(), "0 0% 100%");
        assert_eq!(map.dark.get("--background").unwrap(), "0 0% 0%");
    }

    #[test]
    fn test_light_class_selector() {
        let css = r#"
            .light {
                --foreground: 0 0% 10%;
            }
            .dark {
                --foreground: 0 0% 90%;
            }
        "#;
        let map = extract_css_vars(css);
        assert_eq!(map.light.get("--foreground").unwrap(), "0 0% 10%");
        assert_eq!(map.dark.get("--foreground").unwrap(), "0 0% 90%");
    }

    #[test]
    fn test_not_dark_selector_maps_to_light_theme() {
        let css = r#"
            :not(.dark) {
                --foreground: #000000;
            }
        "#;
        let map = extract_css_vars(css);
        assert_eq!(map.light.get("--foreground").unwrap(), "#000000");
        assert!(!map.dark.contains_key("--foreground"));
    }

    #[test]
    fn test_nested_layer_root_block() {
        let css = r#"
            @layer base {
                :root {
                    --background: 0 0% 100%;
                    --foreground: 0 0% 0%;
                }
            }
        "#;

        let map = extract_css_vars(css);
        assert_eq!(map.light.get("--background").unwrap(), "0 0% 100%");
        assert_eq!(map.light.get("--foreground").unwrap(), "0 0% 0%");
    }

    #[test]
    fn test_light_nested_declarations_are_preserved() {
        let css = r#"
            :root {
                @media (min-width: 0px) {}
                --background: #ffffff;
            }
        "#;

        let map = extract_css_vars(css);
        assert_eq!(map.light.get("--background").unwrap(), "#ffffff");
    }

    #[test]
    fn test_dark_media_root_block() {
        let css = r#"
            @media (prefers-color-scheme: dark) {
                :root {
                    --background: 0 0% 0%;
                    --foreground: 0 0% 100%;
                }
            }
        "#;

        let map = extract_css_vars(css);
        assert_eq!(map.dark.get("--background").unwrap(), "0 0% 0%");
        assert_eq!(map.dark.get("--foreground").unwrap(), "0 0% 100%");
    }

    #[test]
    fn test_theme_nested_in_layer() {
        let css = r#"
            @layer theme {
                @theme inline {
                    --color-primary: #3b82f6;
                }
            }
        "#;

        let map = extract_css_vars(css);
        assert_eq!(map.light.get("--color-primary").unwrap(), "#3b82f6");
    }

    #[test]
    fn test_invalid_css_reports_diagnostics() {
        let css = r#"
            :root {
                --background: #ffffff;
            }

            :root {
                broken;
            }
        "#;

        let (map, diagnostics) = extract_css_vars_with_diagnostics(css);
        assert_eq!(map.light.get("--background").unwrap(), "#ffffff");
        assert!(!diagnostics.is_empty());
    }
}

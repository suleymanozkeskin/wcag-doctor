use serde::Serialize;

use crate::audit::design_system::DesignSystemPair;
use crate::contrast::levels::{ConformanceLevel, MinimumLevel};
use crate::scanner::component::ColorPair;

#[derive(Serialize)]
pub struct JsonReport {
    pub version: String,
    pub minimum_level: String,
    pub summary: JsonSummary,
    pub design_system: Vec<JsonDesignSystemResult>,
    pub components: Vec<JsonComponentResult>,
}

#[derive(Serialize)]
pub struct JsonSummary {
    pub total: usize,
    pub pass_aaa: usize,
    pub pass_aa_large: usize,
    pub pass_aa: usize,
    pub fail: usize,
}

#[derive(Serialize)]
pub struct JsonDesignSystemResult {
    pub theme: String,
    pub foreground: JsonColorInfo,
    pub background: JsonColorInfo,
    pub ratio: f64,
    pub level: String,
    pub passes: bool,
}

#[derive(Serialize)]
pub struct JsonComponentResult {
    pub file: String,
    pub line: usize,
    pub element: String,
    pub foreground: JsonColorInfo,
    pub background: JsonColorInfo,
    pub ratio: f64,
    pub level: String,
    pub passes: bool,
}

#[derive(Serialize)]
pub struct JsonColorInfo {
    pub name: String,
    pub hex: String,
}

/// Build and serialize a full JSON report.
pub fn build_json_report(
    design_pairs: &[DesignSystemPair],
    component_pairs: &[ColorPair],
    minimum: MinimumLevel,
) -> String {
    let mut pass_aaa = 0;
    let mut pass_aa_large = 0;
    let mut pass_aa = 0;
    let mut fail = 0;
    let total = design_pairs.len() + component_pairs.len();

    let ds_results: Vec<JsonDesignSystemResult> = design_pairs
        .iter()
        .map(|p| {
            let passes = p.level.meets_minimum(minimum);
            match p.level {
                ConformanceLevel::Aaa => pass_aaa += 1,
                ConformanceLevel::Aa => pass_aa += 1,
                ConformanceLevel::AaLargeOnly => pass_aa_large += 1,
                ConformanceLevel::Fail => fail += 1,
            }
            JsonDesignSystemResult {
                theme: p.theme.clone(),
                foreground: JsonColorInfo {
                    name: p.foreground_name.clone(),
                    hex: p.foreground_color.to_hex(),
                },
                background: JsonColorInfo {
                    name: p.background_name.clone(),
                    hex: p.background_color.to_hex(),
                },
                ratio: (p.ratio * 100.0).round() / 100.0,
                level: p.level.label().to_string(),
                passes,
            }
        })
        .collect();

    let comp_results: Vec<JsonComponentResult> = component_pairs
        .iter()
        .map(|p| {
            let passes = p.level.meets_minimum(minimum);
            match p.level {
                ConformanceLevel::Aaa => pass_aaa += 1,
                ConformanceLevel::Aa => pass_aa += 1,
                ConformanceLevel::AaLargeOnly => pass_aa_large += 1,
                ConformanceLevel::Fail => fail += 1,
            }
            JsonComponentResult {
                file: p.file.clone(),
                line: p.line,
                element: p.element.clone(),
                foreground: JsonColorInfo {
                    name: p.foreground_name.clone(),
                    hex: p.foreground_color.to_hex(),
                },
                background: JsonColorInfo {
                    name: p.background_name.clone(),
                    hex: p.background_color.to_hex(),
                },
                ratio: (p.ratio * 100.0).round() / 100.0,
                level: p.level.label().to_string(),
                passes,
            }
        })
        .collect();

    let report = JsonReport {
        version: env!("CARGO_PKG_VERSION").to_string(),
        minimum_level: match minimum {
            MinimumLevel::AaLarge => "AA Large".to_string(),
            MinimumLevel::Aa => "AA".to_string(),
            MinimumLevel::Aaa => "AAA".to_string(),
        },
        summary: JsonSummary {
            total,
            pass_aaa,
            pass_aa_large,
            pass_aa,
            fail,
        },
        design_system: ds_results,
        components: comp_results,
    };

    serde_json::to_string_pretty(&report).unwrap_or_else(|e| {
        serde_json::json!({"error": e.to_string()}).to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Rgba;
    use crate::contrast::levels::{ConformanceLevel, MinimumLevel};

    #[test]
    fn json_summary_separates_aa_large_from_fail() {
        let components = vec![ColorPair {
            foreground_name: "text".to_string(),
            foreground_color: Rgba::opaque(0, 0, 0),
            background_name: "bg".to_string(),
            background_color: Rgba::opaque(255, 255, 255),
            ratio: 3.2,
            level: ConformanceLevel::AaLargeOnly,
            file: "file.tsx".to_string(),
            line: 1,
            element: "p".to_string(),
        }];

        let report = build_json_report(&[], &components, MinimumLevel::AaLarge);
        let value: serde_json::Value = serde_json::from_str(&report).unwrap();

        assert_eq!(value["summary"]["pass_aa_large"], 1);
        assert_eq!(value["summary"]["fail"], 0);
        assert_eq!(value["components"][0]["passes"], true);
    }
}

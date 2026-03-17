use serde::Serialize;

/// WCAG 2.1 conformance level for a contrast ratio.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub enum ConformanceLevel {
    /// Below AA threshold — fails WCAG contrast requirements.
    Fail,
    /// Passes AA for large text only (ratio >= 3:1).
    AaLargeOnly,
    /// Passes AA for all text (ratio >= 4.5:1).
    Aa,
    /// Passes AAA for all text (ratio >= 7:1).
    Aaa,
}

impl ConformanceLevel {
    /// Determine the conformance level for a given contrast ratio.
    pub fn from_ratio(ratio: f64) -> Self {
        if ratio >= 7.0 {
            Self::Aaa
        } else if ratio >= 4.5 {
            Self::Aa
        } else if ratio >= 3.0 {
            Self::AaLargeOnly
        } else {
            Self::Fail
        }
    }

    /// Whether this level meets the given minimum requirement.
    pub fn meets_minimum(&self, minimum: MinimumLevel) -> bool {
        match minimum {
            MinimumLevel::AaLarge => *self >= Self::AaLargeOnly,
            MinimumLevel::Aa => *self >= Self::Aa,
            MinimumLevel::Aaa => *self >= Self::Aaa,
        }
    }

    /// Human-readable label.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Fail => "FAIL",
            Self::AaLargeOnly => "AA Large",
            Self::Aa => "AA",
            Self::Aaa => "AAA",
        }
    }
}

impl std::fmt::Display for ConformanceLevel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.label())
    }
}

/// The minimum level the user wants to check against.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MinimumLevel {
    AaLarge,
    Aa,
    Aaa,
}

impl std::str::FromStr for MinimumLevel {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "aa-large" | "aa_large" | "aalarge" => Ok(Self::AaLarge),
            "aa" => Ok(Self::Aa),
            "aaa" => Ok(Self::Aaa),
            other => Err(format!(
                "unknown level '{other}', expected 'aa-large', 'aa' or 'aaa'"
            )),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aaa_ratio() {
        assert_eq!(ConformanceLevel::from_ratio(7.0), ConformanceLevel::Aaa);
        assert_eq!(ConformanceLevel::from_ratio(21.0), ConformanceLevel::Aaa);
    }

    #[test]
    fn test_aa_ratio() {
        assert_eq!(ConformanceLevel::from_ratio(4.5), ConformanceLevel::Aa);
        assert_eq!(ConformanceLevel::from_ratio(6.9), ConformanceLevel::Aa);
    }

    #[test]
    fn test_aa_large_ratio() {
        assert_eq!(
            ConformanceLevel::from_ratio(3.0),
            ConformanceLevel::AaLargeOnly
        );
        assert_eq!(
            ConformanceLevel::from_ratio(4.4),
            ConformanceLevel::AaLargeOnly
        );
    }

    #[test]
    fn test_fail_ratio() {
        assert_eq!(ConformanceLevel::from_ratio(2.9), ConformanceLevel::Fail);
        assert_eq!(ConformanceLevel::from_ratio(1.0), ConformanceLevel::Fail);
    }

    #[test]
    fn test_meets_minimum() {
        assert!(ConformanceLevel::AaLargeOnly.meets_minimum(MinimumLevel::AaLarge));
        assert!(ConformanceLevel::Aaa.meets_minimum(MinimumLevel::Aa));
        assert!(ConformanceLevel::Aaa.meets_minimum(MinimumLevel::Aaa));
        assert!(ConformanceLevel::Aa.meets_minimum(MinimumLevel::Aa));
        assert!(!ConformanceLevel::Aa.meets_minimum(MinimumLevel::Aaa));
        assert!(!ConformanceLevel::Fail.meets_minimum(MinimumLevel::Aa));
    }

    #[test]
    fn test_parse_aa_large() {
        assert_eq!("aa-large".parse::<MinimumLevel>().unwrap(), MinimumLevel::AaLarge);
        assert_eq!("aa_large".parse::<MinimumLevel>().unwrap(), MinimumLevel::AaLarge);
        assert_eq!("aalarge".parse::<MinimumLevel>().unwrap(), MinimumLevel::AaLarge);
    }
}

//! Package versions and version selectors (§83).
//!
//! §83 fixes the supported selector notation to exactly four forms: exact
//! version, `N.M.x`, `N.x`, and `x`. Range operators (`^`, `~`, `>=`) are
//! explicitly not used by ZeroScript.

use std::fmt;
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
}

impl Version {
    pub fn new(major: u64, minor: u64, patch: u64) -> Self {
        Version {
            major,
            minor,
            patch,
        }
    }
}

impl FromStr for Version {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let parts: Vec<&str> = s.split('.').collect();
        if parts.len() != 3 {
            return Err(format!(
                "invalid version '{}' (expected MAJOR.MINOR.PATCH)",
                s
            ));
        }
        let mut nums = [0u64; 3];
        for (i, part) in parts.iter().enumerate() {
            if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
                return Err(format!("invalid version '{}'", s));
            }
            if part.len() > 1 && part.starts_with('0') {
                return Err(format!("invalid version '{}' (leading zero)", s));
            }
            nums[i] = part
                .parse::<u64>()
                .map_err(|_| format!("invalid version '{}'", s))?;
        }
        Ok(Version::new(nums[0], nums[1], nums[2]))
    }
}

impl fmt::Display for Version {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// Version selector (§83). Only the four documented forms exist.
#[derive(Debug, Clone, PartialEq)]
pub enum Selector {
    /// `"1.2.0"` — exactly this version.
    Exact(Version),
    /// `"1.x"` — MAJOR must match.
    Major(u64),
    /// `"1.2.x"` — MAJOR and MINOR must match.
    Minor(u64, u64),
    /// `"x"` — any version.
    Any,
}

impl Selector {
    pub fn parse(s: &str) -> Result<Selector, String> {
        if s == "x" {
            return Ok(Selector::Any);
        }
        let parts: Vec<&str> = s.split('.').collect();
        match parts.as_slice() {
            // Bare "1" is not one of the four documented forms.
            [_] => Err(format!(
                "invalid version selector '{}' (use MAJOR.MINOR.PATCH, N.M.x, N.x, or x)",
                s
            )),
            [major, "x"] => Ok(Selector::Major(parse_component(major, s)?)),
            [major, minor, "x"] => Ok(Selector::Minor(
                parse_component(major, s)?,
                parse_component(minor, s)?,
            )),
            [_, _, _] => {
                // All-numeric three-part form: exact version. Anything else
                // in the third slot (other than "x") is not in the spec.
                let v: Version = s
                    .parse()
                    .map_err(|_| format!("invalid version selector '{}'", s))?;
                Ok(Selector::Exact(v))
            }
            _ => Err(format!(
                "invalid version selector '{}' (use MAJOR.MINOR.PATCH, N.M.x, N.x, or x)",
                s
            )),
        }
    }

    pub fn matches(&self, v: &Version) -> bool {
        match self {
            Selector::Exact(want) => v == want,
            Selector::Major(major) => v.major == *major,
            Selector::Minor(major, minor) => v.major == *major && v.minor == *minor,
            Selector::Any => true,
        }
    }
}

impl fmt::Display for Selector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Selector::Exact(v) => write!(f, "{}", v),
            Selector::Major(major) => write!(f, "{}.x", major),
            Selector::Minor(major, minor) => write!(f, "{}.{}.x", major, minor),
            Selector::Any => write!(f, "x"),
        }
    }
}

fn parse_component(part: &str, whole: &str) -> Result<u64, String> {
    if part.is_empty() || !part.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("invalid version selector '{}'", whole));
    }
    if part.len() > 1 && part.starts_with('0') {
        return Err(format!("invalid version selector '{}'", whole));
    }
    part.parse::<u64>()
        .map_err(|_| format!("invalid version selector '{}'", whole))
}

/// Highest version satisfying **every** selector (§83: "Selector selalu
/// memilih versi terbaru yang memenuhi pola selector").
pub fn highest_matching<'a>(available: &'a [Version], selectors: &[Selector]) -> Option<&'a Version> {
    available
        .iter()
        .filter(|v| selectors.iter().all(|s| s.matches(v)))
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        s.parse().unwrap()
    }

    fn vs(list: &[&str]) -> Vec<Version> {
        list.iter().map(|s| v(s)).collect()
    }

    fn pick(available: &[Version], selectors: &[Selector]) -> Option<Version> {
        highest_matching(available, selectors).cloned()
    }

    // --- §83 examples, verbatim ---

    #[test]
    fn test_job017_version_parse_examples() {
        for s in ["1.2.0", "1.2.10", "1.3.2", "2.0.0"] {
            assert!(s.parse::<Version>().is_ok(), "{}", s);
        }
    }

    #[test]
    fn test_job017_selector_1_2_x_picks_1_2_10() {
        // Available: 1.2.0, 1.2.5, 1.2.10, 1.3.2 → "1.2.x" picks 1.2.10.
        let avail = vs(&["1.2.0", "1.2.5", "1.2.10", "1.3.2"]);
        let sel = Selector::parse("1.2.x").unwrap();
        assert_eq!(highest_matching(&avail, &[sel]).cloned(), Some(v("1.2.10")));
    }

    #[test]
    fn test_job017_selector_1_x_picks_1_3_2() {
        // Available: 1.2.10, 1.3.0, 1.3.2, 2.0.0 → "1.x" picks 1.3.2.
        let avail = vs(&["1.2.10", "1.3.0", "1.3.2", "2.0.0"]);
        let sel = Selector::parse("1.x").unwrap();
        assert_eq!(highest_matching(&avail, &[sel]).cloned(), Some(v("1.3.2")));
    }

    #[test]
    fn test_job017_selector_x_picks_newest() {
        let avail = vs(&["1.0.0", "1.5.0", "2.0.0"]);
        let sel = Selector::parse("x").unwrap();
        assert_eq!(highest_matching(&avail, &[sel]).cloned(), Some(v("2.0.0")));
    }

    #[test]
    fn test_job017_selector_exact_only_matches_itself() {
        let avail = vs(&["1.2.0", "1.2.5"]);
        let sel = Selector::parse("1.2.0").unwrap();
        assert_eq!(highest_matching(&avail, &[sel]).cloned(), Some(v("1.2.0")));

        let sel = Selector::parse("1.2.1").unwrap();
        assert_eq!(highest_matching(&avail, &[sel]).cloned(), None);
    }

    #[test]
    fn test_job017_selector_no_match_is_none() {
        let avail = vs(&["1.2.0", "1.2.10", "1.3.2"]);
        let sel = Selector::parse("3.x").unwrap();
        assert_eq!(highest_matching(&avail, &[sel]).cloned(), None);
    }

    #[test]
    fn test_job017_semver_order_is_numeric_not_lexical() {
        // 1.2.10 > 1.2.5 numerically (string sort would pick 1.2.5).
        let avail = vs(&["1.2.5", "1.2.10"]);
        let sel = Selector::parse("1.2.x").unwrap();
        assert_eq!(highest_matching(&avail, &[sel]).cloned(), Some(v("1.2.10")));
    }

    // --- selector notation limits (§83: no range operators) ---

    #[test]
    fn test_job017_selector_range_operators_rejected() {
        for bad in ["^1.2.0", "~1.2.0", ">=1.2.0", "<2", "1.2.0 - 1.3.0", "*", "1.*", "1.2.*"] {
            assert!(
                Selector::parse(bad).is_err(),
                "expected '{}' to be rejected",
                bad
            );
        }
    }

    #[test]
    fn test_job017_selector_incomplete_forms_rejected() {
        for bad in ["1", "1.2", "x.2.3", "1.x.0", "X", "1.X", "1.2.X", ""] {
            assert!(
                Selector::parse(bad).is_err(),
                "expected '{}' to be rejected",
                bad
            );
        }
    }

    #[test]
    fn test_job017_selector_display_roundtrip() {
        for s in ["1.2.0", "1.2.x", "1.x", "x"] {
            let sel = Selector::parse(s).unwrap();
            assert_eq!(sel.to_string(), s);
        }
    }

    #[test]
    fn test_job017_version_invalid_forms() {
        for bad in ["1.2", "1.2.3.4", "1.2.x", "abc", "01.2.3", "1.02.3", "-1.2.3", ""] {
            assert!(
                bad.parse::<Version>().is_err(),
                "expected '{}' to be rejected",
                bad
            );
        }
        assert_eq!("1.2.3".parse::<Version>().unwrap(), Version::new(1, 2, 3));
    }

    #[test]
    fn test_job017_intersection_of_selectors() {
        // root requires "1.x", a package requires "1.2.5" → shared version.
        let avail = vs(&["1.2.5", "1.3.2"]);
        let sels = vec![
            Selector::parse("1.x").unwrap(),
            Selector::parse("1.2.5").unwrap(),
        ];
        assert_eq!(highest_matching(&avail, &sels).cloned(), Some(v("1.2.5")));
    }
}

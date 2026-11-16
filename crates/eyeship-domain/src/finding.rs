//! Findings produced by rules, with their severity and source location.

use std::fmt;

/// How serious a finding is. Ordered from the least to the most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Severity {
    /// Good practice or style.
    Info,
    /// Real risk in production.
    Warning,
    /// Very likely crash or invalid configuration.
    Error,
}

impl Severity {
    /// Lowercase name, as shown to users and in JSON.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for Severity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Index of a source file in [`crate::Project::files`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct FileId(pub u32);

/// A position in a source file. Lines and columns are 1-indexed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Location {
    pub file: FileId,
    pub line: u32,
    pub column: u32,
}

impl Location {
    pub const fn new(file: FileId, line: u32, column: u32) -> Self {
        Self { file, line, column }
    }
}

/// A value together with the place it was written.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Located<T> {
    pub value: T,
    pub location: Location,
}

impl<T> Located<T> {
    pub const fn new(value: T, location: Location) -> Self {
        Self { value, location }
    }

    /// Transforms the value, keeping the location.
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Located<U> {
        Located::new(f(self.value), self.location)
    }
}

/// Stable identifier of a rule, such as `R001`. Never renumbered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RuleId(pub &'static str);

impl RuleId {
    pub const fn as_str(self) -> &'static str {
        self.0
    }
}

impl fmt::Display for RuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0)
    }
}

/// A risk found in a project.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Finding {
    pub rule: RuleId,
    pub severity: Severity,
    /// Service concerned, if any.
    pub service: Option<String>,
    pub location: Location,
    /// Short English explanation of the risk.
    pub message: String,
    /// Proposed fix.
    pub fix: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn severities_are_ordered_from_info_to_error() {
        assert!(Severity::Info < Severity::Warning);
        assert!(Severity::Warning < Severity::Error);
    }

    #[test]
    fn severities_display_in_lowercase() {
        let shown: Vec<String> = [Severity::Info, Severity::Warning, Severity::Error]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(shown, ["info", "warning", "error"]);
    }

    #[test]
    fn rule_ids_display_as_written() {
        assert_eq!(RuleId("R001").to_string(), "R001");
    }

    #[test]
    fn locations_order_by_file_then_line_then_column() {
        let a = Location::new(FileId(0), 3, 9);
        let b = Location::new(FileId(0), 4, 1);
        let c = Location::new(FileId(1), 1, 1);
        assert!(a < b && b < c);
    }

    #[test]
    fn located_map_keeps_the_location() {
        let location = Location::new(FileId(0), 2, 5);
        let mapped = Located::new("512", location).map(str::len);
        assert_eq!(mapped, Located::new(3, location));
    }
}

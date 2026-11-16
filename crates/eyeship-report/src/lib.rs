//! Output adapters: render an [`eyeship_domain`] report as text or JSON.
#![forbid(unsafe_code)]

mod catalog;
mod json;
mod text;

#[cfg(test)]
mod fixtures;

use std::io::{self, Write};

use eyeship_domain::{FileId, Report};

pub use catalog::{render_explain, render_rules};
pub use json::render_json;
pub use text::{render_text, should_color};

/// Output format of an audit report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable text, with ANSI colors when `color` is true.
    Text { color: bool },
    /// Versioned JSON, see `docs/specs/001-audit-compose/plan.md` §5.
    Json,
}

/// Writes `report` to `out` in the chosen format.
pub fn render(
    report: &Report,
    format: Format,
    tool_version: &str,
    out: &mut dyn Write,
) -> io::Result<()> {
    match format {
        Format::Text { color } => render_text(report, color, out),
        Format::Json => render_json(report, tool_version, out),
    }
}

/// Name of a source file, or `<unknown>` for an out-of-range ID.
fn file_name(files: &[String], file: FileId) -> &str {
    usize::try_from(file.0)
        .ok()
        .and_then(|index| files.get(index))
        .map_or("<unknown>", String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_report;

    fn rendered(format: Format) -> String {
        let mut out = Vec::new();
        render(&sample_report(), format, "0.1.0", &mut out).expect("writing to a Vec never fails");
        String::from_utf8(out).expect("output is UTF-8")
    }

    #[test]
    fn dispatches_to_the_text_renderer() {
        assert!(rendered(Format::Text { color: false }).contains("fix: "));
    }

    #[test]
    fn dispatches_to_the_json_renderer() {
        assert!(rendered(Format::Json).starts_with('{'));
    }
}

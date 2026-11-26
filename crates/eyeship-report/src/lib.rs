//! Output adapters: render an [`eyeship_domain`] report as text, JSON or SARIF.
#![forbid(unsafe_code)]

mod catalog;
pub mod html;
mod json;
mod sarif;
mod text;
mod view;

#[cfg(test)]
mod fixtures;

use std::io::{self, Write};

use eyeship_domain::{FileId, Report};

pub use catalog::{render_explain, render_rules};
pub use json::render_json;
pub use sarif::{SarifRecord, SavedContext, fail_on_name, fingerprint, parse_sarif, render_sarif};
pub use text::{render_text, should_color};
pub use view::{FindingView, ReportView, Status};

/// Output format of an audit report.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// Human-readable text, with ANSI colors when `color` is true.
    Text { color: bool },
    /// Versioned JSON, see `docs/specs/001-audit-compose/plan.md` §5.
    Json,
    /// SARIF 2.1.0, see `docs/specs/004-interface-web/plan.md` §4.2.
    Sarif,
    /// A self-contained HTML page (spec 004 US6).
    Html,
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
        Format::Sarif => render_sarif(report, tool_version, None, out),
        Format::Html => out.write_all(html::standalone_report(report).as_bytes()),
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

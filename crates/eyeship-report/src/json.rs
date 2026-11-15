//! Versioned JSON output.

use std::io::{self, Write};

use eyeship_domain::{Finding, Report};
use serde::Serialize;

/// Version of the JSON schema. Bumped on any incompatible change.
const SCHEMA_VERSION: u32 = 1;
const TOOL_NAME: &str = "eyeship";

#[derive(Serialize)]
struct JsonReport<'a> {
    schema_version: u32,
    tool: Tool<'a>,
    files: &'a [String],
    findings: Vec<JsonFinding<'a>>,
    ignored: usize,
    summary: JsonSummary,
}

#[derive(Serialize)]
struct Tool<'a> {
    name: &'a str,
    version: &'a str,
}

#[derive(Serialize)]
struct JsonFinding<'a> {
    rule: &'a str,
    severity: &'a str,
    service: Option<&'a str>,
    location: JsonLocation<'a>,
    message: &'a str,
    fix: &'a str,
}

#[derive(Serialize)]
struct JsonLocation<'a> {
    file: &'a str,
    line: u32,
    column: u32,
}

#[derive(Serialize)]
struct JsonSummary {
    error: usize,
    warning: usize,
    info: usize,
}

/// Writes `report` as JSON (schema version 1), followed by a newline.
pub fn render_json(report: &Report, tool_version: &str, out: &mut dyn Write) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut *out, &to_dto(report, tool_version))?;
    out.write_all(b"\n")
}

fn to_dto<'a>(report: &'a Report, tool_version: &'a str) -> JsonReport<'a> {
    let summary = report.summary();
    JsonReport {
        schema_version: SCHEMA_VERSION,
        tool: Tool {
            name: TOOL_NAME,
            version: tool_version,
        },
        files: &report.files,
        findings: report
            .findings
            .iter()
            .map(|finding| finding_dto(&report.files, finding))
            .collect(),
        ignored: report.ignored.len(),
        summary: JsonSummary {
            error: summary.error,
            warning: summary.warning,
            info: summary.info,
        },
    }
}

fn finding_dto<'a>(files: &'a [String], finding: &'a Finding) -> JsonFinding<'a> {
    let location = finding.location;
    JsonFinding {
        rule: finding.rule.as_str(),
        severity: finding.severity.as_str(),
        service: finding.service.as_deref(),
        location: JsonLocation {
            file: crate::file_name(files, location.file),
            line: location.line,
            column: location.column,
        },
        message: &finding.message,
        fix: &finding.fix,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_report;

    fn json(report: &Report) -> String {
        let mut out = Vec::new();
        render_json(report, "0.1.0", &mut out).expect("writing to a Vec never fails");
        String::from_utf8(out).expect("output is UTF-8")
    }

    #[test]
    fn renders_the_versioned_schema() {
        insta::assert_snapshot!(json(&sample_report()));
    }

    #[test]
    fn output_is_valid_json_ending_with_a_newline() {
        let output = json(&Report::default());
        assert!(output.ends_with("}\n"));
        let value: serde_json::Value = serde_json::from_str(&output).expect("valid JSON");
        assert_eq!(value["schema_version"], 1);
        assert_eq!(value["tool"]["version"], "0.1.0");
        assert_eq!(value["findings"], serde_json::json!([]));
    }

    #[test]
    fn a_project_finding_has_a_null_service() {
        let value: serde_json::Value =
            serde_json::from_str(&json(&sample_report())).expect("valid JSON");
        assert!(value["findings"][3]["service"].is_null());
    }
}

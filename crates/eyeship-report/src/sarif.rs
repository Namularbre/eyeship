//! SARIF 2.1.0 output and input (spec 004 US10, plan §4.2): the standard
//! exchange format of static analysers, also used for saved reports.
//!
//! Only the subset eyeship writes is read back.

use std::io::{self, Write};

use eyeship_domain::{FailOn, Finding, Report, Rule, Severity, rules};
use serde::{Deserialize, Serialize};

use crate::view::{FindingView, ReportView};

const SCHEMA: &str = "https://json.schemastore.org/sarif-2.1.0.json";
const VERSION: &str = "2.1.0";
const TOOL_NAME: &str = "eyeship";
const INFORMATION_URI: &str = "https://github.com/Namularbre/eyeship";
const FINGERPRINT: &str = "eyeship/v1";
const IGNORED_JUSTIFICATION: &str = "ignored by eyeship configuration";

/// Where and when a report was saved: added by `--save` only, so that the
/// standard output stays deterministic (constitution P4).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedContext {
    pub deployment: String,
    /// Identifier of the saved report, unique within its deployment.
    pub id: String,
    /// RFC 3339 UTC date of the save.
    pub saved_at: String,
    pub label: Option<String>,
    pub fail_on: FailOn,
}

/// A SARIF document read back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SarifRecord {
    pub tool_version: String,
    pub report: ReportView,
    pub context: Option<SavedContext>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Log {
    #[serde(rename = "$schema", default, skip_serializing_if = "Option::is_none")]
    schema: Option<String>,
    version: String,
    runs: Vec<Run>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Run {
    tool: Tool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    automation_details: Option<AutomationDetails>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    invocations: Vec<Invocation>,
    #[serde(default)]
    artifacts: Vec<Artifact>,
    #[serde(default)]
    results: Vec<SarifResult>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    properties: Option<RunProperties>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Tool {
    driver: Driver,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Driver {
    name: String,
    #[serde(default)]
    version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    information_uri: Option<String>,
    #[serde(default)]
    rules: Vec<ReportingDescriptor>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ReportingDescriptor {
    id: String,
    short_description: Text,
    full_description: Text,
    help: Text,
    default_configuration: Configuration,
}

#[derive(Debug, Serialize, Deserialize)]
struct Text {
    text: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Configuration {
    level: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct AutomationDetails {
    id: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Invocation {
    execution_successful: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    end_time_utc: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RunProperties {
    deployment: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    label: Option<String>,
    fail_on: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Artifact {
    location: ArtifactLocation,
}

#[derive(Debug, Serialize, Deserialize)]
struct ArtifactLocation {
    uri: String,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SarifResult {
    rule_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    rule_index: Option<usize>,
    #[serde(default)]
    level: Option<String>,
    message: Text,
    #[serde(default)]
    locations: Vec<ResultLocation>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    suppressions: Vec<Suppression>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    partial_fingerprints: Option<std::collections::BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    properties: Option<ResultProperties>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ResultLocation {
    physical_location: PhysicalLocation,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    logical_locations: Vec<LogicalLocation>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PhysicalLocation {
    artifact_location: ArtifactLocation,
    region: Region,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Region {
    start_line: u32,
    start_column: u32,
}

#[derive(Debug, Serialize, Deserialize)]
struct LogicalLocation {
    name: String,
    kind: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct Suppression {
    kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    justification: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
struct ResultProperties {
    fix: String,
}

/// Writes `report` as a SARIF 2.1.0 document, followed by a newline.
pub fn render_sarif(
    report: &Report,
    tool_version: &str,
    context: Option<&SavedContext>,
    out: &mut dyn Write,
) -> io::Result<()> {
    serde_json::to_writer_pretty(&mut *out, &to_log(report, tool_version, context))?;
    out.write_all(b"\n")
}

fn to_log(report: &Report, tool_version: &str, context: Option<&SavedContext>) -> Log {
    let triggered = triggered_rules(report);
    let rule_index = |id: &str| triggered.iter().position(|rule| rule.id.as_str() == id);
    let result = |finding: &Finding, ignored: bool| SarifResult {
        rule_id: finding.rule.as_str().to_owned(),
        rule_index: rule_index(finding.rule.as_str()),
        level: Some(level(finding.severity).to_owned()),
        message: Text {
            text: finding.message.clone(),
        },
        locations: vec![ResultLocation {
            physical_location: PhysicalLocation {
                artifact_location: ArtifactLocation {
                    uri: to_uri(crate::file_name(&report.files, finding.location.file)),
                },
                region: Region {
                    start_line: finding.location.line.max(1),
                    start_column: finding.location.column.max(1),
                },
            },
            logical_locations: finding
                .service
                .iter()
                .map(|service| LogicalLocation {
                    name: service.clone(),
                    kind: "module".into(),
                })
                .collect(),
        }],
        suppressions: if ignored {
            vec![Suppression {
                kind: "external".into(),
                justification: Some(IGNORED_JUSTIFICATION.into()),
            }]
        } else {
            Vec::new()
        },
        partial_fingerprints: Some(
            [(
                FINGERPRINT.to_owned(),
                fingerprint(
                    finding.rule.as_str(),
                    finding.service.as_deref(),
                    &finding.message,
                ),
            )]
            .into(),
        ),
        properties: Some(ResultProperties {
            fix: finding.fix.clone(),
        }),
    };
    Log {
        schema: Some(SCHEMA.into()),
        version: VERSION.into(),
        runs: vec![Run {
            tool: Tool {
                driver: Driver {
                    name: TOOL_NAME.into(),
                    version: tool_version.into(),
                    information_uri: Some(INFORMATION_URI.into()),
                    rules: triggered.iter().map(|rule| descriptor(rule)).collect(),
                },
            },
            automation_details: context.map(|context| AutomationDetails {
                id: format!("{}/{}", context.deployment, context.id),
            }),
            invocations: context
                .map(|context| Invocation {
                    execution_successful: true,
                    end_time_utc: Some(context.saved_at.clone()),
                })
                .into_iter()
                .collect(),
            artifacts: report
                .files
                .iter()
                .map(|file| Artifact {
                    location: ArtifactLocation { uri: to_uri(file) },
                })
                .collect(),
            results: report
                .findings
                .iter()
                .map(|finding| result(finding, false))
                .chain(report.ignored.iter().map(|finding| result(finding, true)))
                .collect(),
            properties: context.map(|context| RunProperties {
                deployment: context.deployment.clone(),
                label: context.label.clone(),
                fail_on: fail_on_name(context.fail_on).into(),
            }),
        }],
    }
}

/// Rules with at least one finding (retained or ignored), sorted by ID.
fn triggered_rules(report: &Report) -> Vec<&'static Rule> {
    let mut triggered: Vec<&'static Rule> = report
        .findings
        .iter()
        .chain(&report.ignored)
        .filter_map(|finding| rules::find(finding.rule.as_str()))
        .collect();
    triggered.sort_by_key(|rule| rule.id.as_str());
    triggered.dedup_by_key(|rule| rule.id.as_str());
    triggered
}

fn descriptor(rule: &Rule) -> ReportingDescriptor {
    ReportingDescriptor {
        id: rule.id.as_str().into(),
        short_description: Text {
            text: rule.title.into(),
        },
        full_description: Text {
            text: rule.doc.scenario.into(),
        },
        help: Text {
            text: rule.doc.fix.into(),
        },
        default_configuration: Configuration {
            level: level(rule.severity).into(),
        },
    }
}

const fn level(severity: Severity) -> &'static str {
    match severity {
        Severity::Error => "error",
        Severity::Warning => "warning",
        Severity::Info => "note",
    }
}

fn severity(level: Option<&str>) -> Result<Severity, String> {
    match level {
        Some("error") => Ok(Severity::Error),
        Some("warning") | None => Ok(Severity::Warning),
        Some("note") => Ok(Severity::Info),
        Some(other) => Err(format!("unsupported SARIF level '{other}'")),
    }
}

/// Name of a threshold, as written in `.eyeship.toml`.
pub const fn fail_on_name(fail_on: FailOn) -> &'static str {
    match fail_on {
        FailOn::Error => "error",
        FailOn::Warning => "warning",
        FailOn::Info => "info",
        FailOn::None => "none",
    }
}

/// A relative path as a URI reference: forward slashes, escaped specials.
fn to_uri(path: &str) -> String {
    path.replace('\\', "/")
        .chars()
        .map(|c| match c {
            ' ' => "%20".to_owned(),
            '%' => "%25".to_owned(),
            '#' => "%23".to_owned(),
            '?' => "%3F".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

fn from_uri(uri: &str) -> String {
    uri.replace("%20", " ")
        .replace("%23", "#")
        .replace("%3F", "?")
        .replace("%25", "%")
}

/// Stable 64-bit FNV-1a fingerprint of a finding's identity, in hex.
/// Written by hand: `DefaultHasher` may change between Rust versions.
pub fn fingerprint(rule: &str, service: Option<&str>, message: &str) -> String {
    const OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const PRIME: u64 = 0x0100_0000_01b3;
    let hash = [rule, service.unwrap_or(""), message]
        .iter()
        .flat_map(|part| part.bytes().chain(std::iter::once(0)))
        .fold(OFFSET, |hash, byte| {
            (hash ^ u64::from(byte)).wrapping_mul(PRIME)
        });
    format!("{hash:016x}")
}

/// Reads a SARIF document written by eyeship.
pub fn parse_sarif(text: &str) -> Result<SarifRecord, String> {
    let log: Log = serde_json::from_str(text).map_err(|error| format!("invalid SARIF: {error}"))?;
    if log.version != VERSION {
        return Err(format!("unsupported SARIF version '{}'", log.version));
    }
    let [run] = <[Run; 1]>::try_from(log.runs)
        .map_err(|runs| format!("expected one run, found {}", runs.len()))?;
    if run.tool.driver.name != TOOL_NAME {
        return Err(format!(
            "not an eyeship report (tool '{}')",
            run.tool.driver.name
        ));
    }
    let (ignored, findings): (Vec<_>, Vec<_>) = run
        .results
        .iter()
        .map(|result| finding_view(result).map(|view| (!result.suppressions.is_empty(), view)))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .partition(|(ignored, _)| *ignored);
    let context = match (run.properties, run.automation_details) {
        (Some(properties), Some(details)) => Some(SavedContext {
            id: details
                .id
                .rsplit_once('/')
                .map_or(details.id.as_str(), |(_, id)| id)
                .to_owned(),
            deployment: properties.deployment,
            saved_at: run
                .invocations
                .first()
                .and_then(|invocation| invocation.end_time_utc.clone())
                .unwrap_or_default(),
            label: properties.label,
            fail_on: properties.fail_on.parse()?,
        }),
        _ => None,
    };
    Ok(SarifRecord {
        tool_version: run.tool.driver.version,
        report: ReportView {
            files: run
                .artifacts
                .iter()
                .map(|artifact| from_uri(&artifact.location.uri))
                .collect(),
            findings: findings.into_iter().map(|(_, view)| view).collect(),
            ignored: ignored.into_iter().map(|(_, view)| view).collect(),
        },
        context,
    })
}

fn finding_view(result: &SarifResult) -> Result<FindingView, String> {
    let location = result.locations.first();
    let physical = location.map(|location| &location.physical_location);
    Ok(FindingView {
        rule: result.rule_id.clone(),
        severity: severity(result.level.as_deref())?,
        service: location
            .and_then(|location| location.logical_locations.first())
            .map(|logical| logical.name.clone()),
        file: physical.map_or_else(String::new, |physical| {
            from_uri(&physical.artifact_location.uri)
        }),
        line: physical.map_or(0, |physical| physical.region.start_line),
        column: physical.map_or(0, |physical| physical.region.start_column),
        message: result.message.text.clone(),
        fix: result
            .properties
            .as_ref()
            .map(|properties| properties.fix.clone())
            .unwrap_or_default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_report;
    use eyeship_domain::{FileId, Location, RuleId};

    fn sarif(report: &Report, context: Option<&SavedContext>) -> String {
        let mut out = Vec::new();
        render_sarif(report, "0.1.0", context, &mut out).expect("writing to a Vec never fails");
        String::from_utf8(out).expect("output is UTF-8")
    }

    /// A report on real rules, so that their documentation is embedded.
    fn real_report() -> Report {
        let finding = |rule: &'static str, severity, service: Option<&str>, line| Finding {
            rule: RuleId(rule),
            severity,
            service: service.map(String::from),
            location: Location::new(FileId(0), line, 7),
            message: format!("message of {rule}"),
            fix: format!("fix of {rule}"),
        };
        Report {
            files: vec!["infra/compose prod.yaml".into()],
            findings: vec![
                finding("R009", Severity::Error, Some("web"), 12),
                finding("R001", Severity::Warning, Some("api"), 4),
                finding("R017", Severity::Info, None, 1),
            ],
            ignored: vec![finding("R006", Severity::Warning, Some("api"), 3)],
        }
    }

    fn context() -> SavedContext {
        SavedContext {
            deployment: "prod".into(),
            id: "2026-11-26T11-47-17.123Z".into(),
            saved_at: "2026-11-26T11:47:17.123Z".into(),
            label: Some("a1b2c3d".into()),
            fail_on: FailOn::Warning,
        }
    }

    #[test]
    fn renders_sarif_2_1_0() {
        insta::assert_snapshot!(sarif(&real_report(), None));
    }

    #[test]
    fn renders_the_saved_context() {
        insta::assert_snapshot!(sarif(&real_report(), Some(&context())));
    }

    #[test]
    fn maps_levels_services_and_suppressions() {
        let value: serde_json::Value =
            serde_json::from_str(&sarif(&real_report(), None)).expect("valid JSON");
        let results = &value["runs"][0]["results"];
        assert_eq!(results[0]["level"], "error");
        assert_eq!(results[2]["level"], "note");
        assert_eq!(
            results[0]["locations"][0]["logicalLocations"][0]["name"],
            "web"
        );
        assert!(results[2]["locations"][0].get("logicalLocations").is_none());
        assert!(results[0].get("suppressions").is_none());
        assert_eq!(results[3]["suppressions"][0]["kind"], "external");
        assert_eq!(
            results[0]["locations"][0]["physicalLocation"]["artifactLocation"]["uri"],
            "infra/compose%20prod.yaml"
        );
        let rules = &value["runs"][0]["tool"]["driver"]["rules"];
        let ids: Vec<&str> = rules
            .as_array()
            .expect("rules")
            .iter()
            .filter_map(|rule| rule["id"].as_str())
            .collect();
        assert_eq!(ids, ["R001", "R006", "R009", "R017"]);
        assert_eq!(results[0]["ruleIndex"], 2);
    }

    #[test]
    fn standard_output_has_no_date() {
        let output = sarif(&real_report(), None);
        assert!(!output.contains("invocations"));
        assert!(!output.contains("automationDetails"));
        assert_eq!(output, sarif(&real_report(), None));
    }

    #[test]
    fn reads_back_what_it_writes() {
        for report in [real_report(), sample_report(), Report::default()] {
            let record = parse_sarif(&sarif(&report, Some(&context()))).expect("readable");
            assert_eq!(record.report, ReportView::from(&report));
            assert_eq!(record.context, Some(context()));
            assert_eq!(record.tool_version, "0.1.0");
            let plain = parse_sarif(&sarif(&report, None)).expect("readable");
            assert_eq!(plain.context, None);
        }
    }

    #[test]
    fn rejects_foreign_or_invalid_documents() {
        assert!(parse_sarif("{").is_err());
        let foreign = sarif(&Report::default(), None).replace("\"eyeship\"", "\"trivy\"");
        assert!(parse_sarif(&foreign).is_err_and(|e| e.contains("trivy")));
        let old = sarif(&Report::default(), None).replace("\"2.1.0\"", "\"2.0.0\"");
        assert!(parse_sarif(&old).is_err());
    }

    #[test]
    fn fingerprints_are_stable() {
        assert_eq!(fingerprint("", None, ""), "d94d12186c0f2fb7");
        let a = fingerprint("R001", Some("api"), "m");
        assert_eq!(a, fingerprint("R001", Some("api"), "m"));
        assert_ne!(a, fingerprint("R001", Some("ap"), "im"));
        assert_ne!(a, fingerprint("R001", None, "apim"));
    }
}

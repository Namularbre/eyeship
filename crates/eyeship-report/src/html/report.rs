//! The body of a report: summary, filters and findings grouped by service.
//! Shared by the live report, the saved reports and the standalone export.

use eyeship_domain::{Report, Rule, Severity, rules};

use super::{Mode, Nav, escape, layout, plural};
use crate::view::{FindingView, ReportView};

/// What the reader chose to see, carried by the page URL (spec 004 US3 §3).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Filter {
    /// Minimum severity shown.
    pub severity: Option<Severity>,
    pub service: Option<String>,
}

impl Filter {
    fn keeps(&self, finding: &FindingView) -> bool {
        self.severity
            .is_none_or(|minimum| finding.severity >= minimum)
            && self
                .service
                .as_deref()
                .is_none_or(|service| finding.service.as_deref() == Some(service))
    }

    fn is_active(&self) -> bool {
        self.severity.is_some() || self.service.is_some()
    }
}

/// Severity badge: a color **and** a word (spec 004 ENF6).
pub(crate) fn severity_badge(severity: Severity) -> String {
    format!("<span class=\"badge {0}\">{0}</span>", severity.as_str())
}

/// The counts of a report: per severity, then ignored.
pub(crate) fn counts(view: &ReportView) -> String {
    let summary = view.summary();
    let ignored = if view.ignored.is_empty() {
        String::new()
    } else {
        format!(
            "<li><span class=\"badge\">{} ignored by configuration</span></li>",
            view.ignored.len()
        )
    };
    format!(
        "<ul class=\"counts\" aria-label=\"Findings per severity\">\
         <li><span class=\"badge error\">{}</span></li>\
         <li><span class=\"badge warning\">{}</span></li>\
         <li><span class=\"badge info\">{}</span></li>{ignored}</ul>",
        plural(summary.error, "error", "errors"),
        plural(summary.warning, "warning", "warnings"),
        plural(summary.info, "info", "info"),
    )
}

fn filters_form(view: &ReportView, filter: &Filter) -> String {
    let option = |value: &str, label: &str, selected: bool| {
        format!(
            "<option value=\"{}\"{}>{}</option>",
            escape(value),
            if selected { " selected" } else { "" },
            escape(label)
        )
    };
    let severities: String = [
        ("", "All", None),
        ("warning", "Warnings and errors", Some(Severity::Warning)),
        ("error", "Errors only", Some(Severity::Error)),
    ]
    .iter()
    .map(|(value, label, severity)| option(value, label, filter.severity == *severity))
    .collect();
    let services: String =
        std::iter::once(option("", "All", filter.service.is_none()))
            .chain(view.services().into_iter().map(|service| {
                option(service, service, filter.service.as_deref() == Some(service))
            }))
            .collect();
    format!(
        "<form class=\"filters\" method=\"get\" role=\"search\" aria-label=\"Filter findings\">\
         <label>Severity<select name=\"severity\">{severities}</select></label>\
         <label>Service<select name=\"service\">{services}</select></label>\
         <button type=\"submit\">Filter</button>{}</form>",
        if filter.is_active() {
            " <a href=\"?\">Clear filters</a>"
        } else {
            ""
        }
    )
}

/// Link to a rule's explanation: its page when served, an anchor when standalone.
fn rule_link(rule: &str, mode: Mode) -> String {
    let rule = escape(rule);
    match mode {
        Mode::Served => format!("<a href=\"/rules/{rule}\"><code>{rule}</code></a>"),
        Mode::Standalone => format!("<a href=\"#rule-{rule}\"><code>{rule}</code></a>"),
    }
}

pub(crate) fn finding_item(finding: &FindingView, mode: Mode) -> String {
    format!(
        "<li class=\"finding\"><div class=\"head\">{} {} <span class=\"where mono\">{}:{}:{}</span></div>\
         <p>{}</p><p class=\"fix\">Fix: {}</p></li>",
        severity_badge(finding.severity),
        rule_link(&finding.rule, mode),
        escape(&finding.file),
        finding.line,
        finding.column,
        escape(&finding.message),
        escape(&finding.fix),
    )
}

/// Project-level findings first, then services in order of appearance,
/// like the text output.
fn groups<'a>(findings: &[&'a FindingView]) -> Vec<Option<&'a str>> {
    let mut services: Vec<Option<&str>> = Vec::new();
    findings
        .iter()
        .map(|finding| finding.service.as_deref())
        .for_each(|service| {
            if !services.contains(&service) {
                services.push(service);
            }
        });
    services.sort_by_key(Option::is_some);
    services
}

/// Summary, filters (when served) and findings grouped by service.
pub(crate) fn report_section(view: &ReportView, filter: &Filter, mode: Mode) -> String {
    let shown: Vec<&FindingView> = view
        .findings
        .iter()
        .filter(|finding| filter.keeps(finding))
        .collect();
    let form = match mode {
        Mode::Served if !view.findings.is_empty() => filters_form(view, filter),
        _ => String::new(),
    };
    let body = if view.findings.is_empty() {
        "<p class=\"empty\">No issues found.</p>".to_owned()
    } else if shown.is_empty() {
        "<p class=\"empty\">No finding matches these filters.</p>".to_owned()
    } else {
        groups(&shown)
            .into_iter()
            .map(|service| {
                let items: String = shown
                    .iter()
                    .filter(|finding| finding.service.as_deref() == service)
                    .map(|finding| finding_item(finding, mode))
                    .collect();
                let title = service.map_or_else(
                    || "Project".to_owned(),
                    |service| format!("Service <code>{}</code>", escape(service)),
                );
                format!("<h3>{title}</h3><ul class=\"findings\">{items}</ul>")
            })
            .collect()
    };
    format!("{}{form}{body}", counts(view))
}

/// Documentation of a rule, as `eyeship explain` shows it.
pub(crate) fn rule_doc(rule: &Rule) -> String {
    format!(
        "<p>Default severity: {}</p>\
         <h3>Failure scenario</h3><p>{}</p>\
         <h3>Fix</h3><p>{}</p>\
         <h3>Bad example</h3><pre><code>{}</code></pre>\
         <h3>Good example</h3><pre><code>{}</code></pre>",
        severity_badge(rule.severity),
        escape(rule.doc.scenario),
        escape(rule.doc.fix),
        escape(rule.doc.bad_example),
        escape(rule.doc.good_example),
    )
}

/// A self-contained HTML page of a report, for `--format html` (spec 004 US6):
/// inline style, no link to a server, the triggered rules explained at the end.
pub fn standalone_report(report: &Report) -> String {
    let view = ReportView::from(report);
    let mut triggered: Vec<&Rule> = view
        .findings
        .iter()
        .chain(&view.ignored)
        .filter_map(|finding| rules::find(&finding.rule))
        .collect();
    triggered.sort_by_key(|rule| rule.id.as_str());
    triggered.dedup_by_key(|rule| rule.id.as_str());
    let explanations: String = triggered
        .iter()
        .map(|rule| {
            format!(
                "<details id=\"rule-{0}\"><summary><code>{0}</code> {1}</summary>{2}</details>",
                escape(rule.id.as_str()),
                escape(rule.title),
                rule_doc(rule)
            )
        })
        .collect();
    let rules_section = if explanations.is_empty() {
        String::new()
    } else {
        format!("<h2>Rules</h2>{explanations}")
    };
    let files = view
        .files
        .iter()
        .map(|file| format!("<code>{}</code>", escape(file)))
        .collect::<Vec<_>>()
        .join(", ");
    let body = format!(
        "<h1>Audit report</h1><p class=\"sub\">{files}</p>{}{rules_section}",
        report_section(&view, &Filter::default(), Mode::Standalone)
    );
    layout("Audit report", Nav::None, Mode::Standalone, &body)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_report;

    fn view() -> ReportView {
        ReportView::from(&sample_report())
    }

    #[test]
    fn groups_project_findings_first() {
        let section = report_section(&view(), &Filter::default(), Mode::Served);
        let project = section.find("<h3>Project</h3>").expect("project group");
        let api = section.find("Service <code>api</code>").expect("api group");
        let db = section.find("Service <code>db</code>").expect("db group");
        assert!(project < api && api < db);
        assert!(section.contains("2 ignored by configuration"));
    }

    #[test]
    fn filters_by_minimum_severity_and_service() {
        let errors = Filter {
            severity: Some(Severity::Error),
            ..Filter::default()
        };
        let section = report_section(&view(), &errors, Mode::Served);
        assert!(section.contains("R005") && section.contains("R002"));
        assert!(!section.contains("/rules/R001") && !section.contains("/rules/R017"));
        let api = Filter {
            service: Some("api".into()),
            ..Filter::default()
        };
        let section = report_section(&view(), &api, Mode::Served);
        assert!(section.contains("/rules/R005") && section.contains("/rules/R001"));
        assert!(!section.contains("/rules/R002"));
        assert!(section.contains("Clear filters"));
        let nothing = Filter {
            service: Some("nope".into()),
            ..Filter::default()
        };
        assert!(report_section(&view(), &nothing, Mode::Served).contains("No finding matches"));
    }

    #[test]
    fn the_filter_form_keeps_the_selection() {
        let filter = Filter {
            severity: Some(Severity::Warning),
            service: Some("db".into()),
        };
        let section = report_section(&view(), &filter, Mode::Served);
        assert!(section.contains("<option value=\"warning\" selected>"));
        assert!(section.contains("<option value=\"db\" selected>"));
    }

    #[test]
    fn an_empty_report_says_so() {
        let section = report_section(&ReportView::default(), &Filter::default(), Mode::Served);
        assert!(section.contains("No issues found."));
        assert!(!section.contains("<form"));
    }

    #[test]
    fn the_standalone_report_is_self_contained() {
        let page = standalone_report(&sample_report());
        insta::assert_snapshot!(page);
        assert!(page.contains("<style>"));
        assert!(!page.contains("href=\"/"), "no link to a server");
        assert!(!page.contains("<form"));
        assert!(!page.contains("<script"));
        assert!(page.contains("id=\"rule-R001\""));
        assert_eq!(page, standalone_report(&sample_report()));
    }
}

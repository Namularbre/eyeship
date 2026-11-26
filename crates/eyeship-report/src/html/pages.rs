//! The pages of `eyeship ui` (spec 004 US2, US3, US4, US8, US9).

use eyeship_domain::{Diff, FailOn, Rule, Summary};

use super::chart::evolution;
use super::report::{Filter, counts, finding_item, report_section, rule_doc, severity_badge};
use super::{Mode, Nav, escape, layout, plural, short_date};
use crate::sarif::SavedContext;
use crate::view::{FindingView, ReportView, Status};

/// One deployment on the dashboard.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Card<'a> {
    pub name: &'a str,
    pub files: &'a [String],
    /// `None` for a deployment that only has a history (no declaration).
    pub live: Option<Result<(&'a ReportView, FailOn), &'a str>>,
    /// Date of the most recent saved report.
    pub last_saved: Option<&'a str>,
    /// Findings now minus findings in the most recent saved report.
    pub trend: Option<i64>,
}

/// One saved report in a deployment's history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryRow<'a> {
    pub id: &'a str,
    pub outcome: Result<(&'a SavedContext, &'a ReportView), &'a str>,
}

/// The page of a deployment: live report, then history.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentPage<'a> {
    pub name: &'a str,
    pub files: &'a [String],
    pub live: Option<Result<(&'a ReportView, FailOn), &'a str>>,
    pub filter: &'a Filter,
    /// Most recent first.
    pub history: &'a [HistoryRow<'a>],
}

/// A saved report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedPage<'a> {
    pub name: &'a str,
    pub context: &'a SavedContext,
    pub report: &'a ReportView,
    pub filter: &'a Filter,
}

/// Comparison of two reports of a deployment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComparePage<'a> {
    pub name: &'a str,
    /// How each side is shown, such as `saved 2026-11-25 09:12 UTC` or `live`.
    pub from: &'a str,
    pub to: &'a str,
    pub diff: Diff<'a, FindingView>,
}

fn status_badge(status: Status) -> &'static str {
    match status {
        Status::Pass => "<span class=\"badge pass\">ready</span>",
        Status::Fail => "<span class=\"badge fail\">failing</span>",
        Status::Unreadable => "<span class=\"badge unreadable\">unreadable</span>",
    }
}

fn status_of(live: Option<Result<(&ReportView, FailOn), &str>>) -> Option<Status> {
    live.map(|live| match live {
        Ok((report, fail_on)) => report.status(fail_on),
        Err(_) => Status::Unreadable,
    })
}

/// The threshold in words.
fn threshold(fail_on: FailOn) -> &'static str {
    match fail_on {
        FailOn::Error => "Fails on errors.",
        FailOn::Warning => "Fails on warnings and errors.",
        FailOn::Info => "Fails on any finding.",
        FailOn::None => "Never fails.",
    }
}

fn file_list(files: &[String]) -> String {
    files
        .iter()
        .map(|file| format!("<code>{}</code>", escape(file)))
        .collect::<Vec<_>>()
        .join(", ")
}

fn trend_text(trend: i64) -> String {
    match trend {
        0 => "no change since the last saved report".to_owned(),
        n if n > 0 => format!("▲ {n} more since the last saved report"),
        n => format!("▼ {} fewer since the last saved report", -n),
    }
}

fn total(summary: Summary) -> usize {
    summary.error + summary.warning + summary.info
}

/// Difference of total findings between two reports.
pub fn trend(now: &ReportView, before: &ReportView) -> i64 {
    let count = |view: &ReportView| i64::try_from(total(view.summary())).unwrap_or(i64::MAX);
    count(now) - count(before)
}

/// The home page: one card per deployment, failing first (spec 004 US2).
pub fn dashboard_page(cards: &[Card]) -> String {
    let items: String = cards
        .iter()
        .map(|card| {
            let name = escape(card.name);
            let status = status_of(card.live).map_or_else(
                || "<span class=\"badge\">history only</span>".to_owned(),
                |status| status_badge(status).to_owned(),
            );
            let detail = match card.live {
                Some(Ok((report, fail_on))) => format!(
                    "{}<p class=\"sub\">{}</p>",
                    counts(report),
                    threshold(fail_on)
                ),
                Some(Err(error)) => format!("<p class=\"notice error\">{}</p>", escape(error)),
                None => String::new(),
            };
            let saved = card.last_saved.map_or_else(String::new, |date| {
                format!(
                    "<p class=\"sub\">Last saved report: {}{}</p>",
                    escape(&short_date(date)),
                    card.trend
                        .map(|trend| format!(", {}", trend_text(trend)))
                        .unwrap_or_default()
                )
            });
            format!(
                "<li class=\"card\"><h2><a href=\"/deployments/{name}\">{name}</a> {status}</h2>\
                 {detail}{saved}<p class=\"files\">{}</p></li>",
                file_list(card.files)
            )
        })
        .collect();
    let body = if cards.is_empty() {
        "<p class=\"empty\">No deployment.</p>".to_owned()
    } else {
        format!("<ul class=\"cards\">{items}</ul>")
    };
    let ready = cards
        .iter()
        .filter(|card| status_of(card.live) == Some(Status::Pass))
        .count();
    layout(
        "Deployments",
        Nav::Deployments,
        Mode::Served,
        &format!(
            "<h1>Deployments</h1><p class=\"sub\">{ready} of {} ready for production.</p>{body}",
            plural(cards.len(), "deployment", "deployments")
        ),
    )
}

fn history_section(name: &str, history: &[HistoryRow], live: bool) -> String {
    if history.is_empty() {
        return format!(
            "<h2>History</h2><p class=\"empty\">No saved report. Save one with \
             <code>eyeship compose --deployment {} --save</code>.</p>",
            escape(name)
        );
    }
    let chronological: Vec<Summary> = history
        .iter()
        .rev()
        .filter_map(|row| row.outcome.ok())
        .map(|(_, report)| report.summary())
        .collect();
    let chart = evolution(&chronological).unwrap_or_default();
    let rows: String = history
        .iter()
        .map(|row| match row.outcome {
            Ok((context, report)) => {
                let summary = report.summary();
                format!(
                    "<tr><td><a href=\"/deployments/{}/reports/{}\">{}</a></td><td>{}</td>\
                     <td>{}</td><td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>",
                    escape(name),
                    escape(row.id),
                    escape(&short_date(&context.saved_at)),
                    context
                        .label
                        .as_deref()
                        .map(|label| format!("<code>{}</code>", escape(label)))
                        .unwrap_or_default(),
                    status_badge(report.status(context.fail_on)),
                    summary.error,
                    summary.warning,
                    summary.info,
                    report.ignored.len()
                )
            }
            Err(error) => format!(
                "<tr><td><code>{}</code></td><td colspan=\"6\">{} {}</td></tr>",
                escape(row.id),
                status_badge(Status::Unreadable),
                escape(error)
            ),
        })
        .collect();
    let option = |value: &str, label: &str, selected: bool| {
        format!(
            "<option value=\"{}\"{}>{}</option>",
            escape(value),
            if selected { " selected" } else { "" },
            escape(label)
        )
    };
    let readable: Vec<(&str, &SavedContext)> = history
        .iter()
        .filter_map(|row| row.outcome.ok().map(|(context, _)| (row.id, context)))
        .collect();
    let choices = |selected: usize| -> String {
        readable
            .iter()
            .enumerate()
            .map(|(index, (id, context))| {
                option(id, &short_date(&context.saved_at), index == selected)
            })
            .collect()
    };
    let compare = match (live, readable.len()) {
        (true, 1..) | (false, 2..) => format!(
            "<form class=\"filters\" method=\"get\" action=\"/deployments/{}/compare\" \
             aria-label=\"Compare two reports\">\
             <label>From<select name=\"from\">{}</select></label>\
             <label>To<select name=\"to\">{}{}</select></label>\
             <button type=\"submit\">Compare</button></form>",
            escape(name),
            choices(0),
            if live {
                option("live", "Live", true)
            } else {
                String::new()
            },
            choices(if live { usize::MAX } else { 1 }),
        ),
        _ => String::new(),
    };
    format!(
        "<h2>History</h2>{chart}{compare}<div class=\"table-wrap\"><table>\
         <caption class=\"skip\">Saved reports, most recent first</caption>\
         <thead><tr><th scope=\"col\">Saved</th><th scope=\"col\">Label</th><th scope=\"col\">Status</th>\
         <th scope=\"col\">Errors</th><th scope=\"col\">Warnings</th><th scope=\"col\">Info</th>\
         <th scope=\"col\">Ignored</th></tr></thead><tbody>{rows}</tbody></table></div>"
    )
}

/// A deployment: its live report, filtered, then its history (US3, US8).
pub fn deployment_page(page: &DeploymentPage) -> String {
    let name = escape(page.name);
    let status = status_of(page.live)
        .map(status_badge)
        .unwrap_or("<span class=\"badge\">history only</span>");
    let live = match page.live {
        Some(Ok((report, fail_on))) => format!(
            "<h2>Live report</h2><p class=\"sub\">Audited now. {}</p>{}",
            threshold(fail_on),
            report_section(report, page.filter, Mode::Served)
        ),
        Some(Err(error)) => format!(
            "<h2>Live report</h2><p class=\"notice error\">This deployment cannot be read: {}</p>",
            escape(error)
        ),
        None => {
            "<p class=\"notice\">This deployment is not declared in <code>.eyeship.toml</code>: \
                 only its saved reports are shown.</p>"
                .to_owned()
        }
    };
    let body = format!(
        "<p class=\"sub\"><a href=\"/\">Deployments</a></p><h1>{name} {status}</h1>\
         <p class=\"sub\">{}</p>{live}{}",
        file_list(page.files),
        history_section(page.name, page.history, matches!(page.live, Some(Ok(_))))
    );
    layout(page.name, Nav::Deployments, Mode::Served, &body)
}

/// A saved report, with the same presentation as a live one (US8 §3).
pub fn saved_report_page(page: &SavedPage) -> String {
    let name = escape(page.name);
    let context = page.context;
    let label = context
        .label
        .as_deref()
        .map(|label| format!(", label <code>{}</code>", escape(label)))
        .unwrap_or_default();
    let body = format!(
        "<p class=\"sub\"><a href=\"/\">Deployments</a> / <a href=\"/deployments/{name}\">{name}</a></p>\
         <h1>{name} {}</h1>\
         <p class=\"notice\">Saved report from {}{label}. {}</p>\
         <p class=\"sub\">{}</p>{}",
        status_badge(page.report.status(context.fail_on)),
        escape(&short_date(&context.saved_at)),
        threshold(context.fail_on),
        file_list(&page.report.files),
        report_section(page.report, page.filter, Mode::Served)
    );
    layout(
        &format!("{} saved {}", page.name, short_date(&context.saved_at)),
        Nav::Deployments,
        Mode::Served,
        &body,
    )
}

/// What changed between two reports (US9).
pub fn compare_page(page: &ComparePage) -> String {
    let name = escape(page.name);
    let section = |title: &str, findings: &[&FindingView], empty: &str| {
        let items: String = findings
            .iter()
            .map(|finding| finding_item(finding, Mode::Served))
            .collect();
        let list = if findings.is_empty() {
            format!("<p class=\"empty\">{empty}</p>")
        } else {
            format!("<ul class=\"findings\">{items}</ul>")
        };
        format!("<h2>{title} ({})</h2>{list}", findings.len())
    };
    let body = format!(
        "<p class=\"sub\"><a href=\"/\">Deployments</a> / <a href=\"/deployments/{name}\">{name}</a></p>\
         <h1>Compare {name}</h1><p class=\"sub\">From {} to {}.</p>{}{}{}",
        escape(page.from),
        escape(page.to),
        section("New", &page.diff.new, "No new finding."),
        section("Resolved", &page.diff.resolved, "No resolved finding."),
        section("Unchanged", &page.diff.unchanged, "No unchanged finding."),
    );
    layout(
        &format!("Compare {}", page.name),
        Nav::Deployments,
        Mode::Served,
        &body,
    )
}

/// Every rule, as `eyeship rules` lists them (US4 §1).
pub fn rules_page(rules: &[Rule]) -> String {
    let rows: String = rules
        .iter()
        .map(|rule| {
            format!(
                "<tr><td><a href=\"/rules/{0}\"><code>{0}</code></a></td><td>{1}</td><td>{2}</td></tr>",
                escape(rule.id.as_str()),
                severity_badge(rule.severity),
                escape(rule.title)
            )
        })
        .collect();
    layout(
        "Rules",
        Nav::Rules,
        Mode::Served,
        &format!(
            "<h1>Rules</h1><p class=\"sub\">Each rule prevents a real production failure.</p>\
             <div class=\"table-wrap\"><table><thead><tr><th scope=\"col\">ID</th>\
             <th scope=\"col\">Default severity</th><th scope=\"col\">Title</th></tr></thead>\
             <tbody>{rows}</tbody></table></div>"
        ),
    )
}

/// One rule, as `eyeship explain` shows it (US4 §2).
pub fn rule_page(rule: &Rule) -> String {
    layout(
        &format!("{} {}", rule.id, rule.title),
        Nav::Rules,
        Mode::Served,
        &format!(
            "<p class=\"sub\"><a href=\"/rules\">Rules</a></p><h1><code>{}</code> {}</h1>{}",
            escape(rule.id.as_str()),
            escape(rule.title),
            rule_doc(rule)
        ),
    )
}

/// A page for a missing deployment, report or rule.
pub fn error_page(title: &str, message: &str) -> String {
    layout(
        title,
        Nav::None,
        Mode::Served,
        &format!(
            "<h1>{}</h1><p>{}</p><p><a href=\"/\">Back to the deployments</a></p>",
            escape(title),
            escape(message)
        ),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures::sample_report;
    use eyeship_domain::{RULES, diff};

    const EVIL: &str = "<script>alert(1)</script>";

    fn view() -> ReportView {
        ReportView::from(&sample_report())
    }

    fn context(saved_at: &str, label: Option<&str>) -> SavedContext {
        SavedContext {
            deployment: "prod".into(),
            id: saved_at.replace(':', "-"),
            saved_at: saved_at.into(),
            label: label.map(String::from),
            fail_on: FailOn::Error,
        }
    }

    fn files() -> Vec<String> {
        vec!["compose.yaml".into(), "compose.prod.yaml".into()]
    }

    #[test]
    fn dashboard() {
        let (report, clean, files) = (view(), ReportView::default(), files());
        let page = dashboard_page(&[
            Card {
                name: "broken",
                files: &files[..1],
                live: Some(Err("file not found: missing.yaml")),
                last_saved: None,
                trend: None,
            },
            Card {
                name: "prod",
                files: &files,
                live: Some(Ok((&report, FailOn::Error))),
                last_saved: Some("2026-11-25T09:12:03.481Z"),
                trend: Some(2),
            },
            Card {
                name: "staging",
                files: &files[..1],
                live: Some(Ok((&clean, FailOn::Warning))),
                last_saved: Some("2026-11-26T08:00:00.000Z"),
                trend: Some(-3),
            },
            Card {
                name: "old",
                files: &[],
                live: None,
                last_saved: Some("2026-10-01T00:00:00.000Z"),
                trend: None,
            },
        ]);
        insta::assert_snapshot!(page);
    }

    #[test]
    fn deployment_with_history() {
        let (report, files, filter) = (view(), files(), Filter::default());
        let (older, newer) = (
            context("2026-11-25T09:12:03.481Z", Some("a1b2c3d")),
            context("2026-11-26T11:47:17.123Z", None),
        );
        let clean = ReportView::default();
        let history = [
            HistoryRow {
                id: "2026-11-26T11-47-17.123Z",
                outcome: Ok((&newer, &clean)),
            },
            HistoryRow {
                id: "2026-11-25T09-12-03.481Z",
                outcome: Ok((&older, &report)),
            },
            HistoryRow {
                id: "2026-11-01T00-00-00.000Z",
                outcome: Err("invalid SARIF: EOF"),
            },
        ];
        insta::assert_snapshot!(deployment_page(&DeploymentPage {
            name: "prod",
            files: &files,
            live: Some(Ok((&report, FailOn::Error))),
            filter: &filter,
            history: &history,
        }));
    }

    #[test]
    fn deployment_without_history_or_unreadable() {
        let (files, filter) = (files(), Filter::default());
        let page = deployment_page(&DeploymentPage {
            name: "dev",
            files: &files,
            live: Some(Err("file not found: compose.dev.yaml")),
            filter: &filter,
            history: &[],
        });
        assert!(page.contains("cannot be read: file not found"));
        assert!(page.contains("eyeship compose --deployment dev --save"));
        assert!(page.contains("badge unreadable"));
    }

    #[test]
    fn saved_report() {
        let (report, filter) = (view(), Filter::default());
        let context = context("2026-11-25T09:12:03.481Z", Some("a1b2c3d"));
        let page = saved_report_page(&SavedPage {
            name: "prod",
            context: &context,
            report: &report,
            filter: &filter,
        });
        assert!(
            page.contains("Saved report from 2026-11-25 09:12 UTC, label <code>a1b2c3d</code>")
        );
        assert!(page.contains("badge fail"));
        assert!(page.contains("<form class=\"filters\""));
    }

    #[test]
    fn comparison() {
        let before = view();
        let mut after = view();
        after.findings.remove(0);
        after.findings[0].line = 99;
        let page = compare_page(&ComparePage {
            name: "prod",
            from: "2026-11-25 09:12 UTC",
            to: "live",
            diff: diff(&before.findings, &after.findings, FindingView::key),
        });
        assert!(page.contains("<h2>New (0)</h2>"));
        assert!(page.contains("<h2>Resolved (1)</h2>"));
        assert!(page.contains("<h2>Unchanged (3)</h2>"));
    }

    #[test]
    fn rules_and_rule() {
        let page = rules_page(RULES);
        assert_eq!(
            page.matches("<tr><td><a href=\"/rules/").count(),
            RULES.len()
        );
        let rule = rule_page(&RULES[0]);
        insta::assert_snapshot!(rule);
    }

    #[test]
    fn every_page_escapes_what_comes_from_the_audited_files() {
        let mut report = view();
        report.files = vec![EVIL.into()];
        report.findings.iter_mut().for_each(|finding| {
            finding.service = Some(EVIL.into());
            finding.message = EVIL.into();
            finding.fix = EVIL.into();
            finding.file = EVIL.into();
        });
        let files = vec![EVIL.to_owned()];
        let filter = Filter {
            service: Some(EVIL.into()),
            ..Filter::default()
        };
        let context = SavedContext {
            label: Some(EVIL.into()),
            ..context("2026-11-25T09:12:03.481Z", None)
        };
        let history = [HistoryRow {
            id: "x",
            outcome: Ok((&context, &report)),
        }];
        let pages = [
            dashboard_page(&[Card {
                name: EVIL,
                files: &files,
                live: Some(Err(EVIL)),
                last_saved: None,
                trend: None,
            }]),
            deployment_page(&DeploymentPage {
                name: EVIL,
                files: &files,
                live: Some(Ok((&report, FailOn::Error))),
                filter: &filter,
                history: &history,
            }),
            deployment_page(&DeploymentPage {
                name: "a",
                files: &files,
                live: Some(Ok((&report, FailOn::Error))),
                filter: &Filter::default(),
                history: &history,
            }),
            saved_report_page(&SavedPage {
                name: EVIL,
                context: &context,
                report: &report,
                filter: &filter,
            }),
            compare_page(&ComparePage {
                name: EVIL,
                from: EVIL,
                to: EVIL,
                diff: diff(&[], &report.findings, FindingView::key),
            }),
            error_page(EVIL, EVIL),
        ];
        for page in pages {
            assert!(!page.contains("<script"), "unescaped:\n{page}");
            assert!(page.contains("&lt;script&gt;"));
        }
    }

    #[test]
    fn no_page_loads_an_external_resource() {
        let page = dashboard_page(&[]);
        for forbidden in ["<script", "src=\"http", "href=\"http"] {
            assert!(!page.contains(forbidden), "{forbidden}");
        }
        assert!(rule_page(&RULES[0]).matches("http").all(|_| false));
    }

    #[test]
    fn trends_count_every_severity() {
        let now = view();
        assert_eq!(trend(&now, &ReportView::default()), 4);
        assert_eq!(trend(&ReportView::default(), &now), -4);
        assert_eq!(trend_text(0), "no change since the last saved report");
        assert_eq!(trend_text(-2), "▼ 2 fewer since the last saved report");
    }
}

//! From a request to a response. Pure, given a [`DeploymentSource`] (plan §5).

use eyeship_domain::{RULES, diff, rules};
use eyeship_history::Entry;
use eyeship_report::html::{
    Card, ComparePage, DeploymentPage, HistoryRow, STYLE, SavedPage, compare_page, dashboard_page,
    deployment_page, error_page, rule_page, rules_page, saved_report_page, style_path, trend,
};
use eyeship_report::{FindingView, ReportView, SarifRecord};

use crate::deployment::DeploymentResult;
use crate::route::{Route, route};

/// Port: what the interface needs from the application. Called on every
/// request, so that a reload shows the files as they are now (US2 §4).
pub trait DeploymentSource {
    /// Every declared deployment, audited now, failing first; or why the
    /// configuration cannot be read.
    fn deployments(&self) -> Result<Vec<DeploymentResult>, String>;
    /// Names of the deployments that have saved reports, sorted.
    fn history_names(&self) -> Vec<String>;
    /// Saved reports of a deployment, most recent first.
    fn history(&self, name: &str) -> Vec<Entry>;
}

/// An HTTP request, reduced to what the interface looks at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request<'a> {
    pub method: &'a str,
    pub url: &'a str,
    pub host: Option<&'a str>,
}

/// An HTTP response.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Response {
    pub status: u16,
    pub headers: Vec<(&'static str, String)>,
    pub body: String,
}

const CSP: &str = "default-src 'none'; style-src 'self'; img-src 'self' data:; \
                   form-action 'self'; base-uri 'none'; frame-ancestors 'none'";

fn response(status: u16, content_type: &str, cache: &str, body: String) -> Response {
    Response {
        status,
        headers: vec![
            ("Content-Type", content_type.to_owned()),
            ("Cache-Control", cache.to_owned()),
            ("Content-Security-Policy", CSP.to_owned()),
            ("X-Content-Type-Options", "nosniff".to_owned()),
            ("Referrer-Policy", "no-referrer".to_owned()),
            ("Cross-Origin-Resource-Policy", "same-origin".to_owned()),
        ],
        body,
    }
}

fn page(status: u16, body: String) -> Response {
    response(status, "text/html; charset=utf-8", "no-store", body)
}

fn not_found(message: &str) -> Response {
    page(404, error_page("Not found", message))
}

/// Whether `host` (the `Host` header) names this machine. Guards a server
/// that listens locally against DNS rebinding (plan §5).
pub fn is_local_host(host: &str) -> bool {
    let name = if host.starts_with('[') {
        host.split_once(']').map_or(host, |(name, _)| &name[1..])
    } else {
        host.rsplit_once(':').map_or(host, |(name, _)| name)
    };
    matches!(
        name.to_ascii_lowercase().as_str(),
        "localhost" | "127.0.0.1" | "::1"
    )
}

/// Answers a request. `local_only` is true when the server listens on a
/// loopback address: the `Host` header must then name this machine.
pub fn handle(request: &Request, source: &dyn DeploymentSource, local_only: bool) -> Response {
    if !matches!(request.method, "GET" | "HEAD") {
        let mut response = page(
            405,
            error_page("Method not allowed", "This interface is read-only."),
        );
        response.headers.push(("Allow", "GET, HEAD".to_owned()));
        return response;
    }
    if local_only && !request.host.is_some_and(is_local_host) {
        return page(
            421,
            error_page(
                "Misdirected request",
                "This interface only answers to localhost.",
            ),
        );
    }
    match route(request.url) {
        Route::Dashboard => dashboard(source),
        Route::Deployment { name, filter } => {
            let Some((live, history)) = deployment_data(source, &name) else {
                return not_found(&format!("No deployment named '{name}'."));
            };
            let views = HistoryViews::new(&history);
            let live_view = live
                .as_ref()
                .map(|d| (d.outcome.as_ref().map(ReportView::from), d));
            let files = live.as_ref().map_or_else(
                || views.latest_files(),
                |deployment| deployment.files.clone(),
            );
            page(
                200,
                deployment_page(&DeploymentPage {
                    name: &name,
                    files: &files,
                    live: live_view.as_ref().map(|(view, deployment)| match view {
                        Ok(view) => Ok((view, deployment.fail_on)),
                        Err(error) => Err(error.as_str()),
                    }),
                    filter: &filter,
                    history: &views.rows(),
                }),
            )
        }
        Route::SavedReport { name, id, filter } => {
            let history = known(source, &name).then(|| source.history(&name));
            match history
                .iter()
                .flatten()
                .find(|entry| entry.id == id)
                .map(|entry| &entry.outcome)
            {
                None => not_found(&format!("No saved report '{id}' for '{name}'.")),
                Some(Err(error)) => page(
                    200,
                    error_page(
                        "Unreadable report",
                        &format!("'{id}' cannot be read: {error}"),
                    ),
                ),
                Some(Ok(record)) => match &record.context {
                    Some(context) => page(
                        200,
                        saved_report_page(&SavedPage {
                            name: &name,
                            context,
                            report: &record.report,
                            filter: &filter,
                        }),
                    ),
                    None => not_found(&format!("'{id}' has no save information.")),
                },
            }
        }
        Route::Compare { name, from, to } => compare(source, &name, &from, &to),
        Route::Rules => page(200, rules_page(RULES)),
        Route::Rule { id } => rules::find(&id).map_or_else(
            || not_found(&format!("No rule '{id}'.")),
            |rule| page(200, rule_page(rule)),
        ),
        Route::Style { path } if path == style_path() => response(
            200,
            "text/css; charset=utf-8",
            "public, max-age=31536000, immutable",
            STYLE.to_owned(),
        ),
        Route::Style { .. } => not_found("No such file."),
        Route::Health => response(200, "text/plain; charset=utf-8", "no-store", "ok\n".into()),
        Route::NotFound => not_found("This page does not exist."),
    }
}

/// A deployment is known when it is declared or has saved reports.
fn known(source: &dyn DeploymentSource, name: &str) -> bool {
    source
        .deployments()
        .is_ok_and(|all| all.iter().any(|deployment| deployment.name == name))
        || source.history_names().iter().any(|known| known == name)
}

/// The live audit (if declared) and the history of a known deployment.
fn deployment_data(
    source: &dyn DeploymentSource,
    name: &str,
) -> Option<(Option<DeploymentResult>, Vec<Entry>)> {
    let live = source
        .deployments()
        .ok()
        .and_then(|all| all.into_iter().find(|deployment| deployment.name == name));
    let has_history = source.history_names().iter().any(|known| known == name);
    (live.is_some() || has_history).then(|| (live, source.history(name)))
}

/// Saved reports with their errors as text, for the pages.
struct HistoryViews<'a> {
    entries: &'a [Entry],
}

impl<'a> HistoryViews<'a> {
    fn new(entries: &'a [Entry]) -> Self {
        Self { entries }
    }

    fn rows(&self) -> Vec<HistoryRow<'a>> {
        self.entries
            .iter()
            .map(|entry| match &entry.outcome {
                Ok(SarifRecord {
                    context: Some(context),
                    report,
                    ..
                }) => HistoryRow {
                    id: &entry.id,
                    outcome: Ok((context, report)),
                },
                Ok(_) => HistoryRow {
                    id: &entry.id,
                    outcome: Err("no save information"),
                },
                Err(error) => HistoryRow {
                    id: &entry.id,
                    outcome: Err(error.as_str()),
                },
            })
            .collect()
    }

    fn latest(&self) -> Option<&'a SarifRecord> {
        self.entries
            .iter()
            .find_map(|entry| entry.outcome.as_ref().ok())
    }

    fn latest_files(&self) -> Vec<String> {
        self.latest()
            .map(|record| record.report.files.clone())
            .unwrap_or_default()
    }
}

fn dashboard(source: &dyn DeploymentSource) -> Response {
    let deployments = match source.deployments() {
        Ok(deployments) => deployments,
        Err(error) => return page(500, error_page("Invalid configuration", &error)),
    };
    let history_only: Vec<String> = source
        .history_names()
        .into_iter()
        .filter(|name| {
            deployments
                .iter()
                .all(|deployment| &deployment.name != name)
        })
        .collect();
    let histories: Vec<Vec<Entry>> = deployments
        .iter()
        .map(|deployment| deployment.name.as_str())
        .chain(history_only.iter().map(String::as_str))
        .map(|name| source.history(name))
        .collect();
    let live_views: Vec<Result<ReportView, &str>> = deployments
        .iter()
        .map(|deployment| match &deployment.outcome {
            Ok(report) => Ok(ReportView::from(report)),
            Err(error) => Err(error.as_str()),
        })
        .collect();
    let latest_files: Vec<Vec<String>> = histories
        .iter()
        .map(|history| HistoryViews::new(history).latest_files())
        .collect();
    let latest = |index: usize| HistoryViews::new(&histories[index]).latest();
    let saved_at = |index: usize| {
        latest(index)
            .and_then(|record| record.context.as_ref())
            .map(|context| context.saved_at.as_str())
    };
    let cards: Vec<Card> = deployments
        .iter()
        .zip(&live_views)
        .enumerate()
        .map(|(index, (deployment, view))| Card {
            name: &deployment.name,
            files: &deployment.files,
            live: Some(
                view.as_ref()
                    .map(|view| (view, deployment.fail_on))
                    .map_err(|e| *e),
            ),
            last_saved: saved_at(index),
            trend: view
                .as_ref()
                .ok()
                .zip(latest(index))
                .map(|(now, before)| trend(now, &before.report)),
        })
        .chain(history_only.iter().enumerate().map(|(offset, name)| {
            let index = deployments.len() + offset;
            Card {
                name,
                files: &latest_files[index],
                live: None,
                last_saved: saved_at(index),
                trend: None,
            }
        }))
        .collect();
    page(200, dashboard_page(&cards))
}

fn compare(source: &dyn DeploymentSource, name: &str, from: &str, to: &str) -> Response {
    let Some((live, history)) = deployment_data(source, name) else {
        return not_found(&format!("No deployment named '{name}'."));
    };
    let live_view = live
        .as_ref()
        .and_then(|deployment| deployment.outcome.as_ref().ok())
        .map(ReportView::from);
    let side = |id: &str| -> Option<(String, &ReportView)> {
        if id == "live" {
            return live_view.as_ref().map(|view| ("live".to_owned(), view));
        }
        history
            .iter()
            .find(|entry| entry.id == id)
            .and_then(|entry| {
                let record = entry.outcome.as_ref().ok()?;
                let saved = record
                    .context
                    .as_ref()
                    .map_or_else(|| id.to_owned(), |context| context.saved_at.clone());
                Some((format!("saved {}", short(&saved)), &record.report))
            })
    };
    match (side(from), side(to)) {
        (Some((from_label, before)), Some((to_label, after))) => page(
            200,
            compare_page(&ComparePage {
                name,
                from: &from_label,
                to: &to_label,
                diff: diff(&before.findings, &after.findings, FindingView::key),
            }),
        ),
        _ => not_found("One of the compared reports does not exist or cannot be read."),
    }
}

fn short(rfc3339: &str) -> String {
    match (rfc3339.get(..10), rfc3339.get(11..16)) {
        (Some(day), Some(time)) => format!("{day} {time} UTC"),
        _ => rfc3339.to_owned(),
    }
}

#[cfg(test)]
mod tests;

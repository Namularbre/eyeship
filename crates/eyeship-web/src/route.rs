//! From a request path to what to show. Pure (plan §5).

use eyeship_domain::Severity;
use eyeship_report::html::Filter;

/// A page or file of the interface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Dashboard,
    Deployment {
        name: String,
        filter: Filter,
    },
    SavedReport {
        name: String,
        id: String,
        filter: Filter,
    },
    Compare {
        name: String,
        from: String,
        to: String,
    },
    Rules,
    Rule {
        id: String,
    },
    Style {
        path: String,
    },
    Health,
    NotFound,
}

/// Parses `/path?query`. Unknown query parameters and invalid filter values
/// are ignored rather than rejected.
pub fn route(url: &str) -> Route {
    let (path, query) = url.split_once('?').unwrap_or((url, ""));
    let params = parse_query(query);
    let param = |key: &str| {
        params
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, value)| value.clone())
            .filter(|value| !value.is_empty())
    };
    let filter = || Filter {
        severity: param("severity").and_then(|value| match value.as_str() {
            "error" => Some(Severity::Error),
            "warning" => Some(Severity::Warning),
            "info" => Some(Severity::Info),
            _ => None,
        }),
        service: param("service"),
    };
    let segments: Vec<String> = path
        .trim_end_matches('/')
        .split('/')
        .skip(1)
        .map(percent_decode)
        .collect();
    let segments: Vec<&str> = segments.iter().map(String::as_str).collect();
    match segments.as_slice() {
        [] | [""] => Route::Dashboard,
        ["deployments", name] => Route::Deployment {
            name: (*name).to_owned(),
            filter: filter(),
        },
        ["deployments", name, "reports", id] => Route::SavedReport {
            name: (*name).to_owned(),
            id: (*id).to_owned(),
            filter: filter(),
        },
        ["deployments", name, "compare"] => match (param("from"), param("to")) {
            (Some(from), Some(to)) => Route::Compare {
                name: (*name).to_owned(),
                from,
                to,
            },
            _ => Route::Deployment {
                name: (*name).to_owned(),
                filter: Filter::default(),
            },
        },
        ["rules"] => Route::Rules,
        ["rules", id] => Route::Rule {
            id: (*id).to_owned(),
        },
        ["assets", file] if file.starts_with("style.") && file.ends_with(".css") => Route::Style {
            path: path.to_owned(),
        },
        ["health"] => Route::Health,
        _ => Route::NotFound,
    }
}

fn parse_query(query: &str) -> Vec<(String, String)> {
    query
        .split('&')
        .filter(|pair| !pair.is_empty())
        .map(|pair| {
            let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
            (
                percent_decode(&key.replace('+', " ")),
                percent_decode(&value.replace('+', " ")),
            )
        })
        .collect()
}

/// Decodes `%XX` sequences; an invalid sequence is kept as written.
fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        let decoded = (bytes[index] == b'%')
            .then(|| text.get(index + 1..index + 3))
            .flatten()
            .and_then(|hex| u8::from_str_radix(hex, 16).ok());
        match decoded {
            Some(byte) => {
                out.push(byte);
                index += 3;
            }
            None => {
                out.push(bytes[index]);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deployment(name: &str, severity: Option<Severity>, service: Option<&str>) -> Route {
        Route::Deployment {
            name: name.into(),
            filter: Filter {
                severity,
                service: service.map(String::from),
            },
        }
    }

    #[test]
    fn routes_every_page() {
        let cases = [
            ("/", Route::Dashboard),
            ("", Route::Dashboard),
            ("/?x=1", Route::Dashboard),
            ("/deployments/prod", deployment("prod", None, None)),
            ("/deployments/prod/", deployment("prod", None, None)),
            (
                "/deployments/prod/reports/2026-11-26T11-47-17.123Z",
                Route::SavedReport {
                    name: "prod".into(),
                    id: "2026-11-26T11-47-17.123Z".into(),
                    filter: Filter::default(),
                },
            ),
            (
                "/deployments/prod/compare?from=a&to=live",
                Route::Compare {
                    name: "prod".into(),
                    from: "a".into(),
                    to: "live".into(),
                },
            ),
            (
                "/deployments/prod/compare?from=a",
                deployment("prod", None, None),
            ),
            ("/rules", Route::Rules),
            ("/rules/R001", Route::Rule { id: "R001".into() }),
            (
                "/assets/style.0123abcd.css",
                Route::Style {
                    path: "/assets/style.0123abcd.css".into(),
                },
            ),
            ("/health", Route::Health),
            ("/assets/other.js", Route::NotFound),
            ("/deployments", Route::NotFound),
            ("/nope", Route::NotFound),
            ("/deployments/a/b/c/d/e", Route::NotFound),
        ];
        for (url, expected) in cases {
            assert_eq!(route(url), expected, "{url}");
        }
    }

    #[test]
    fn parses_filters_and_ignores_invalid_values() {
        assert_eq!(
            route("/deployments/prod?severity=warning&service=my+api"),
            deployment("prod", Some(Severity::Warning), Some("my api"))
        );
        assert_eq!(
            route("/deployments/prod?severity=loud&service=&other=1"),
            deployment("prod", None, None)
        );
        assert_eq!(
            route("/deployments/prod?service=caf%C3%A9%26co"),
            deployment("prod", None, Some("café&co"))
        );
    }

    #[test]
    fn decodes_percent_sequences_leniently() {
        assert_eq!(percent_decode("a%20b"), "a b");
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("%zz"), "%zz");
        assert_eq!(percent_decode("%2e%2e"), "..");
    }
}

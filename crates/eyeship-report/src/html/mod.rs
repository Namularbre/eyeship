//! HTML pages of the web interface and of the standalone export (spec 004).
//!
//! Every page is a pure function from data to a `String`. Every value that
//! comes from an audited file goes through [`escape`].

mod chart;
mod pages;
mod report;

pub use pages::{
    Card, ComparePage, DeploymentPage, HistoryRow, SavedPage, compare_page, dashboard_page,
    deployment_page, error_page, rule_page, rules_page, saved_report_page, trend,
};
pub use report::{Filter, standalone_report};

use crate::sarif::fingerprint;

/// The stylesheet, embedded in the binary.
pub const STYLE: &str = include_str!("../../assets/style.css");

/// URL of the stylesheet, versioned by its content so that it can be cached.
pub fn style_path() -> String {
    format!("/assets/style.{}.css", &fingerprint(STYLE, None, "")[..8])
}

/// Escapes text for HTML content and quoted attribute values.
pub fn escape(text: &str) -> String {
    text.chars()
        .map(|c| match c {
            '&' => "&amp;".to_owned(),
            '<' => "&lt;".to_owned(),
            '>' => "&gt;".to_owned(),
            '"' => "&quot;".to_owned(),
            '\'' => "&#39;".to_owned(),
            c => c.to_string(),
        })
        .collect()
}

/// Escapes a value placed in a URL query, then for HTML.
pub fn query_value(text: &str) -> String {
    text.bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                char::from(byte).to_string()
            }
            byte => format!("%{byte:02X}"),
        })
        .collect()
}

/// Which navigation entry is current.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Nav {
    Deployments,
    Rules,
    None,
}

/// Served pages link the stylesheet; a standalone export inlines it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    Served,
    Standalone,
}

pub(crate) fn layout(title: &str, nav: Nav, mode: Mode, body: &str) -> String {
    let style = match mode {
        Mode::Served => format!("<link rel=\"stylesheet\" href=\"{}\">", style_path()),
        Mode::Standalone => format!("<style>\n{STYLE}</style>"),
    };
    let link = |href: &str, label: &str, entry: Nav| {
        let current = if nav == entry {
            " aria-current=\"page\""
        } else {
            ""
        };
        format!("<a href=\"{href}\"{current}>{label}</a>")
    };
    let header = match mode {
        Mode::Served => format!(
            "<header class=\"top\"><a class=\"brand\" href=\"/\">eyeship</a>\
             <nav aria-label=\"Main\">{}{}</nav></header>",
            link("/", "Deployments", Nav::Deployments),
            link("/rules", "Rules", Nav::Rules)
        ),
        Mode::Standalone => {
            "<header class=\"top\"><span class=\"brand\">eyeship</span></header>".to_owned()
        }
    };
    format!(
        "<!doctype html>\n<html lang=\"en\">\n<head>\n<meta charset=\"utf-8\">\n\
         <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n\
         <title>{} · eyeship</title>\n{style}\n</head>\n<body>\n\
         <a class=\"skip\" href=\"#main\">Skip to content</a>\n{header}\n\
         <main id=\"main\">\n{body}</main>\n\
         <footer>eyeship {} · static audit, nothing was run</footer>\n</body>\n</html>\n",
        escape(title),
        env!("CARGO_PKG_VERSION")
    )
}

/// `1 error`, `2 errors`.
pub(crate) fn plural(count: usize, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// `2026-11-26T11:47:17.123Z` as `2026-11-26 11:47 UTC`.
pub(crate) fn short_date(rfc3339: &str) -> String {
    match (rfc3339.get(..10), rfc3339.get(11..16)) {
        (Some(day), Some(time)) => format!("{day} {time} UTC"),
        _ => rfc3339.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn escapes_html_special_characters() {
        assert_eq!(
            escape("<script>alert('x & \"y\"')</script>"),
            "&lt;script&gt;alert(&#39;x &amp; &quot;y&quot;&#39;)&lt;/script&gt;"
        );
        assert_eq!(escape("plain-text_1"), "plain-text_1");
    }

    #[test]
    fn encodes_query_values() {
        assert_eq!(query_value("my api&x=1"), "my%20api%26x%3D1");
        assert_eq!(query_value("été"), "%C3%A9t%C3%A9");
    }

    #[test]
    fn the_style_path_depends_on_the_content() {
        let path = style_path();
        assert!(path.starts_with("/assets/style.") && path.ends_with(".css"));
        assert_eq!(path.len(), "/assets/style.12345678.css".len());
    }

    #[test]
    fn the_stylesheet_loads_nothing_external() {
        assert!(!STYLE.contains("http"), "no external URL");
        assert!(!STYLE.contains("@import"));
        assert!(!STYLE.contains("url("));
        assert!(STYLE.len() < 10 * 1024);
    }

    #[test]
    fn the_layout_escapes_the_title() {
        let page = layout("<b>", Nav::None, Mode::Served, "");
        assert!(page.contains("<title>&lt;b&gt; · eyeship</title>"));
        assert!(page.starts_with("<!doctype html>"));
        assert!(page.contains("lang=\"en\""));
    }

    #[test]
    fn dates_are_shortened() {
        assert_eq!(
            short_date("2026-11-26T11:47:17.123Z"),
            "2026-11-26 11:47 UTC"
        );
        assert_eq!(short_date("bad"), "bad");
    }
}

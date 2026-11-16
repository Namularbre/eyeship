//! Rule catalogue output, for `eyeship rules` and `eyeship explain`.

use std::io::{self, Write};

use eyeship_domain::Rule;

const ID_HEADER: &str = "ID";
const SEVERITY_HEADER: &str = "SEVERITY";

/// Lists rules as aligned columns: ID, default severity, title.
pub fn render_rules(rules: &[Rule], out: &mut dyn Write) -> io::Result<()> {
    let id_width = rules
        .iter()
        .map(|rule| rule.id.as_str().len())
        .fold(ID_HEADER.len(), usize::max);
    let row = |id: &str, severity: &str, title: &str| {
        format!(
            "{id:<id_width$}  {severity:<sev_width$}  {title}\n",
            sev_width = SEVERITY_HEADER.len()
        )
    };
    let text = rules
        .iter()
        .map(|rule| row(rule.id.as_str(), rule.severity.as_str(), rule.title))
        .fold(row(ID_HEADER, SEVERITY_HEADER, "TITLE"), |text, line| {
            text + &line
        });
    out.write_all(text.as_bytes())
}

/// Shows the full documentation of one rule.
pub fn render_explain(rule: &Rule, out: &mut dyn Write) -> io::Result<()> {
    let doc = rule.doc;
    let sections = [
        ("Scenario", doc.scenario),
        ("Fix", doc.fix),
        ("Bad example", doc.bad_example),
        ("Good example", doc.good_example),
    ]
    .iter()
    .map(|(heading, body)| format!("{heading}:\n{}", indented(body)))
    .collect::<Vec<_>>()
    .join("\n");
    let text = format!(
        "{} {}\nSeverity: {}\n\n{sections}",
        rule.id, rule.title, rule.severity
    );
    out.write_all(text.as_bytes())
}

/// Indents every non-empty line by four spaces.
fn indented(text: &str) -> String {
    text.lines()
        .map(|line| match line {
            "" => "\n".to_owned(),
            line => format!("    {line}\n"),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use eyeship_domain::{Finding, Project, RuleDoc, RuleId, Severity};

    fn no_findings(_: &Project) -> Vec<Finding> {
        Vec::new()
    }

    const HEALTHY: Rule = Rule {
        id: RuleId("R001"),
        title: "depends_on does not wait for a healthy service",
        severity: Severity::Warning,
        doc: RuleDoc {
            scenario: "The API starts before the database accepts connections and crashes.",
            fix: "Add a healthcheck to the dependency and use `condition: service_healthy`.",
            bad_example: "services:\n  api:\n    depends_on: [db]\n",
            good_example: "services:\n  api:\n    depends_on:\n      db:\n        condition: service_healthy\n",
        },
        check: no_findings,
    };

    const VERSION: Rule = Rule {
        id: RuleId("R017"),
        title: "obsolete `version` key",
        severity: Severity::Info,
        doc: HEALTHY.doc,
        check: no_findings,
    };

    const LONG_ID: Rule = Rule {
        id: RuleId("R1000"),
        title: "a rule with a longer ID",
        severity: Severity::Error,
        doc: HEALTHY.doc,
        check: no_findings,
    };

    fn rendered(write: impl FnOnce(&mut dyn Write) -> io::Result<()>) -> String {
        let mut out = Vec::new();
        write(&mut out).expect("writing to a Vec never fails");
        String::from_utf8(out).expect("output is UTF-8")
    }

    #[test]
    fn lists_rules_in_aligned_columns() {
        insta::assert_snapshot!(rendered(|out| render_rules(
            &[HEALTHY, VERSION, LONG_ID],
            out
        )));
    }

    #[test]
    fn titles_start_in_the_same_column() {
        let output = rendered(|out| render_rules(&[HEALTHY, LONG_ID], out));
        let column = |title: &str| output.lines().find_map(|line| line.find(title));
        assert!(column(HEALTHY.title).is_some());
        assert_eq!(column(HEALTHY.title), column(LONG_ID.title));
    }

    #[test]
    fn explains_a_rule() {
        insta::assert_snapshot!(rendered(|out| render_explain(&HEALTHY, out)));
    }
}

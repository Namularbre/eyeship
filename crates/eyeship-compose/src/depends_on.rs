//! Normalization of `depends_on`, list and map forms.

use eyeship_domain::{Condition, Dependency};

use crate::context::Context;
use crate::raw::{ListOrMap, RawDependsOn};

/// Maps `depends_on`; each dependency is located at its name.
pub(crate) fn map_depends_on(
    ctx: &Context<'_>,
    depends_on: &ListOrMap<Option<RawDependsOn>>,
) -> Vec<Dependency> {
    match depends_on {
        ListOrMap::List(names) => names
            .iter()
            .filter_map(|name| ctx.resolved_text(name))
            .map(|name| Dependency {
                service: name.value,
                condition: Condition::Started,
                location: name.location,
            })
            .collect(),
        ListOrMap::Map(entries) => entries
            .iter()
            .map(|(name, long)| Dependency {
                service: name.value.as_str().to_owned(),
                condition: long
                    .as_ref()
                    .and_then(|long| long.condition.as_ref())
                    .map_or(Condition::Started, |condition| {
                        parse_condition(&ctx.interpolate(condition.value.as_str()))
                    }),
                location: ctx.location(&name.referenced),
            })
            .collect(),
    }
}

/// Unknown conditions count as `Started`, the weakest guarantee.
fn parse_condition(text: &str) -> Condition {
    match text {
        "service_healthy" => Condition::Healthy,
        "service_completed_successfully" => Condition::CompletedSuccessfully,
        _ => Condition::Started,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::test_support::*;

    fn deps(yaml: &str) -> Vec<Dependency> {
        with_context(&[], &[], |ctx| {
            service(yaml)
                .depends_on
                .map(|d| map_depends_on(ctx, &d))
                .unwrap_or_default()
        })
    }

    fn dep(service: &str, condition: Condition, line: u32, column: u32) -> Dependency {
        Dependency {
            service: service.into(),
            condition,
            location: at(line, column),
        }
    }

    #[test]
    fn list_form_waits_for_start() {
        assert_eq!(
            deps("depends_on:\n  - db\n  - cache\n"),
            [
                dep("db", Condition::Started, 2, 5),
                dep("cache", Condition::Started, 3, 5)
            ]
        );
    }

    #[test]
    fn map_form_reads_conditions() {
        let yaml = "depends_on:
  a:
    condition: service_started
  b:
    condition: service_healthy
  c:
    condition: service_completed_successfully
  d:
    restart: true
  e:
";
        assert_eq!(
            deps(yaml),
            [
                dep("a", Condition::Started, 2, 3),
                dep("b", Condition::Healthy, 4, 3),
                dep("c", Condition::CompletedSuccessfully, 6, 3),
                dep("d", Condition::Started, 8, 3),
                dep("e", Condition::Started, 10, 3),
            ]
        );
    }

    #[test]
    fn names_with_an_unresolved_variable_are_left_to_r013() {
        assert_eq!(
            deps("depends_on: [\"${UNSET}\", db]"),
            [dep("db", Condition::Started, 1, 26)]
        );
    }
}

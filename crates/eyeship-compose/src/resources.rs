//! Memory limit, logging and healthcheck of a service.

use eyeship_domain::{Healthcheck, Located, Logging};
use serde_saphyr::Spanned;

use crate::context::Context;
use crate::duration::parse_duration;
use crate::raw::{OneOrMany, RawHealthcheck, RawLogging, RawService, Text};
use crate::size::parse_size;

/// `mem_limit`, else `deploy.resources.limits.memory`, in bytes.
pub(crate) fn map_memory_limit(ctx: &Context<'_>, service: &RawService) -> Option<Located<u64>> {
    let deploy_memory = || {
        service
            .deploy
            .as_ref()?
            .resources
            .as_ref()?
            .limits
            .as_ref()?
            .memory
            .as_ref()
    };
    let text = service.mem_limit.as_ref().or_else(deploy_memory)?;
    let located = ctx.text(text);
    parse_size(&located.value).map(|bytes| Located::new(bytes, located.location))
}

pub(crate) fn map_logging(ctx: &Context<'_>, logging: &Spanned<RawLogging>) -> Logging {
    let text = |t: &Text| ctx.text(t).value;
    Logging {
        driver: logging.value.driver.as_ref().map(text),
        max_size: logging
            .value
            .options
            .as_ref()
            .and_then(|options| options.max_size.as_ref())
            .map(text),
        location: ctx.location(&logging.referenced),
    }
}

/// Whether the service's healthcheck writes `test` or `disable`, the keys
/// that decide whether it is disabled.
pub(crate) fn healthcheck_sets_status(service: &RawService) -> bool {
    service.healthcheck.as_ref().is_some_and(|healthcheck| {
        healthcheck.value.test.is_some() || healthcheck.value.disable.is_some()
    })
}

pub(crate) fn map_healthcheck(
    ctx: &Context<'_>,
    healthcheck: &Spanned<RawHealthcheck>,
) -> Healthcheck {
    let raw = &healthcheck.value;
    let duration = |text: &Option<Text>| {
        let located = ctx.text(text.as_ref()?);
        parse_duration(&located.value).map(|d| Located::new(d, located.location))
    };
    let disabled_flag = raw
        .disable
        .as_ref()
        .is_some_and(|flag| ctx.interpolate(flag.as_str()) == "true");
    let test_none = matches!(
        &raw.test,
        Some(OneOrMany::Many(items)) if items.first().is_some_and(|first| first.value.as_str() == "NONE")
    );
    Healthcheck {
        disabled: disabled_flag || test_none,
        interval: duration(&raw.interval),
        timeout: duration(&raw.timeout),
        retries: raw.retries.as_ref().and_then(|text| {
            let located = ctx.text(text);
            located
                .value
                .trim()
                .parse()
                .ok()
                .map(|n| Located::new(n, located.location))
        }),
        location: ctx.location(&healthcheck.referenced),
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::context::test_support::*;

    fn memory(yaml: &str) -> Option<Located<u64>> {
        with_context(&[], &[("MEM", "1g")], |ctx| {
            map_memory_limit(ctx, &service(yaml))
        })
    }

    fn healthcheck(yaml: &str) -> Healthcheck {
        with_context(&[], &[], |ctx| {
            map_healthcheck(ctx, service(yaml).healthcheck.as_ref().unwrap())
        })
    }

    #[test]
    fn mem_limit_with_units() {
        assert_eq!(
            memory("mem_limit: 512m"),
            Some(Located::new(512 << 20, at(1, 12)))
        );
        assert_eq!(memory("mem_limit: 1024").map(|m| m.value), Some(1024));
        assert_eq!(memory("mem_limit: ${MEM}").map(|m| m.value), Some(1 << 30));
    }

    #[test]
    fn deploy_memory_limit() {
        let yaml = "deploy:\n  resources:\n    limits:\n      memory: 2G\n";
        assert_eq!(memory(yaml), Some(Located::new(2 << 30, at(4, 15))));
    }

    #[test]
    fn no_or_invalid_memory_limit() {
        assert_eq!(memory("image: x"), None);
        assert_eq!(memory("deploy:\n  replicas: 2\n"), None);
        assert_eq!(memory("mem_limit: lots"), None);
    }

    #[test]
    fn logging_driver_and_max_size() {
        let yaml =
            "logging:\n  driver: json-file\n  options:\n    max-size: 10m\n    max-file: \"3\"\n";
        let logging = with_context(&[], &[], |ctx| {
            map_logging(ctx, service(yaml).logging.as_ref().unwrap())
        });
        assert_eq!(logging.driver.as_deref(), Some("json-file"));
        assert_eq!(logging.max_size.as_deref(), Some("10m"));
        assert_eq!(logging.location.line, 2);
    }

    #[test]
    fn logging_without_options() {
        let logging = with_context(&[], &[], |ctx| {
            map_logging(
                ctx,
                service("logging:\n  driver: syslog\n")
                    .logging
                    .as_ref()
                    .unwrap(),
            )
        });
        assert_eq!(
            (logging.driver.as_deref(), logging.max_size),
            (Some("syslog"), None)
        );
    }

    #[test]
    fn healthcheck_timings() {
        let yaml = "healthcheck:
  test: [CMD, curl, -f, http://localhost]
  interval: 1m30s
  timeout: 10s
  retries: 3
";
        let check = healthcheck(yaml);
        assert!(!check.disabled);
        assert_eq!(
            check.interval,
            Some(Located::new(Duration::from_secs(90), at(3, 13)))
        );
        assert_eq!(
            check.timeout.map(|t| t.value),
            Some(Duration::from_secs(10))
        );
        assert_eq!(check.retries, Some(Located::new(3, at(5, 12))));
        assert_eq!(check.location.line, 2);
    }

    #[test]
    fn healthcheck_disabled() {
        assert!(healthcheck("healthcheck:\n  disable: true\n").disabled);
        assert!(healthcheck("healthcheck:\n  test: [\"NONE\"]\n").disabled);
        assert!(!healthcheck("healthcheck:\n  test: curl -f http://x\n").disabled);
    }

    #[test]
    fn healthcheck_status_is_set_by_test_or_disable() {
        assert!(healthcheck_sets_status(&service(
            "healthcheck:\n  disable: false\n"
        )));
        assert!(healthcheck_sets_status(&service(
            "healthcheck:\n  test: curl x\n"
        )));
        assert!(!healthcheck_sets_status(&service(
            "healthcheck:\n  interval: 5s\n"
        )));
        assert!(!healthcheck_sets_status(&service("image: x\n")));
    }

    #[test]
    fn invalid_healthcheck_values_are_none() {
        let check = healthcheck("healthcheck:\n  interval: soon\n  retries: many\n");
        assert_eq!((check.interval, check.retries), (None, None));
    }
}

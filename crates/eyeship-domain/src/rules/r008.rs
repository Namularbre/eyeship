//! R008: an unpinned image changes without any change to the file.

use crate::finding::{Finding, RuleId, Severity};
use crate::model::{ImageRef, Project};
use crate::rule::{Rule, RuleDoc};

pub const RULE: Rule = Rule {
    id: RuleId("R008"),
    title: "Image not pinned",
    severity: Severity::Warning,
    doc: RuleDoc {
        scenario: "An image without a tag, or tagged `latest`, resolves to whatever was pushed \
                   last. A redeploy or a new host silently pulls a different version, possibly \
                   with breaking changes, without any change to the Compose file.",
        fix: "Pin the image to an explicit version tag (e.g. `postgres:16.4`), or to a digest \
              (`@sha256:...`) for full reproducibility.",
        bad_example: "services:\n  db:\n    image: postgres:latest\n",
        good_example: "services:\n  db:\n    image: postgres:16.4\n",
    },
    check,
};

fn check(project: &Project) -> Vec<Finding> {
    project
        .services
        .iter()
        .filter_map(|service| {
            service
                .image
                .as_ref()
                .filter(|image| is_unpinned(image))
                .map(|image| {
                    RULE.finding(
                        Some(&service.name),
                        image.location,
                        format!(
                            "image '{}' is not pinned to a version and may change on the next pull",
                            image.raw
                        ),
                        format!(
                            "Pin '{}' to an explicit version tag or a digest (@sha256:...).",
                            image.name
                        ),
                    )
                })
        })
        .collect()
}

/// No tag or `latest`, and no digest (a digest pins even `latest`).
fn is_unpinned(image: &ImageRef) -> bool {
    image.digest.is_none() && image.tag.as_deref().is_none_or(|tag| tag == "latest")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Service;
    use crate::test_support::{at, image, project, service};

    fn with_image(raw: &str) -> Service {
        Service {
            image: image(raw, 3),
            ..service("db", 2)
        }
    }

    #[test]
    fn flags_an_image_without_tag_on_the_image() {
        let findings = check(&project(vec![with_image("postgres")]));
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].rule, RuleId("R008"));
        assert_eq!(findings[0].service.as_deref(), Some("db"));
        assert_eq!(findings[0].location, at(3));
        assert!(findings[0].message.contains("'postgres'"));
    }

    #[test]
    fn flags_the_latest_tag() {
        let findings = check(&project(vec![with_image("postgres:latest")]));
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("'postgres:latest'"));
    }

    #[test]
    fn flags_a_registry_with_port_without_tag() {
        assert_eq!(
            check(&project(vec![with_image("localhost:5000/team/api")])).len(),
            1
        );
    }

    #[test]
    fn accepts_a_version_tag() {
        let services = [
            "postgres:16.4",
            "ghcr.io/acme/api:1.4.2",
            "localhost:5000/api:2",
        ]
        .into_iter()
        .map(with_image)
        .collect();
        assert!(check(&project(services)).is_empty());
    }

    #[test]
    fn a_digest_pins_the_image_even_with_latest() {
        let services = ["postgres@sha256:abc", "postgres:latest@sha256:abc"]
            .into_iter()
            .map(with_image)
            .collect();
        assert!(check(&project(services)).is_empty());
    }

    #[test]
    fn ignores_a_service_without_image() {
        let api = Service {
            has_build: true,
            ..service("api", 2)
        };
        assert!(check(&project(vec![api, service("other", 5)])).is_empty());
    }

    #[test]
    fn flags_an_unpinned_image_even_with_a_build_as_the_spec_says() {
        let api = Service {
            has_build: true,
            ..with_image("acme/api")
        };
        assert_eq!(check(&project(vec![api])).len(), 1);
    }

    #[test]
    fn an_empty_project_has_no_findings() {
        assert!(check(&project(vec![])).is_empty());
    }
}

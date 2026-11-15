//! The rule registry. One module per rule: `rNNN.rs`.

mod r005;
mod r006;
mod r007;
mod r008;
mod r013;
mod r014;
mod r015;
mod r016;
mod r017;

use crate::rule::Rule;

/// Every rule, in ID order.
pub static RULES: &[Rule] = &[
    r005::RULE,
    r006::RULE,
    r007::RULE,
    r008::RULE,
    r013::RULE,
    r014::RULE,
    r015::RULE,
    r016::RULE,
    r017::RULE,
];

/// Finds a rule by its ID, case-insensitively.
pub fn find(id: &str) -> Option<&'static Rule> {
    RULES
        .iter()
        .find(|rule| rule.id.as_str().eq_ignore_ascii_case(id))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rules_are_sorted_by_unique_id() {
        let ids: Vec<&str> = RULES.iter().map(|rule| rule.id.as_str()).collect();
        assert!(ids.windows(2).all(|pair| pair[0] < pair[1]), "{ids:?}");
    }

    #[test]
    fn every_rule_is_documented() {
        assert!(RULES.iter().all(|rule| {
            !rule.title.is_empty()
                && !rule.doc.scenario.is_empty()
                && !rule.doc.fix.is_empty()
                && !rule.doc.bad_example.is_empty()
                && !rule.doc.good_example.is_empty()
        }));
    }
}

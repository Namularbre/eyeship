//! Comparison of two reports (spec 004 US9): what a change fixed or broke.

use std::collections::BTreeMap;

/// Findings sorted into new, resolved and unchanged.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diff<'a, T> {
    /// In `after` only.
    pub new: Vec<&'a T>,
    /// In `before` only.
    pub resolved: Vec<&'a T>,
    /// In both; the `after` version is kept.
    pub unchanged: Vec<&'a T>,
}

/// Compares two lists of findings by `key`, as multisets: two findings with
/// the same key are paired in order, so a duplicate that disappears counts
/// as resolved. Each output list keeps the order of its source.
pub fn diff<'a, T, K: Ord>(before: &'a [T], after: &'a [T], key: impl Fn(&T) -> K) -> Diff<'a, T> {
    let count = |items: &'a [T]| {
        items.iter().fold(BTreeMap::new(), |mut counts, item| {
            *counts.entry(key(item)).or_insert(0usize) += 1;
            counts
        })
    };
    let (before_counts, after_counts) = (count(before), count(after));
    // Keeps, in order, the first `limit(key)` items of each key.
    let take = |items: &'a [T], limit: &dyn Fn(&K) -> usize, keep: bool| -> Vec<&'a T> {
        let mut seen: BTreeMap<K, usize> = BTreeMap::new();
        items
            .iter()
            .filter(|item| {
                let k = key(item);
                let shared = limit(&k);
                let rank = seen.entry(k).or_insert(0);
                *rank += 1;
                (*rank <= shared) == keep
            })
            .collect()
    };
    let shared = |k: &K| {
        before_counts
            .get(k)
            .copied()
            .unwrap_or(0)
            .min(after_counts.get(k).copied().unwrap_or(0))
    };
    Diff {
        new: take(after, &shared, false),
        resolved: take(before, &shared, false),
        unchanged: take(after, &shared, true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (rule, line): the line is not part of the key.
    type Item = (&'static str, u32);

    fn key(item: &Item) -> &'static str {
        item.0
    }

    fn rules<'a>(items: &[&'a Item]) -> Vec<&'a str> {
        items.iter().map(|item| item.0).collect()
    }

    #[test]
    fn sorts_findings_into_new_resolved_and_unchanged() {
        let before: [Item; 3] = [("R001", 1), ("R002", 2), ("R003", 3)];
        let after: [Item; 3] = [("R002", 2), ("R003", 3), ("R004", 4)];
        let diff = diff(&before, &after, key);
        assert_eq!(rules(&diff.new), ["R004"]);
        assert_eq!(rules(&diff.resolved), ["R001"]);
        assert_eq!(rules(&diff.unchanged), ["R002", "R003"]);
    }

    #[test]
    fn a_moved_finding_is_unchanged() {
        let before: [Item; 1] = [("R001", 10)];
        let after: [Item; 1] = [("R001", 42)];
        let diff = diff(&before, &after, key);
        assert!(diff.new.is_empty() && diff.resolved.is_empty());
        assert_eq!(diff.unchanged, [&("R001", 42)]);
    }

    #[test]
    fn duplicates_are_paired_one_by_one() {
        let before: [Item; 3] = [("R001", 1), ("R001", 2), ("R001", 3)];
        let after: [Item; 1] = [("R001", 5)];
        let diff = diff(&before, &after, key);
        assert_eq!(diff.unchanged.len(), 1);
        assert_eq!(diff.resolved, [&("R001", 2), &("R001", 3)]);
        let reverse = super::diff(&after, &before, key);
        assert_eq!(reverse.new.len(), 2);
    }

    #[test]
    fn empty_reports() {
        let none: [Item; 0] = [];
        let some: [Item; 1] = [("R001", 1)];
        assert_eq!(diff(&none, &some, key).new.len(), 1);
        assert_eq!(diff(&some, &none, key).resolved.len(), 1);
        assert!(diff(&none, &none, key).unchanged.is_empty());
    }
}

use crate::error::{Error, Result};
use std::collections::{HashMap, HashSet};

/// Reserve first-available names without rescanning an occupied prefix.
/// Names are only added during one import, so every cursor can move forward.
pub(super) struct Names {
    used: HashSet<String>,
    next: HashMap<String, usize>,
    #[cfg(test)]
    probes: usize,
}

impl Names {
    pub(super) fn new(used: impl Iterator<Item = String>) -> Self {
        Self {
            used: used.collect(),
            next: HashMap::new(),
            #[cfg(test)]
            probes: 0,
        }
    }

    pub(super) fn allocate(&mut self, base: &str) -> Result<String> {
        let next = self.next.entry(base.to_owned()).or_insert(1);
        loop {
            let index = *next;
            let following = index.checked_add(1).ok_or(Error::Safe(
                "Proxy identifier space exhausted; configuration unchanged.",
            ))?;
            let candidate = if index == 1 {
                base.to_owned()
            } else {
                format!("{base}-{index}")
            };
            #[cfg(test)]
            {
                self.probes += 1;
            }
            *next = following;
            if self.used.insert(candidate.clone()) {
                return Ok(candidate);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn previous(used: &mut HashSet<String>, base: &str) -> String {
        // Independent reference for the exact pre-fix first-free naming rule.
        let mut name = base.to_owned();
        let mut suffix = 2;
        while used.contains(&name) {
            name = format!("{base}-{suffix}");
            suffix += 1;
        }
        used.insert(name.clone());
        name
    }

    #[test]
    fn exhaustive_small_histories_match_the_previous_naming_rule() {
        let candidates = ["pool", "pool-2", "pool-3", "pool-2-2", "q", "q-2"];
        let bases = ["pool", "pool-2", "q"];
        for mask in 0..(1usize << candidates.len()) {
            let initial: HashSet<String> = candidates
                .iter()
                .enumerate()
                .filter(|(index, _)| mask & (1 << index) != 0)
                .map(|(_, name)| (*name).to_owned())
                .collect();
            for sequence in 0..81 {
                let mut reference = initial.clone();
                let mut actual = Names::new(initial.clone().into_iter());
                let mut code = sequence;
                for _ in 0..4 {
                    let base = bases[code % bases.len()];
                    code /= bases.len();
                    assert_eq!(
                        actual.allocate(base).unwrap(),
                        previous(&mut reference, base)
                    );
                }
                assert_eq!(actual.used, reference);
            }
        }
    }

    #[test]
    fn repeated_base_has_a_linear_candidate_probe_budget() {
        let count = 50_000;
        let mut names = Names::new(std::iter::empty());
        for index in 1..=count {
            let expected = if index == 1 {
                "pool".to_owned()
            } else {
                format!("pool-{index}")
            };
            assert_eq!(names.allocate("pool").unwrap(), expected);
        }
        assert_eq!(names.probes, count);
        assert_eq!(names.used.len(), count);
    }

    #[test]
    fn existing_dense_prefix_is_scanned_only_once_per_import() {
        let existing = 8_000;
        let added = 8_000;
        let initial = (1..=existing).map(|index| {
            if index == 1 {
                "p".to_owned()
            } else {
                format!("p-{index}")
            }
        });
        let mut names = Names::new(initial);
        for index in existing + 1..=existing + added {
            assert_eq!(names.allocate("p").unwrap(), format!("p-{index}"));
        }
        assert_eq!(names.probes, existing + added);
    }

    #[test]
    fn holes_nested_bases_case_and_numeric_spelling_remain_compatible() {
        let mut reference: HashSet<String> = ["p", "p-3", "p-02", "p-2-2", "P", "p-100"]
            .into_iter()
            .map(str::to_owned)
            .collect();
        let mut names = Names::new(reference.clone().into_iter());
        for base in ["p", "p", "p-2", "P", "p-02", "p", "p-3", "p-2", "P"] {
            assert_eq!(
                names.allocate(base).unwrap(),
                previous(&mut reference, base)
            );
        }
        assert_eq!(names.used, reference);
    }

    #[test]
    fn cursor_state_does_not_leak_between_independent_imports() {
        let mut first = Names::new(std::iter::empty());
        first.allocate("p").unwrap();
        first.allocate("p").unwrap();
        let mut second = Names::new(["p".to_owned(), "p-3".to_owned()].into_iter());
        assert_eq!(second.allocate("p").unwrap(), "p-2");
        assert_eq!(second.allocate("p").unwrap(), "p-4");
    }

    #[test]
    fn exhausted_cursor_is_a_safe_error_without_reserving_a_name() {
        let mut names = Names::new(std::iter::empty());
        names.next.insert("p".to_owned(), usize::MAX);
        assert!(names.allocate("p").is_err());
        assert!(names.used.is_empty());
    }
}

//! Shared overload ordering.
//!
//! Applicability is intentionally owned by the caller because constructors,
//! top-level methods, and class members expose different declaration shapes.
//! Once applicable candidates have been collected, however, Apex's "single
//! undominated candidate" rule must be identical for every call surface.

pub(super) fn unique_most_specific<T>(
    candidates: &[T],
    same_candidate: impl Fn(&T, &T) -> bool,
    more_specific: impl Fn(&T, &T) -> bool,
) -> Option<usize> {
    let mut selected = None;
    for (candidate_index, candidate) in candidates.iter().enumerate() {
        let dominated = candidates
            .iter()
            .any(|other| !same_candidate(other, candidate) && more_specific(other, candidate));
        if dominated {
            continue;
        }
        if selected.replace(candidate_index).is_some() {
            return None;
        }
    }
    selected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct Candidate {
        id: usize,
        rank: [usize; 2],
    }

    fn dominates(left: &Candidate, right: &Candidate) -> bool {
        let no_worse = left
            .rank
            .iter()
            .zip(right.rank)
            .all(|(left, right)| left <= &right);
        let strictly_better = left
            .rank
            .iter()
            .zip(right.rank)
            .any(|(left, right)| left < &right);
        no_worse && strictly_better
    }

    #[test]
    fn selects_the_only_undominated_candidate() {
        let candidates = [
            Candidate {
                id: 0,
                rank: [1, 1],
            },
            Candidate {
                id: 1,
                rank: [0, 0],
            },
            Candidate {
                id: 2,
                rank: [0, 1],
            },
        ];

        assert_eq!(
            unique_most_specific(&candidates, |left, right| left.id == right.id, dominates,),
            Some(1)
        );
    }

    #[test]
    fn rejects_crossing_candidates_as_ambiguous() {
        let candidates = [
            Candidate {
                id: 0,
                rank: [0, 1],
            },
            Candidate {
                id: 1,
                rank: [1, 0],
            },
        ];

        assert_eq!(
            unique_most_specific(&candidates, |left, right| left.id == right.id, dominates,),
            None
        );
    }

    #[test]
    fn returns_none_for_an_empty_candidate_set() {
        let candidates: [Candidate; 0] = [];

        assert_eq!(
            unique_most_specific(&candidates, |left, right| left.id == right.id, dominates,),
            None
        );
    }
}

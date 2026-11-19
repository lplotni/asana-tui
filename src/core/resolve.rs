//! Names where GIDs are taken (F19): a name resolves to one thing only when
//! it matches exactly or is the only candidate. Anything else is returned as
//! [`Unresolved`] for the caller to show, and nothing is written.

use crate::api::tasks::Named;

/// A name that didn't resolve to exactly one thing: the candidates found
/// (empty when nothing matched), so the caller can retry with a GID.
#[derive(Debug, Clone, PartialEq)]
pub struct Unresolved {
    /// What was looked for, singular: "person", "section", "project".
    pub kind: &'static str,
    pub name: String,
    pub candidates: Vec<Named>,
}

/// Whether `arg` is a GID (10 or more digits) rather than a name.
pub fn is_gid(arg: &str) -> bool {
    arg.len() >= 10 && arg.bytes().all(|b| b.is_ascii_digit())
}

/// The one candidate `name` means: a case-insensitive exact match wins,
/// otherwise a single candidate. Several exact matches, several candidates
/// without an exact match, or none are [`Unresolved`].
pub fn pick(kind: &'static str, name: &str, candidates: Vec<Named>) -> Result<Named, Unresolved> {
    let wanted = name.trim().to_lowercase();
    let exact: Vec<Named> = candidates
        .iter()
        .filter(|c| c.name.trim().to_lowercase() == wanted)
        .cloned()
        .collect();
    let mut choice = if exact.is_empty() { candidates } else { exact };
    if choice.len() == 1 {
        return Ok(choice.remove(0));
    }
    Err(Unresolved {
        kind,
        name: name.to_owned(),
        candidates: choice,
    })
}

/// [`pick`] among the candidates whose name contains `name` (ignoring
/// case), for lists that aren't already a search result (a project's
/// sections).
pub fn pick_containing(
    kind: &'static str,
    name: &str,
    candidates: Vec<Named>,
) -> Result<Named, Unresolved> {
    let wanted = name.trim().to_lowercase();
    let containing = candidates
        .into_iter()
        .filter(|c| c.name.to_lowercase().contains(&wanted))
        .collect();
    pick(kind, name, containing)
}

/// The candidate `arg` means in a list known up front (a task's fields, a
/// field's options): its GID, or a name as in [`pick_containing`]. When
/// nothing matches, every candidate is listed, so the caller sees the choice.
pub fn pick_local(
    kind: &'static str,
    arg: &str,
    candidates: Vec<Named>,
) -> Result<Named, Unresolved> {
    let picked = if is_gid(arg) {
        candidates
            .iter()
            .find(|c| c.gid == arg)
            .cloned()
            .ok_or_else(|| Unresolved {
                kind,
                name: arg.to_owned(),
                candidates: Vec::new(),
            })
    } else {
        pick_containing(kind, arg, candidates.clone())
    };
    picked.map_err(|mut unresolved| {
        if unresolved.candidates.is_empty() {
            unresolved.candidates = candidates;
        }
        unresolved
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn named(gid: &str, name: &str) -> Named {
        Named {
            gid: gid.into(),
            name: name.into(),
        }
    }

    #[test]
    fn ten_or_more_digits_are_a_gid() {
        assert!(is_gid("1200000000000011"));
        assert!(is_gid("1234567890"));
    }

    #[test]
    fn names_short_numbers_and_mixed_text_are_not_gids() {
        assert!(!is_gid("Ada Lovelace"));
        assert!(!is_gid("123456789"));
        assert!(!is_gid("1200000000000011x"));
        assert!(!is_gid(""));
    }

    #[test]
    fn an_exact_name_wins_over_other_candidates_ignoring_case() {
        let found = vec![
            named("11", "Ada Lovelace"),
            named("12", "Ada Lovelace-King"),
        ];

        let picked = pick("person", "ada lovelace", found);

        assert_eq!(picked, Ok(named("11", "Ada Lovelace")));
    }

    #[test]
    fn a_single_candidate_is_picked_without_an_exact_match() {
        let picked = pick("person", "ada", vec![named("11", "Ada Lovelace")]);

        assert_eq!(picked, Ok(named("11", "Ada Lovelace")));
    }

    #[test]
    fn several_candidates_without_an_exact_match_are_ambiguous() {
        let found = vec![named("11", "Ada Lovelace"), named("12", "Ada Byron")];

        let picked = pick("person", "ada", found.clone());

        assert_eq!(
            picked,
            Err(Unresolved {
                kind: "person",
                name: "ada".into(),
                candidates: found,
            })
        );
    }

    #[test]
    fn two_exact_matches_are_ambiguous_between_those_two() {
        let found = vec![
            named("11", "Ada Lovelace"),
            named("12", "Ada Lovelace"),
            named("13", "Ada Lovelace-King"),
        ];

        let picked = pick("person", "Ada Lovelace", found);

        assert_eq!(
            picked.unwrap_err().candidates,
            vec![named("11", "Ada Lovelace"), named("12", "Ada Lovelace")]
        );
    }

    #[test]
    fn no_candidates_is_unresolved_with_none_to_show() {
        let picked = pick("person", "Grace", Vec::new());

        assert_eq!(picked.unwrap_err().candidates, Vec::new());
    }

    #[test]
    fn containing_ignores_candidates_without_the_name() {
        let sections = vec![
            named("1", "Backlog"),
            named("2", "In Review"),
            named("3", "Done"),
        ];

        let picked = pick_containing("section", "review", sections);

        assert_eq!(picked, Ok(named("2", "In Review")));
    }

    #[test]
    fn containing_a_name_that_no_candidate_has_is_unresolved_with_none() {
        let picked = pick_containing("section", "Backlog", vec![named("3", "Done")]);

        assert_eq!(picked.unwrap_err().candidates, Vec::new());
    }

    #[test]
    fn containing_still_lets_an_exact_name_win() {
        let sections = vec![named("1", "Review"), named("2", "In Review")];

        let picked = pick_containing("section", "review", sections);

        assert_eq!(picked, Ok(named("1", "Review")));
    }
    #[test]
    fn local_lists_match_a_gid_exactly() {
        let options = vec![
            named("1200000000000062", "Checkout"),
            named("1200000000000063", "Onboarding"),
        ];

        let picked = pick_local("option", "1200000000000063", options);

        assert_eq!(picked, Ok(named("1200000000000063", "Onboarding")));
    }

    #[test]
    fn local_lists_match_a_name_like_sections() {
        let options = vec![named("1", "Checkout"), named("2", "Onboarding")];

        assert_eq!(
            pick_local("option", "board", options),
            Ok(named("2", "Onboarding"))
        );
    }

    #[test]
    fn nothing_matching_in_a_local_list_lists_every_candidate() {
        let options = vec![named("1", "Checkout"), named("2", "Onboarding")];

        let unresolved = pick_local("option", "Mobile", options.clone()).unwrap_err();

        assert_eq!(unresolved.candidates, options);
    }

    #[test]
    fn an_unknown_gid_in_a_local_list_lists_every_candidate() {
        let options = vec![named("1", "Checkout")];

        let unresolved = pick_local("option", "1200000000000099", options.clone()).unwrap_err();

        assert_eq!(unresolved.candidates, options);
    }
}

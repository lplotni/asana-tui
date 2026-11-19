//! Output formatting for CLI subcommands.

pub mod board;
pub mod inbox;
pub mod search;
pub mod task;

use crate::present::format::is_bidi_control;

/// `text` made safe to print to a terminal. Asana text (names, comments,
/// descriptions) is written by other people: control characters (escape
/// sequences, tabs, newlines) become spaces and bidi overrides are dropped,
/// so it can't move the cursor, set the clipboard or title, reorder what's
/// displayed, or break line- and tab-based output.
pub fn terminal_safe(text: &str) -> String {
    text.chars()
        .filter(|&c| !is_bidi_control(c))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
}

/// One tab-separated line per item: `gid  name` (sections, people).
pub fn render_named(items: &[crate::api::tasks::Named]) -> String {
    items.iter().map(|i| tsv_line(&[&i.gid, &i.name])).collect()
}

/// What to print (on stderr) when a name didn't resolve to one thing (F19):
/// the candidates as `gid  name` lines, or that nothing matched.
pub fn render_unresolved(unresolved: &crate::core::resolve::Unresolved) -> String {
    let name = terminal_safe(&unresolved.name);
    let kind = unresolved.kind;
    if unresolved.candidates.is_empty() {
        return format!("no {kind} matches \"{name}\"\n");
    }
    format!("\"{name}\" matches more than one {kind}; use a GID:\n")
        + &render_named(&unresolved.candidates)
}

/// `asn project fields`: each single-select field (`gid  name`), then its
/// enabled options as indented `gid  name` lines.
pub fn render_fields(fields: &[crate::api::projects::SelectField]) -> String {
    let mut text = String::new();
    for field in fields {
        text += &tsv_line(&[&field.gid, &field.name]);
        for option in field.options.iter().filter(|o| o.enabled) {
            text += &tsv_line(&["", &option.gid, &option.name]);
        }
    }
    text
}

/// `asn task field`: `<Field>: <Option>`, or `<Field>: none` when cleared.
pub fn render_field_set(set: &crate::core::FieldSet) -> String {
    let option = set.option.as_deref().unwrap_or("none");
    format!("{}: {}\n", terminal_safe(&set.field), terminal_safe(option))
}

/// One line of tab-separated output, every field made [`terminal_safe`]
/// (so a field can't contain a tab or newline that breaks the columns).
pub fn tsv_line(fields: &[&str]) -> String {
    let safe: Vec<String> = fields.iter().map(|f| terminal_safe(f)).collect();
    safe.join("\t") + "\n"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_items_are_listed_one_per_line_as_gid_and_name() {
        use crate::api::tasks::Named;
        let items = [
            Named {
                gid: "1".into(),
                name: "To do".into(),
            },
            Named {
                gid: "2".into(),
                name: "Done\u{1b}[2J".into(),
            },
        ];

        assert_eq!(render_named(&items), "1\tTo do\n2\tDone [2J\n");
    }

    #[test]
    fn an_ambiguous_name_lists_the_candidates_to_retry_with() {
        use crate::api::tasks::Named;
        use crate::core::resolve::Unresolved;
        let unresolved = Unresolved {
            kind: "person",
            name: "ada".into(),
            candidates: vec![
                Named {
                    gid: "1200000000000011".into(),
                    name: "Ada Lovelace".into(),
                },
                Named {
                    gid: "1200000000000012".into(),
                    name: "Ada\u{202e} Byron".into(),
                },
            ],
        };

        assert_eq!(
            render_unresolved(&unresolved),
            "\"ada\" matches more than one person; use a GID:\n\
             1200000000000011\tAda Lovelace\n\
             1200000000000012\tAda Byron\n"
        );
    }

    #[test]
    fn an_unknown_name_says_nothing_matched() {
        use crate::core::resolve::Unresolved;
        let unresolved = Unresolved {
            kind: "person",
            name: "Grace\u{1b}[2J".into(),
            candidates: Vec::new(),
        };

        assert_eq!(
            render_unresolved(&unresolved),
            "no person matches \"Grace [2J\"\n"
        );
    }

    #[test]
    fn project_fields_list_each_field_then_its_enabled_options() {
        use crate::api::projects::{SelectField, SelectOption};
        let option = |gid: &str, name: &str, enabled: bool| SelectOption {
            gid: gid.into(),
            name: name.into(),
            enabled,
        };
        let fields = [SelectField {
            gid: "61".into(),
            name: "Initiative".into(),
            options: vec![
                option("62", "Checkout", true),
                option("64", "Legacy", false),
                option("63", "On\u{1b}[2Jboarding", true),
            ],
        }];

        assert_eq!(
            render_fields(&fields),
            "61\tInitiative\n\t62\tCheckout\n\t63\tOn [2Jboarding\n"
        );
    }

    #[test]
    fn a_set_field_reads_field_colon_option() {
        use crate::core::FieldSet;
        let set = FieldSet {
            field: "Initiative".into(),
            option: Some("Checkout".into()),
        };
        let cleared = FieldSet {
            field: "Initiative".into(),
            option: None,
        };

        assert_eq!(render_field_set(&set), "Initiative: Checkout\n");
        assert_eq!(render_field_set(&cleared), "Initiative: none\n");
    }

    #[test]
    fn escape_sequences_lose_their_escape_character() {
        let osc52 = "\u{1b}]52;c;aGk=\u{7}hi \u{1b}[2J";

        let safe = terminal_safe(osc52);

        assert!(!safe.chars().any(char::is_control), "{safe:?}");
        assert!(safe.contains("hi"));
    }

    #[test]
    fn tabs_and_newlines_become_spaces() {
        assert_eq!(terminal_safe("a\tb\nc\r"), "a b c ");
    }

    #[test]
    fn bidi_overrides_are_removed() {
        assert_eq!(terminal_safe("abc\u{202e}fed\u{2066}x\u{2069}"), "abcfedx");
    }

    #[test]
    fn ordinary_text_is_unchanged() {
        assert_eq!(
            terminal_safe("Frédéric · ✓ 日本 🎉"),
            "Frédéric · ✓ 日本 🎉"
        );
    }
}

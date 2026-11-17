//! Output formatting for CLI subcommands.

pub mod inbox;
pub mod project;
pub mod search;
pub mod task;

/// `text` made safe to print to a terminal. Asana text (names, comments,
/// descriptions) is written by other people: control characters (escape
/// sequences, tabs, newlines) become spaces and bidi overrides are dropped,
/// so it can't move the cursor, set the clipboard or title, reorder what's
/// displayed, or break line- and tab-based output.
pub fn terminal_safe(text: &str) -> String {
    text.chars()
        .filter(|c| !matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
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

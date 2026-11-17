//! `asn project` output.

use super::terminal_safe;
use crate::api::tasks::Named;

/// One tab-separated line per section: `gid  name`.
pub fn render_sections(sections: &[Named]) -> String {
    sections
        .iter()
        .map(|s| format!("{}\t{}\n", terminal_safe(&s.gid), terminal_safe(&s.name)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sections_are_listed_one_per_line_as_gid_and_name() {
        let sections = [
            Named {
                gid: "1".into(),
                name: "To do".into(),
            },
            Named {
                gid: "2".into(),
                name: "Done\u{1b}[2J".into(),
            },
        ];

        assert_eq!(render_sections(&sections), "1\tTo do\n2\tDone [2J\n");
    }
}

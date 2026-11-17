//! `asn project` output.

use super::tsv_line;
use crate::api::tasks::Named;

/// One tab-separated line per section: `gid  name`.
pub fn render_sections(sections: &[Named]) -> String {
    sections
        .iter()
        .map(|s| tsv_line(&[&s.gid, &s.name]))
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

//! @-mentions: Asana stores them as rich text, `<a data-asana-gid="GID"/>`
//! inside `<body>`. Written text keeps them as `@Label`; this turns that
//! text into what Asana expects.

/// An `@Label` in written text that stands for an Asana user, task or
/// project.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    /// As written, including the `@`: "@Ada Lovelace".
    pub label: String,
    pub gid: String,
}

/// The fewest digits after `@` that count as a GID in typed text, so "@3pm"
/// or "@2026" stay text. Asana GIDs are 16 digits.
const MIN_GID_DIGITS: usize = 10;

/// `text` as Asana rich text: each `@Label` of `mentions` (longest first
/// where they overlap) becomes a mention tag, everything else is escaped,
/// and line breaks are kept. `None` when no mention is in the text, so it
/// can be saved as plain text.
pub fn rich_text(text: &str, mentions: &[Mention]) -> Option<String> {
    let mut by_length: Vec<&Mention> = mentions.iter().filter(|m| !m.label.is_empty()).collect();
    by_length.sort_by_key(|m| std::cmp::Reverse(m.label.len()));
    let mut body = String::from("<body>");
    let mut tagged = false;
    let mut rest = text;
    while let Some(c) = rest.chars().next() {
        if let Some(m) = by_length.iter().find(|m| rest.starts_with(&m.label)) {
            body.push_str(&format!(r#"<a data-asana-gid="{}"/>"#, m.gid));
            rest = &rest[m.label.len()..];
            tagged = true;
            continue;
        }
        match c {
            '&' => body.push_str("&amp;"),
            '<' => body.push_str("&lt;"),
            '>' => body.push_str("&gt;"),
            c => body.push(c),
        }
        rest = &rest[c.len_utf8()..];
    }
    body.push_str("</body>");
    tagged.then_some(body)
}

/// Mentions typed as `@` and a GID (at least [`MIN_GID_DIGITS`] digits, at
/// the start of a word), as the CLI takes them. Each GID once.
pub fn gid_mentions(text: &str) -> Vec<Mention> {
    let mut found: Vec<Mention> = Vec::new();
    let mut previous: Option<char> = None;
    for (at, c) in text.char_indices() {
        let starts_word = previous.is_none_or(|p| !p.is_alphanumeric());
        previous = Some(c);
        if c != '@' || !starts_word {
            continue;
        }
        let digits: String = text[at + 1..]
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        let ends_word = text[at + 1 + digits.len()..]
            .chars()
            .next()
            .is_none_or(|next| !next.is_alphanumeric());
        if digits.len() >= MIN_GID_DIGITS && ends_word && !found.iter().any(|m| m.gid == digits) {
            found.push(Mention {
                label: format!("@{digits}"),
                gid: digits,
            });
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mention(label: &str, gid: &str) -> Mention {
        Mention {
            label: label.into(),
            gid: gid.into(),
        }
    }

    #[test]
    fn text_without_a_mention_stays_plain() {
        assert_eq!(rich_text("Looks good", &[mention("@Ada", "7")]), None);
        assert_eq!(rich_text("Looks good", &[]), None);
    }

    #[test]
    fn a_mention_becomes_a_tag_inside_body() {
        assert_eq!(
            rich_text(
                "Looks good @Ada Lovelace!",
                &[mention("@Ada Lovelace", "7")]
            ),
            Some(r#"<body>Looks good <a data-asana-gid="7"/>!</body>"#.into())
        );
    }

    #[test]
    fn everything_else_is_escaped_and_line_breaks_are_kept() {
        assert_eq!(
            rich_text("a < b & c > d\n@Ada", &[mention("@Ada", "7")]),
            Some("<body>a &lt; b &amp; c &gt; d\n<a data-asana-gid=\"7\"/></body>".into())
        );
    }

    #[test]
    fn every_occurrence_of_every_mention_is_tagged_longest_label_first() {
        let mentions = [mention("@Ada", "1"), mention("@Ada Lovelace", "2")];

        assert_eq!(
            rich_text("@Ada Lovelace and @Ada and @Ada Lovelace", &mentions),
            Some(
                r#"<body><a data-asana-gid="2"/> and <a data-asana-gid="1"/> and <a data-asana-gid="2"/></body>"#
                    .into()
            )
        );
    }

    #[test]
    fn a_mention_edited_away_is_plain_text() {
        assert_eq!(
            rich_text("thanks @Ada Love", &[mention("@Ada Lovelace", "7")]),
            None
        );
    }

    #[test]
    fn an_at_sign_followed_by_a_long_gid_is_a_mention() {
        assert_eq!(
            gid_mentions("ping @1200000000000011, and @1200000000000011 again"),
            [mention("@1200000000000011", "1200000000000011")]
        );
    }

    #[test]
    fn short_numbers_and_emails_are_not_mentions() {
        assert_eq!(gid_mentions("meet @3pm or mail a@1200000000000011.com"), []);
    }
}

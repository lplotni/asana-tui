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

/// Where Asana's own links start; mentions show as these in plain text.
const ASANA_URL: &str = "https://app.asana.com/";

/// `text` (Asana's plain `text` or `notes`) with each mention shown by its
/// label: Asana writes a mention there as a URL ending in the mentioned GID,
/// and `html` (the matching `html_text` or `html_notes`) names it. URLs that
/// aren't mentions stay; without `html` the text is unchanged.
pub fn readable(text: &str, html: Option<&str>) -> String {
    let labels = html.map(mention_labels).unwrap_or_default();
    if labels.is_empty() {
        return text.to_owned();
    }
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(ASANA_URL) {
        out.push_str(&rest[..at]);
        let url = rest[at..]
            .split(char::is_whitespace)
            .next()
            .unwrap_or_default();
        // Sentence punctuation after a link isn't part of it.
        let link = url.trim_end_matches(['.', ',', ';', ':', '!', '?', ')', ']', '"', '\'']);
        // A link's `href` is the URL in the plain text. Its GID is no help
        // for people: a profile URL ends in the profile's id, not the user's.
        let mention = labels
            .iter()
            .find(|m| m.href.as_deref() == Some(link))
            .or_else(|| last_gid(link).and_then(|gid| labels.iter().find(|m| m.gid == gid)));
        match mention.map(|m| &m.label) {
            Some(label) => {
                out.push_str(label);
                out.push_str(&url[link.len()..]);
            }
            None => out.push_str(url),
        }
        rest = &rest[at + url.len()..];
    }
    out.push_str(rest);
    out
}

/// The last all-digit path segment of an Asana URL: the object it links to.
fn last_gid(url: &str) -> Option<&str> {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.split('/')
        .rev()
        .find(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
}

/// A mention link in Asana rich text.
struct Linked {
    href: Option<String>,
    gid: String,
    label: String,
}

/// Every mention link (`<a … data-asana-gid="…">label</a>`) in Asana rich
/// text. A person's label always starts with `@`.
fn mention_labels(html: &str) -> Vec<Linked> {
    let mut labels = Vec::new();
    let mut rest = html;
    while let Some(start) = rest.find("<a ") {
        rest = &rest[start..];
        let Some(open_end) = rest.find('>') else {
            break;
        };
        let tag = &rest[..open_end];
        let inner_end = rest.find("</a>").unwrap_or(rest.len());
        let inner = rest.get(open_end + 1..inner_end).unwrap_or_default();
        rest = &rest[open_end..];
        let Some(gid) = attribute(tag, "data-asana-gid") else {
            continue;
        };
        let mut label = unescape(&strip_tags(inner));
        if label.trim().is_empty() {
            continue;
        }
        if attribute(tag, "data-asana-type") == Some("user") && !label.starts_with('@') {
            label.insert(0, '@');
        }
        labels.push(Linked {
            href: attribute(tag, "href").map(unescape),
            gid: gid.to_owned(),
            label,
        });
    }
    labels
}

/// The value of `name="…"` in an HTML tag.
fn attribute<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let start = tag.find(&format!("{name}=\""))? + name.len() + 2;
    let len = tag[start..].find('"')?;
    Some(&tag[start..start + len])
}

fn strip_tags(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text
}

fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#x27;", "'")
        .replace("&amp;", "&")
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

    const ADA_HTML: &str = r#"<body>Thanks <a href="https://app.asana.com/1/1200000000000001/profile/1200000000000011" data-asana-gid="1200000000000011" data-asana-accessible="true" data-asana-type="user" data-asana-dynamic="true">@Ada Lovelace</a>!</body>"#;

    #[test]
    fn a_profile_url_reads_as_the_persons_name() {
        let text = "Thanks https://app.asana.com/1/1200000000000001/profile/1200000000000011!";

        assert_eq!(readable(text, Some(ADA_HTML)), "Thanks @Ada Lovelace!");
    }

    /// As Asana returns it: the profile URL's id is not the user's GID, so
    /// only the link's `href` ties the plain text to the mention.
    #[test]
    fn a_profile_url_whose_id_is_not_the_user_gid_reads_as_the_name() {
        let html = r#"<body>So we stop? <a href="https://app.asana.com/1/1200000000000001/profile/1200000000000099" data-asana-gid="1200000000000011" data-asana-accessible="true" data-asana-type="user" data-asana-dynamic="true">@Frédéric Ünal</a></body>"#;
        let text = "So we stop? https://app.asana.com/1/1200000000000001/profile/1200000000000099";

        assert_eq!(readable(text, Some(html)), "So we stop? @Frédéric Ünal");
    }

    #[test]
    fn several_mentions_in_parentheses_all_read_as_names() {
        let html = r#"<body>(<a href="https://app.asana.com/1/1/profile/91" data-asana-gid="11" data-asana-type="user">@Ada Lovelace</a> will check) and I (<a href="https://app.asana.com/1/1/profile/92" data-asana-gid="12" data-asana-type="user">@Grace Hopper</a>).</body>"#;
        let text = "(https://app.asana.com/1/1/profile/91 will check) and I (https://app.asana.com/1/1/profile/92).";

        assert_eq!(
            readable(text, Some(html)),
            "(@Ada Lovelace will check) and I (@Grace Hopper)."
        );
    }

    #[test]
    fn the_old_profile_url_format_reads_as_the_name_too() {
        let text = "cc https://app.asana.com/0/profile/1200000000000011";

        assert_eq!(readable(text, Some(ADA_HTML)), "cc @Ada Lovelace");
    }

    #[test]
    fn a_task_url_reads_as_the_task_name_and_keeps_punctuation() {
        let html = r#"<body>see <a href="https://app.asana.com/0/1200000000000042/1200000000000031/f" data-asana-gid="1200000000000031" data-asana-type="task">Cookie banner</a>.</body>"#;
        let text = "see https://app.asana.com/0/1200000000000042/1200000000000031/f.";

        assert_eq!(readable(text, Some(html)), "see Cookie banner.");
    }

    #[test]
    fn labels_are_unescaped_and_people_always_get_an_at_sign() {
        let html = r#"<body><a data-asana-gid="1200000000000012" data-asana-type="user">Tom &amp; Jerry</a></body>"#;
        let text = "https://app.asana.com/0/profile/1200000000000012";

        assert_eq!(readable(text, Some(html)), "@Tom & Jerry");
    }

    #[test]
    fn urls_that_are_not_mentions_stay() {
        let text = "docs at https://app.asana.com/0/1200000000000099/list and https://example.com/1200000000000011";

        assert_eq!(readable(text, Some(ADA_HTML)), text);
    }

    #[test]
    fn without_html_the_text_is_unchanged() {
        let text = "Thanks https://app.asana.com/0/profile/1200000000000011";

        assert_eq!(readable(text, None), text);
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

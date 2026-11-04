//! Classify links once when rebuilding the canonical visible-message projection.
use linkify::{LinkFinder, LinkKind};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};

fn web_destination(value: &str) -> bool {
    url::Url::parse(value)
        .is_ok_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
}

pub(crate) fn contains_web_link(text: &str) -> bool {
    let mut finder = LinkFinder::new();
    finder.kinds(&[LinkKind::Url]);
    finder.url_must_have_scheme(false);
    let mut suppressed = 0_u32;
    for event in Parser::new_ext(text, Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TABLES) {
        match event {
            Event::Start(Tag::Link { dest_url, .. }) => {
                if suppressed == 0 && web_destination(&dest_url) {
                    return true;
                }
                suppressed += 1;
            }
            Event::Start(Tag::CodeBlock(_) | Tag::Image { .. }) => suppressed += 1,
            Event::End(TagEnd::CodeBlock | TagEnd::Image | TagEnd::Link) => {
                suppressed = suppressed.saturating_sub(1);
            }
            Event::Text(value)
                if suppressed == 0
                    && finder.links(&value).any(|link| {
                        let value = link.as_str();
                        web_destination(value)
                            || (!value.contains(':')
                                && web_destination(&format!("https://{value}")))
                    }) =>
            {
                return true;
            }
            _ => {}
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::contains_web_link;
    #[test]
    fn rendered_web_entities_exclude_code_images_emails_and_unsafe_destinations() {
        for text in [
            "See https://example.org/path?x=1.",
            "www.example.org",
            "example.org/path",
            "https://例え.jp/道",
            "[Read the release](https://example.org/release)",
            "[Read][release]\n\n[release]: https://example.org/release",
            "<https://example.org>",
            "**https://example.org**",
            "| Link |\n| --- |\n| https://example.org |",
        ] {
            assert!(contains_web_link(text), "{text}");
        }
        for text in [
            "plain text",
            "person@example.org",
            "<person@example.org>",
            "`https://example.org`",
            "```text\nhttps://example.org\n```",
            "    https://example.org",
            "![https://example.org](https://example.org/image.png)",
            "[https://example.org](javascript:alert)",
            "[local](file:///private/doc)",
            "[mail](mailto:person@example.org)",
            "[broken](https://)",
            "<!-- https://example.org -->",
        ] {
            assert!(!contains_web_link(text), "{text}");
        }
    }
}

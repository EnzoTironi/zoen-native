//! Bounded display attribution, asserted by a forwarder rather than the original author.

/// Names use the existing profile's 64 Unicode-scalar limit. This metadata carries
/// neither a peer identifier nor a source conversation/message reference.
#[must_use]
pub fn author_valid(forwarded: bool, name: Option<&str>) -> bool {
    name.is_none_or(|name| {
        forwarded
            && !name.is_empty()
            && name.trim() == name
            && name.chars().count() <= 64
            && !name.chars().any(|c| {
                c.is_control() || matches!(c, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}')
            })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_is_bounded_and_never_turns_an_ordinary_message_into_a_forward() {
        assert!(author_valid(false, None));
        assert!(author_valid(true, None));
        assert!(author_valid(true, Some("Marina 雨 😀")));
        assert!(author_valid(true, Some(&"雨".repeat(64))));
        for name in [
            "",
            " leading",
            "trailing ",
            "two\nlines",
            "fake\u{202e}name",
            "a\0b",
        ] {
            assert!(!author_valid(true, Some(name)), "{name:?}");
        }
        assert!(!author_valid(true, Some(&"a".repeat(65))));
        assert!(!author_valid(false, Some("Marina")));
    }
    #[test]
    fn ordinary_signed_messages_and_voice_keep_their_legacy_bytes(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let body = crate::EventBody::MessagePosted {
            message: "m".into(),
            text: "t".into(),
            attaches: None,
            media: None,
            reply: None,
            forwarded: false,
            forwarded_author: None,
        };
        assert_eq!(
            serde_json::to_string(&body)?,
            r#"{"MessagePosted":{"message":"m","text":"t","attaches":null}}"#
        );
        assert_eq!(
            serde_json::to_string(&crate::VoiceMeta::default())?,
            r#"{"ms":0}"#
        );
        let legacy: crate::EventBody = serde_json::from_str(
            r#"{"MessagePosted":{"message":"m","text":"t","attaches":null,"forwarded":true}}"#,
        )?;
        assert!(matches!(
            legacy,
            crate::EventBody::MessagePosted {
                forwarded: true,
                forwarded_author: None,
                ..
            }
        ));
        Ok(())
    }
}

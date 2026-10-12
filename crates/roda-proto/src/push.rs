//! APNs input and wake-payload contract. Registration transport and provider delivery
//! must exist before the relay advertises `PUSH_REGISTRATION_CAPABILITY`.

use serde::{Deserialize, Deserializer, Serialize};
use std::fmt;

pub const PUSH_REGISTRATION_CAPABILITY: &str = "push_registration_v1";
pub const APNS_BACKGROUND_PUSH_TYPE: &str = "background";
pub const APNS_BACKGROUND_PRIORITY: &str = "5";
pub const MAX_APNS_TOKEN_HEX_BYTES: usize = 512;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PushEnvironment {
    Sandbox,
    Production,
}

impl PushEnvironment {
    pub fn parse(value: &str) -> Result<Self, PushInputError> {
        match value {
            "sandbox" => Ok(Self::Sandbox),
            "production" => Ok(Self::Production),
            _ => Err(PushInputError::Environment),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sandbox => "sandbox",
            Self::Production => "production",
        }
    }
}

#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(transparent)]
pub struct ApnsToken(String);

impl ApnsToken {
    /// APNs tokens have variable length. The protocol bounds input without assuming
    /// that every OS token is 32 bytes, and keeps one canonical hexadecimal form.
    pub fn parse(value: &str) -> Result<Self, PushInputError> {
        if value.is_empty()
            || value.len() > MAX_APNS_TOKEN_HEX_BYTES
            || value.len() % 2 != 0
            || !value.bytes().all(|byte| byte.is_ascii_hexdigit())
        {
            return Err(PushInputError::Token);
        }
        Ok(Self(value.to_ascii_lowercase()))
    }

    pub fn as_hex(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for ApnsToken {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("ApnsToken(<redacted>)")
    }
}

impl<'de> Deserialize<'de> for ApnsToken {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushInputError {
    Environment,
    Topic,
    Token,
    Space,
    Sequence,
}

impl fmt::Display for PushInputError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Environment => "invalid APNs environment",
            Self::Topic => "invalid APNs topic",
            Self::Token => "invalid APNs token",
            Self::Space => "invalid push Space id",
            Self::Sequence => "invalid push event sequence",
        })
    }
}

impl std::error::Error for PushInputError {}

pub fn validate_topic(topic: &str) -> Result<(), PushInputError> {
    if topic.is_empty()
        || topic.len() > 255
        || !topic.contains('.')
        || topic.split('.').any(|part| part.is_empty())
        || !topic
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'.' || byte == b'-')
    {
        return Err(PushInputError::Topic);
    }
    Ok(())
}

/// A sync hint only. The client still authenticates and validates Space membership
/// and the signed log; this payload cannot approve, execute or author anything.
pub fn encode_sync_wake(space_id: &str, event_seq: u64) -> Result<Vec<u8>, PushInputError> {
    if space_id.is_empty()
        || space_id.len() > 128
        || !space_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_' || byte == b'-')
    {
        return Err(PushInputError::Space);
    }
    if event_seq == 0 {
        return Err(PushInputError::Sequence);
    }
    Ok(serde_json::json!({
        "aps": { "content-available": 1 },
        "zoen": { "v": 1, "space_id": space_id, "event_seq": event_seq.to_string() }
    })
    .to_string()
    .into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wake_has_only_versioned_sync_fields_and_an_exact_decimal_cursor() {
        let payload = encode_sync_wake("sp_01jqtest", u64::MAX).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&payload).unwrap();
        assert_eq!(
            value,
            serde_json::json!({
                "aps": { "content-available": 1 },
                "zoen": { "v": 1, "space_id": "sp_01jqtest", "event_seq": "18446744073709551615" }
            })
        );
        assert_eq!(APNS_BACKGROUND_PUSH_TYPE, "background");
        assert_eq!(APNS_BACKGROUND_PRIORITY, "5");
        assert!(payload.len() < 4096);
    }

    #[test]
    fn wake_rejects_unbounded_or_unrouteable_hints() {
        for space in [
            "",
            "../other",
            "space with text",
            "space\0id",
            &"s".repeat(129),
        ] {
            assert_eq!(encode_sync_wake(space, 1), Err(PushInputError::Space));
        }
        assert_eq!(
            encode_sync_wake("sp_valid", 0),
            Err(PushInputError::Sequence)
        );
        assert!(encode_sync_wake(&"s".repeat(128), 1).unwrap().len() < 4096);
    }

    #[test]
    fn token_keeps_variable_lengths_but_rejects_bad_or_oversized_hex() {
        for bytes in [1, 16, 32, 64, 256] {
            let token = ApnsToken::parse(&"AB".repeat(bytes)).unwrap();
            assert_eq!(token.as_hex(), "ab".repeat(bytes));
            assert_eq!(
                serde_json::from_str::<ApnsToken>(&serde_json::to_string(&token).unwrap()).unwrap(),
                token
            );
        }
        for token in ["", "a", "zz", "a b", "<ab>", &"ab".repeat(257)] {
            assert_eq!(ApnsToken::parse(token), Err(PushInputError::Token));
        }
        let token = ApnsToken::parse(&"ab".repeat(32)).unwrap();
        assert!(!format!("{token:?}").contains(token.as_hex()));
        assert!(!PushInputError::Token.to_string().contains(token.as_hex()));
        assert!(serde_json::from_str::<ApnsToken>("\"<bad>\"").is_err());
    }

    #[test]
    fn environment_and_topic_are_explicit_and_do_not_admit_wildcards() {
        for environment in ["sandbox", "production"] {
            assert_eq!(
                PushEnvironment::parse(environment).unwrap().as_str(),
                environment
            );
        }
        for environment in ["", "development", "Production", "production\n"] {
            assert_eq!(
                PushEnvironment::parse(environment),
                Err(PushInputError::Environment)
            );
        }
        assert!(validate_topic("xyz.tironi.zoen").is_ok());
        for topic in [
            "",
            "xyz.*",
            "xyz..zoen",
            ".xyz",
            "xyz/zoen",
            "xyz.zoen\n",
            &"a".repeat(256),
        ] {
            assert_eq!(validate_topic(topic), Err(PushInputError::Topic));
        }
    }
}

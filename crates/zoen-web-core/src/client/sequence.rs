//! Relay sequences cross JavaScript boundaries as exact decimal strings.
use serde::{Serialize, Serializer};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct MessageSequence(pub(super) u64);

impl Serialize for MessageSequence {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::MessageSequence;

    #[test]
    fn optional_message_sequence_preserves_decimal_wire_and_pending_null(
    ) -> Result<(), serde_json::Error> {
        assert_eq!(
            serde_json::to_string(&Some(MessageSequence(u64::MAX)))?,
            "\"18446744073709551615\""
        );
        assert_eq!(serde_json::to_string(&None::<MessageSequence>)?, "null");
        assert_eq!(serde_json::to_string(&Some(MessageSequence(0)))?, "\"0\"");
        Ok(())
    }
}

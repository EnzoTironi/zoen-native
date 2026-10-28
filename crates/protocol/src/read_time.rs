//! Optional peer-clock evidence over the existing authenticated read watermark.
//! An interval covers only newly read sequence numbers, never a fabricated old date.

/// Detailed receipt audience follows the existing small-space full broadcast bound.
pub const MAX_MEMBERS: usize = 16;
/// Timestamp evidence expires independently of the permanent read watermark.
pub const RETENTION_MS: i64 = 7 * 24 * 60 * 60 * 1000;
/// Bound per-identity retained intervals and reconnect work.
pub const MAX_INTERVALS: usize = 256;

/// A device-reported clock value, authenticated as the receipt sender by the relay.
/// It is not a relay timestamp or a verified UTC clock.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReadTime {
    /// Exclusive old read prefix. Legacy prefixes gain no retroactive timestamp.
    pub after_seq: u64,
    /// Inclusive prefix observed by this explicit read.
    pub through_seq: u64,
    /// Sender device wall clock when the prefix advanced.
    pub reported_at_ms: i64,
}
impl ReadTime {
    /// Validate shape/range without pretending the peer's clock is trustworthy.
    #[must_use]
    pub fn valid(&self) -> bool {
        self.after_seq < self.through_seq
            && self.reported_at_ms > 0
            && self.reported_at_ms <= 253_402_300_799_999
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        wire::{pb_ephemeral, PbEphemeral},
        ClientFrame, EphemeralKind,
    };
    use prost::Message;
    #[derive(Clone, PartialEq, Message)]
    struct LegacyEphemeral {
        #[prost(string, tag = "1")]
        space: String,
        #[prost(string, tag = "2")]
        from: String,
        #[prost(oneof = "pb_ephemeral::Kind", tags = "3,4,5,6,7")]
        kind: Option<pb_ephemeral::Kind>,
    }
    #[test]
    fn optional_time_keeps_legacy_read_discriminator_and_exact_unsigned_prefix() {
        let time = ReadTime {
            after_seq: 7,
            through_seq: u64::MAX,
            reported_at_ms: 1234,
        };
        let frame = ClientFrame::Ephemeral {
            space: "space".into(),
            kind: EphemeralKind::ReadTime(time.clone()),
        };
        assert_eq!(ClientFrame::decode(&frame.encode()).unwrap(), frame);
        let wire = PbEphemeral {
            space: "space".into(),
            from: "peer".into(),
            kind: Some(pb_ephemeral::Kind::Read(time.through_seq)),
            read_time: Some(crate::wire::PbReadTime {
                after_seq: time.after_seq,
                reported_at_ms: time.reported_at_ms,
            }),
        };
        let legacy = LegacyEphemeral::decode(wire.encode_to_vec().as_slice()).unwrap();
        assert_eq!(legacy.kind, Some(pb_ephemeral::Kind::Read(u64::MAX)));
        assert!(PbEphemeral::decode(legacy.encode_to_vec().as_slice())
            .unwrap()
            .read_time
            .is_none());
    }
    #[test]
    fn invalid_interval_or_clock_is_rejected_not_replaced_with_local_now() {
        for time in [
            ReadTime {
                after_seq: 8,
                through_seq: 8,
                reported_at_ms: 1,
            },
            ReadTime {
                after_seq: 0,
                through_seq: 8,
                reported_at_ms: 0,
            },
        ] {
            let frame = ClientFrame::Ephemeral {
                space: "space".into(),
                kind: EphemeralKind::ReadTime(time),
            };
            assert!(ClientFrame::decode(&frame.encode()).is_err());
        }
        let old = ClientFrame::Ephemeral {
            space: "space".into(),
            kind: EphemeralKind::Read { seq: 8 },
        };
        assert_eq!(ClientFrame::decode(&old.encode()).unwrap(), old);
    }
}

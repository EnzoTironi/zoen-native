//! Validate caller configuration before a node can acquire or renew authority.
use super::{Duration, VERSIONS_PER_MS};

pub(super) struct Timing {
    pub ttl_versions: i64,
    pub interval: Duration,
}

impl Timing {
    pub fn new(node: &str, ttl: Duration) -> anyhow::Result<Self> {
        anyhow::ensure!(!node.is_empty(), "empty ownership node");
        anyhow::ensure!(
            ttl >= Duration::from_millis(100),
            "ownership lease too short"
        );
        let milliseconds = i64::try_from(ttl.as_millis())
            .map_err(|_| anyhow::anyhow!("ownership lease duration overflow"))?;
        let ttl_versions = milliseconds
            .checked_mul(VERSIONS_PER_MS)
            .ok_or_else(|| anyhow::anyhow!("ownership lease version overflow"))?;
        Ok(Self {
            ttl_versions,
            interval: ttl / 3,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_lease_configuration_is_an_error_before_any_store_access() {
        assert!(Timing::new("", Duration::from_secs(15)).is_err());
        assert!(Timing::new("node", Duration::from_millis(99)).is_err());
        assert!(Timing::new("node", Duration::MAX).is_err());
        // Milliseconds fit in i64 but conversion to FDB versions does not.
        assert!(Timing::new("node", Duration::from_secs(10_000_000_000_000)).is_err());
        let timing = Timing::new("node", Duration::from_secs(15)).unwrap();
        assert_eq!(timing.ttl_versions, 15_000_000);
        assert_eq!(timing.interval, Duration::from_secs(5));
    }

    #[test]
    fn empty_partition_or_live_set_never_invents_an_owner() {
        assert_eq!(super::super::partition_of_n("space", 0), None);
        assert_eq!(super::super::owner_of_partition(0, &[]), None);
        assert!(super::super::owned_partitions("node", &[]).is_err());
        assert!(super::super::owned_partitions("node", &["other".into()]).is_err());
        assert!(super::super::owned_partitions("node", &["node".into(), "".into()]).is_err());
        let partitions = super::super::owned_partitions("node", &["node".into()]).unwrap();
        assert_eq!(
            partitions,
            (0..super::super::PARTITION_COUNT).collect::<Vec<_>>()
        );
    }
}

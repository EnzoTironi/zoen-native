//! Consumptive claims use the authenticated relay clock, never event sequencing.
use super::KeyPackageClaimClock;
impl KeyPackageClaimClock {
    /// Encode an operation at this exact relay sample, independently of event
    /// ULID sequencing. Callers supply 80 fresh cryptographically random bits.
    #[must_use]
    pub fn operation_with_entropy(self, entropy: [u8; 10]) -> String {
        let mut random = [0u8; 16];
        random[6..].copy_from_slice(&entropy);
        let mut value = (u128::from(self.0.unsigned_abs()) << 80) | u128::from_be_bytes(random);
        let alphabet = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
        let mut encoded = [0u8; 26];
        for c in encoded.iter_mut().rev() {
            *c = alphabet[(value & 31) as usize];
            value >>= 5;
        }
        encoded.into_iter().map(char::from).collect()
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn claim_operations_do_not_inherit_a_later_event_clock(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let event = types::new_ulid(2)?;
        let sample = KeyPackageClaimClock::from_server_ms(1).ok_or("valid sample")?;
        let operation = sample.operation_with_entropy([7; 10]);
        assert!(types::id_time_ms(&event).ok_or("event timestamp")? > 1);
        assert_eq!(crate::key_package_claim_time_ms(&operation), Some(1));
        assert_eq!(sample.operation_with_entropy([7; 10]), operation);
        assert_ne!(sample.operation_with_entropy([8; 10]), operation);
        let maximum =
            KeyPackageClaimClock::from_server_ms(0xFFFF_FFFF_FFFF).ok_or("maximum sample")?;
        assert_eq!(
            crate::key_package_claim_time_ms(&maximum.operation_with_entropy([255; 10])),
            Some(0xFFFF_FFFF_FFFF)
        );
        assert!(KeyPackageClaimClock::from_server_ms(0).is_none());
        assert!(KeyPackageClaimClock::from_server_ms(0x1_0000_0000_0000).is_none());
        Ok(())
    }
}

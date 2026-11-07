//! Exact existing tool policy fingerprint, shared by local and durable hosts.
use super::*;
impl Gate {
    /// Binds thresholds, provider identities and the closed tool question set.
    /// # Errors
    /// Refuses non-numeric thresholds or an unencodable policy.
    pub fn tool_policy_sha256(&self) -> Result<String, GateError> {
        let invalid = || GateError::BadResponse("policy encoding".into());
        let thresholds = serde_json::to_value(&self.thresholds).map_err(|_| invalid())?;
        if thresholds
            .as_object()
            .is_none_or(|fields| fields.values().any(|v| !v.is_number()))
        {
            return Err(GateError::BadResponse("invalid policy thresholds".into()));
        }
        let bytes = serde_json::to_vec(&(
            "zoen.tool-policy.v1",
            thresholds,
            self.remote.as_ref().map(|p| p.name()),
            self.on_device.as_ref().map(|p| p.name()),
            DecisionProvider::name(&self.fallback),
            Gate::questions(Point::ToolCall),
        ))
        .map_err(|_| invalid())?;
        Ok(format!("{:x}", Sha256::digest(bytes)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn fingerprint_preserves_existing_local_host_contract() -> Result<(), Box<dyn std::error::Error>>
    {
        let gate = Gate::default();
        let thresholds = serde_json::to_value(&gate.thresholds)?;
        let old = serde_json::to_vec(&(
            "zoen.tool-policy.v1",
            thresholds,
            gate.remote.as_ref().map(|provider| provider.name()),
            gate.on_device.as_ref().map(|provider| provider.name()),
            DecisionProvider::name(&gate.fallback),
            Gate::questions(Point::ToolCall),
        ))?;
        assert_eq!(
            gate.tool_policy_sha256().map_err(|_| "fingerprint")?,
            format!("{:x}", Sha256::digest(old))
        );
        let first = gate.tool_policy_sha256().map_err(|_| "fingerprint")?;
        let mut changed = Gate::default();
        changed.thresholds.min_confidence = 0.98;
        assert_ne!(
            first,
            changed.tool_policy_sha256().map_err(|_| "fingerprint")?
        );
        changed.thresholds.min_confidence = f64::NAN;
        assert!(changed.tool_policy_sha256().is_err());
        Ok(())
    }
}

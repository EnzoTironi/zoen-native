//! Provider transport only. No session RPC or generic event route invokes this.
//! Receiver consent/block policy and authenticated call-purpose admission must
//! be implemented before exposing a wake operation or advertising cold calls.
use super::*;

impl Pusher {
    /// Requires a previously admitted wake. This transport cannot prove that an
    /// encrypted log event is a call or authorize media/acceptance on its own.
    pub async fn call_wake(&self, target: &Target, wake: &protocol::push::CallWake) -> Outcome {
        if !matches!(target.platform.as_str(), APNS_VOIP | APNS_VOIP_SANDBOX)
            || !valid_registration(&target.platform, &target.token)
            || target.token.is_empty()
        {
            return Outcome::Failed("invalid VoIP target".into());
        }
        let Some(apns) = &self.apns else {
            return Outcome::Disabled;
        };
        let now_ms = now().saturating_mul(1000).min(i64::MAX as u64) as i64;
        let Ok(body) = wake.encode(now_ms) else {
            return Outcome::Failed("expired or invalid VoIP wake".into());
        };
        let topic = format!("{}.voip", apns.cfg.topic);
        if protocol::push::validate_topic(&topic).is_err() {
            return Outcome::Failed("invalid VoIP topic".into());
        }
        let result = async {
            let host = if target.platform == APNS_VOIP_SANDBOX {
                "api.sandbox.push.apple.com"
            } else {
                "api.push.apple.com"
            };
            let (status, body) = self
                .http
                .post(
                    &format!("https://{host}/3/device/{}", target.token),
                    &[
                        ("authorization", format!("bearer {}", Self::apns_jwt(apns)?)),
                        ("apns-topic", topic),
                        ("apns-push-type", "voip".into()),
                        ("apns-priority", "10".into()),
                        ("apns-expiration", (wake.expires_at_ms() / 1000).to_string()),
                        ("content-type", "application/json".into()),
                    ],
                    body,
                )
                .await?;
            Ok::<_, anyhow::Error>(match status {
                200 => Outcome::Sent,
                410 => Outcome::Gone,
                400 if body.contains("BadDeviceToken") => Outcome::Gone,
                status => Outcome::Failed(format!("apns voip {status}")),
            })
        }
        .await;
        result.unwrap_or_else(|_| Outcome::Failed("VoIP provider unavailable".into()))
    }
}

#[cfg(test)]
mod tests;

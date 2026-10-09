//! The MLS credential of a Zoen device: who the leaf is, provable by any member.

use roda_log::{device_cert_message, verify_sig};

const TAG: &str = "zoen-leaf/1";

/// One device of one identity, as named in its MLS credential.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Leaf {
    pub identity: String,
    pub device: String,
    /// The identity's signature over `device_cert_message(device)`.
    pub cert: String,
}

impl Leaf {
    /// The credential bytes: `zoen-leaf/1:<identity>:<device>:<cert>`, all hex.
    pub fn encode(&self) -> Vec<u8> {
        format!("{TAG}:{}:{}:{}", self.identity, self.device, self.cert).into_bytes()
    }

    /// Parses a credential and checks it. The certificate must verify under the identity,
    /// and the leaf's MLS signature key must be the certified device key. Otherwise anyone
    /// could claim to be anyone's device.
    pub fn verified(credential: &[u8], signature_key: &[u8]) -> Option<Self> {
        let s = std::str::from_utf8(credential).ok()?;
        let mut parts = s.split(':');
        let (tag, identity, device, cert) =
            (parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        if tag != TAG || parts.next().is_some() {
            return None;
        }
        if hex::encode(signature_key) != device {
            return None;
        }
        if !verify_sig(identity, &device_cert_message(device), cert) {
            return None;
        }
        Some(Self {
            identity: identity.to_string(),
            device: device.to_string(),
            cert: cert.to_string(),
        })
    }
}

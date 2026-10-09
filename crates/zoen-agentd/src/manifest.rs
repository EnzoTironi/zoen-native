//! Tool capability manifests: what a tool needs, declared by its publisher and signed.
//!
//! The manifest is the only thing that can move a tool off the cheapest tier. A model's
//! request at runtime can't (see [`crate::router`]).

use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
pub use zoen_egress::{EgressRule, SecretBinding};

/// Hard ceilings for any manifest (raised per plan later, never by a manifest alone).
pub const MAX_VCPU: u8 = 8;
pub const MAX_MEM_MIB: u32 = 8192;
pub const MAX_DISK_MIB: u32 = 20 * 1024;
pub const MAX_SECS: u32 = 3600;

/// What a tool needs to run.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Needs {
    /// Runs in the agent runtime itself (prompting, formatting, calling Zoen's own APIs).
    None,
    /// A WASM component with host functions (hooks, parsers, transforms).
    Wasm,
    /// A real Linux in a microVM.
    MicroVm {
        /// In pairs (2, 4, 6, 8): with core scheduling a VM fills whole cores (ADR 0028 §6b).
        vcpu: u8,
        mem_mib: u32,
        disk_mib: u32,
        max_secs: u32,
    },
    /// A browser in a microVM.
    Browser { mem_mib: u32, max_secs: u32 },
}

/// A tool's declaration. `needs` defaults to `None`: undeclared means the cheapest tier.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ToolManifest {
    pub id: String,
    pub version: String,
    /// Shown on approval cards, in plain words.
    pub summary: String,
    #[serde(default = "needs_none")]
    pub needs: Needs,
    #[serde(default)]
    pub egress: Vec<EgressRule>,
    #[serde(default)]
    pub secrets: Vec<SecretBinding>,
    /// For `needs: wasm`: the sha256 of the component the publisher signed. The WASM tier
    /// refuses any other bytes, so nobody can swap a tool's code under its manifest.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component_sha256: Option<String>,
}

fn needs_none() -> Needs {
    Needs::None
}

#[derive(Debug, Clone, thiserror::Error, PartialEq, Eq)]
pub enum ManifestError {
    #[error("manifest is not signed by a trusted publisher")]
    UntrustedPublisher,
    #[error("manifest signature does not verify")]
    BadSignature,
    #[error("manifest is invalid: {0}")]
    Invalid(String),
}

impl ToolManifest {
    pub fn validate(&self) -> Result<(), ManifestError> {
        let bad = |m: String| Err(ManifestError::Invalid(m));
        if self.id.is_empty()
            || !self
                .id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        {
            return bad(format!("tool id `{}`", self.id));
        }
        match &self.needs {
            Needs::MicroVm {
                vcpu,
                mem_mib,
                disk_mib,
                max_secs,
            } => {
                if *vcpu == 0 || vcpu % 2 != 0 || *vcpu > MAX_VCPU {
                    return bad(format!("vcpu {vcpu}: must be 2, 4, 6 or 8"));
                }
                if *mem_mib < 128 || *mem_mib > MAX_MEM_MIB {
                    return bad(format!("mem_mib {mem_mib}"));
                }
                if *disk_mib > MAX_DISK_MIB || *max_secs == 0 || *max_secs > MAX_SECS {
                    return bad("disk or time over the ceiling".into());
                }
            }
            Needs::Browser { mem_mib, max_secs } => {
                if *mem_mib < 512
                    || *mem_mib > MAX_MEM_MIB
                    || *max_secs == 0
                    || *max_secs > MAX_SECS
                {
                    return bad("browser memory or time out of range".into());
                }
            }
            Needs::None | Needs::Wasm => {}
        }
        for r in &self.egress {
            r.validate().map_err(ManifestError::Invalid)?;
        }
        for s in &self.secrets {
            if s.hosts.is_empty() {
                return bad(format!("secret `{}` is bound to no host", s.name));
            }
            if !s.hosts.iter().all(|h| {
                self.egress
                    .iter()
                    .any(|r| zoen_egress::host_matches(&r.host, h) || r.host == *h)
            }) {
                return bad(format!(
                    "secret `{}` is bound to a host the tool can't reach",
                    s.name
                ));
            }
        }
        Ok(())
    }

    fn signing_bytes(&self) -> Vec<u8> {
        let mut b = b"zoen-tool-manifest/1\n".to_vec();
        b.extend(serde_json::to_vec(self).expect("manifest serializes"));
        b
    }

    pub fn sign(self, publisher: &SigningKey) -> SignedManifest {
        let sig = publisher.sign(&self.signing_bytes());
        SignedManifest {
            publisher: hex::encode(publisher.verifying_key().to_bytes()),
            signature: hex::encode(sig.to_bytes()),
            manifest: self,
        }
    }
}

/// A manifest as shipped: the publisher's key and signature over the canonical bytes.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct SignedManifest {
    pub manifest: ToolManifest,
    pub publisher: String,
    pub signature: String,
}

impl SignedManifest {
    /// Checks the publisher is trusted, the signature verifies and the content is valid.
    pub fn verify(&self, trusted: &[VerifyingKey]) -> Result<&ToolManifest, ManifestError> {
        let key = trusted
            .iter()
            .find(|k| hex::encode(k.to_bytes()) == self.publisher)
            .ok_or(ManifestError::UntrustedPublisher)?;
        let sig: [u8; 64] = hex::decode(&self.signature)
            .ok()
            .and_then(|b| b.try_into().ok())
            .ok_or(ManifestError::BadSignature)?;
        key.verify(&self.manifest.signing_bytes(), &Signature::from_bytes(&sig))
            .map_err(|_| ManifestError::BadSignature)?;
        self.manifest.validate()?;
        Ok(&self.manifest)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(b: u8) -> SigningKey {
        SigningKey::from_bytes(&[b; 32])
    }

    fn code_tool() -> ToolManifest {
        ToolManifest {
            id: "python".into(),
            version: "1".into(),
            summary: "Roda código Python".into(),
            needs: Needs::MicroVm {
                vcpu: 2,
                mem_mib: 1024,
                disk_mib: 2048,
                max_secs: 300,
            },
            egress: vec![EgressRule::host("*.pypi.org")],
            secrets: vec![],
            component_sha256: None,
        }
    }

    #[test]
    fn a_tampered_manifest_does_not_verify() {
        let k = key(7);
        let mut s = code_tool().sign(&k);
        assert!(s.verify(&[k.verifying_key()]).is_ok());
        assert_eq!(
            s.verify(&[key(8).verifying_key()]),
            Err(ManifestError::UntrustedPublisher)
        );
        s.manifest.needs = Needs::MicroVm {
            vcpu: 8,
            mem_mib: 8192,
            disk_mib: 2048,
            max_secs: 3600,
        };
        assert_eq!(
            s.verify(&[k.verifying_key()]),
            Err(ManifestError::BadSignature)
        );
    }

    #[test]
    fn odd_vcpus_and_unbound_secrets_are_invalid() {
        let mut m = code_tool();
        m.needs = Needs::MicroVm {
            vcpu: 1,
            mem_mib: 1024,
            disk_mib: 1,
            max_secs: 10,
        };
        assert!(m.validate().is_err());
        let mut m = code_tool();
        m.secrets = vec![SecretBinding {
            name: "gh".into(),
            hosts: vec!["api.github.com".into()],
        }];
        assert!(
            m.validate().is_err(),
            "secret bound to a host outside the allowlist"
        );
    }

    #[test]
    fn undeclared_needs_mean_the_cheapest_tier() {
        let m: ToolManifest =
            serde_json::from_str(r#"{"id":"summarize","version":"1","summary":"x"}"#).unwrap();
        assert_eq!(m.needs, Needs::None);
    }
}

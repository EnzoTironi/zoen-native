//! Device-bound encrypted-profile policy. Hosts persist only ciphertext and
//! signatures in the authoritative Directory transaction domain.
use super::{internal, no, reads, Session};
use crate::portable::BoundaryError;
use base64::{engine::general_purpose::STANDARD, Engine};
use roda_log::profile::{DeviceSigned, MAX_CIPHERTEXT};
use roda_proto::SealedProfile;
use roda_proto::{AgreementKeyRecord, Op, Reply};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Signed {
    pub device: String,
    pub sig: String,
    pub cert: String,
}
impl From<&DeviceSigned> for Signed {
    fn from(value: &DeviceSigned) -> Self {
        Self {
            device: value.device.clone(),
            sig: value.sig.clone(),
            cert: value.cert.clone(),
        }
    }
}
impl From<&Signed> for DeviceSigned {
    fn from(value: &Signed) -> Self {
        Self {
            device: value.device.clone(),
            sig: value.sig.clone(),
            cert: value.cert.clone(),
        }
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Agreement {
    pub identity: String,
    pub public: String,
    pub signed: Signed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub identity: String,
    // Decimal text preserves every accepted BIGINT across the JS boundary.
    pub version: String,
    pub ciphertext_base64: String,
    pub signed: Signed,
}
impl From<&SealedProfile> for Profile {
    fn from(value: &SealedProfile) -> Self {
        Self {
            identity: value.identity.clone(),
            version: value.version.to_string(),
            ciphertext_base64: STANDARD.encode(&value.ciphertext),
            signed: (&value.signed).into(),
        }
    }
}
impl Profile {
    fn decode(&self) -> Result<SealedProfile, BoundaryError> {
        let version: u64 = self
            .version
            .parse()
            .map_err(|_| internal("invalid sealed profile version"))?;
        if version > i64::MAX as u64 || version.to_string() != self.version {
            return Err(internal("invalid sealed profile version"));
        }
        if self.ciphertext_base64.len() > MAX_CIPHERTEXT.div_ceil(3) * 4 {
            return Err(internal("invalid sealed profile ciphertext"));
        }
        let ciphertext = STANDARD
            .decode(&self.ciphertext_base64)
            .map_err(|_| internal("invalid sealed profile ciphertext"))?;
        if ciphertext.len() > MAX_CIPHERTEXT
            || STANDARD.encode(&ciphertext) != self.ciphertext_base64
        {
            return Err(internal("invalid sealed profile ciphertext"));
        }
        Ok(SealedProfile {
            identity: self.identity.clone(),
            version,
            ciphertext,
            signed: (&self.signed).into(),
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Query {
    PublishAgreement,
    AgreementKeys { ids: Vec<String> },
    PutProfile { identity: String },
    GetProfiles { ids: Vec<String> },
}
pub fn query(op: &Op) -> Option<Query> {
    Some(match op {
        Op::PublishAgreementKey { .. } => Query::PublishAgreement,
        Op::AgreementKeys { ids } => Query::AgreementKeys {
            ids: reads::profile_ids(ids.clone()),
        },
        Op::PutProfile { profile } => Query::PutProfile {
            identity: profile.identity.clone(),
        },
        Op::GetProfiles { ids } => Query::GetProfiles {
            ids: reads::profile_ids(ids.clone()),
        },
        _ => return None,
    })
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Facts {
    pub query: Query,
    pub agreements: Vec<Agreement>,
    pub profiles: Vec<Profile>,
}

pub fn plan(op: &Op, session: &Session, facts: &Facts) -> Result<(Value, Reply), BoundaryError> {
    let expected = query(op).ok_or_else(|| internal("invalid encrypted directory operation"))?;
    if expected != facts.query {
        return Err(internal("encrypted directory rows belong to another query"));
    }
    match op {
        Op::PublishAgreementKey { public, signed } => {
            if !facts.agreements.is_empty() || !facts.profiles.is_empty() {
                return Err(internal("unexpected agreement publication rows"));
            }
            validate_agreement(&session.identity, &session.device, public, signed).map_err(no)?;
            Ok((
                json!({"action":"publish_agreement","agreement":Agreement {
                identity: session.identity.clone(), public: public.clone(), signed: signed.into() }}),
                Reply::Done,
            ))
        }
        Op::PutProfile { profile } => {
            validate_profile(&session.identity, &session.device, profile).map_err(no)?;
            if !facts.agreements.is_empty()
                || facts.profiles.len() > 1
                || facts
                    .profiles
                    .iter()
                    .any(|row| row.identity != session.identity)
            {
                return Err(internal(
                    "stored encrypted profile belongs to another identity",
                ));
            }
            let stored = facts.profiles.first().map(Profile::decode).transpose()?;
            if !accepts_profile(stored.as_ref(), profile) {
                return Err(no("stale profile version"));
            }
            Ok((
                json!({"action":"put_profile","profile":Profile::from(profile)}),
                Reply::Done,
            ))
        }
        Op::AgreementKeys { .. } => {
            let Query::AgreementKeys { ids } = expected else {
                unreachable!()
            };
            if !facts.profiles.is_empty() {
                return Err(internal("unexpected encrypted profile rows"));
            }
            check_rows(
                &ids,
                facts.agreements.iter().map(|row| row.identity.as_str()),
            )?;
            Ok((
                json!({"action":"read"}),
                Reply::AgreementKeys(
                    facts
                        .agreements
                        .iter()
                        .map(|row| AgreementKeyRecord {
                            identity: row.identity.clone(),
                            public: row.public.clone(),
                            signed: (&row.signed).into(),
                        })
                        .collect(),
                ),
            ))
        }
        Op::GetProfiles { .. } => {
            let Query::GetProfiles { ids } = expected else {
                unreachable!()
            };
            if !facts.agreements.is_empty() {
                return Err(internal("unexpected agreement rows"));
            }
            check_rows(&ids, facts.profiles.iter().map(|row| row.identity.as_str()))?;
            Ok((
                json!({"action":"read"}),
                Reply::SealedProfiles(
                    facts
                        .profiles
                        .iter()
                        .map(Profile::decode)
                        .collect::<Result<_, _>>()?,
                ),
            ))
        }
        _ => Err(internal("invalid encrypted directory operation")),
    }
}
fn check_rows<'a>(
    ids: &[String],
    rows: impl Iterator<Item = &'a str>,
) -> Result<(), BoundaryError> {
    let wanted: BTreeSet<_> = ids.iter().map(String::as_str).collect();
    let mut selected = BTreeSet::new();
    for id in rows {
        if !wanted.contains(id) || !selected.insert(id) {
            return Err(internal(
                "encrypted directory rows do not match the requested identities",
            ));
        }
    }
    Ok(())
}

pub fn reply_json(bytes: &[u8]) -> Result<String, BoundaryError> {
    let frame = roda_proto::ServerFrame::decode(bytes)
        .map_err(|_| internal("invalid encrypted directory reply"))?;
    if frame.encode() != bytes {
        return Err(internal("encrypted directory reply is not canonical"));
    }
    let roda_proto::ServerFrame::Res { id, result } = frame else {
        return Err(internal("expected directory response"));
    };
    super::encoded(&match result {
        Ok(Reply::AgreementKeys(rows)) => {
            json!({"id":id.to_string(),"kind":"agreements","rows":rows.iter().map(|row|
            Agreement { identity: row.identity.clone(), public: row.public.clone(), signed: (&row.signed).into() }).collect::<Vec<_>>()})
        }
        Ok(Reply::SealedProfiles(rows)) => {
            json!({"id":id.to_string(),"kind":"sealed_profiles","rows":rows.iter().map(Profile::from).collect::<Vec<_>>()})
        }
        Err(reason) => json!({"id":id.to_string(),"kind":"rejected","reason":reason}),
        _ => return Err(internal("unsupported encrypted directory reply")),
    })
}

pub fn validate_agreement(
    identity: &str,
    device: &str,
    public: &str,
    signed: &DeviceSigned,
) -> Result<(), String> {
    if signed.device != device {
        return Err("agreement key is for another device".into());
    }
    if !roda_log::profile::verify_agreement(identity, public, signed) {
        return Err("agreement key isn't signed by this identity".into());
    }
    Ok(())
}

pub fn validate_profile(
    identity: &str,
    device: &str,
    profile: &SealedProfile,
) -> Result<(), String> {
    if profile.identity != identity {
        return Err("profile is for another identity".into());
    }
    if profile.signed.device != device {
        return Err("profile is for another device".into());
    }
    if profile.ciphertext.len() > MAX_CIPHERTEXT {
        return Err("profile too large".into());
    }
    // Native PostgreSQL stores BIGINT. A signed UInt64 must never wrap negative
    // and invert monotonic replacement when a portable host uses decimal text.
    if profile.version > i64::MAX as u64 {
        return Err("profile version out of range".into());
    }
    if !roda_log::profile::verify_upload(
        &profile.identity,
        profile.version,
        &profile.ciphertext,
        &profile.signed,
    ) {
        return Err("profile isn't signed by this identity".into());
    }
    Ok(())
}

/// Identical ciphertext at an equal version is a resumable write. A newer
/// device signature may replace the old signature without changing the bytes.
pub fn accepts_profile(stored: Option<&SealedProfile>, incoming: &SealedProfile) -> bool {
    stored.is_none_or(|previous| {
        previous.version < incoming.version
            || previous.version == incoming.version && previous.ciphertext == incoming.ciphertext
    })
}

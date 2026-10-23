use super::*;

pub(super) fn signed_to(s: &DeviceSigned) -> Option<PbDeviceSigned> {
    Some(PbDeviceSigned {
        device: s.device.clone(),
        sig: s.sig.clone(),
        cert: s.cert.clone(),
    })
}

pub(super) fn signed_from(s: Option<PbDeviceSigned>) -> Result<DeviceSigned, DecodeError> {
    let s = s.ok_or_else(|| bad("unsigned statement"))?;
    Ok(DeviceSigned {
        device: s.device,
        sig: s.sig,
        cert: s.cert,
    })
}

pub(super) fn agreement_to(k: &AgreementKeyRecord) -> PbAgreementKey {
    PbAgreementKey {
        identity: k.identity.clone(),
        public: k.public.clone(),
        signed: signed_to(&k.signed),
    }
}

pub(super) fn agreement_from(k: PbAgreementKey) -> Result<AgreementKeyRecord, DecodeError> {
    Ok(AgreementKeyRecord {
        identity: k.identity,
        public: k.public,
        signed: signed_from(k.signed)?,
    })
}

pub(super) fn sealed_profile_to(p: &SealedProfile) -> PbSealedProfile {
    PbSealedProfile {
        identity: p.identity.clone(),
        version: p.version,
        ciphertext: p.ciphertext.clone(),
        signed: signed_to(&p.signed),
    }
}

pub(super) fn sealed_profile_from(p: PbSealedProfile) -> Result<SealedProfile, DecodeError> {
    Ok(SealedProfile {
        identity: p.identity,
        version: p.version,
        ciphertext: p.ciphertext,
        signed: signed_from(p.signed)?,
    })
}

pub(super) fn role_to(r: Role) -> i32 {
    (match r {
        Role::Owner => PbRole::Owner,
        Role::Admin => PbRole::Admin,
        Role::Member => PbRole::Member,
        Role::Reader => PbRole::Reader,
    }) as i32
}

pub(super) fn role_from(r: i32) -> Result<Role, DecodeError> {
    match PbRole::try_from(r).map_err(|_| bad("role"))? {
        PbRole::Owner => Ok(Role::Owner),
        PbRole::Admin => Ok(Role::Admin),
        PbRole::Member => Ok(Role::Member),
        PbRole::Reader => Ok(Role::Reader),
    }
}

pub(super) fn profile_to(p: &Identity) -> PbProfile {
    PbProfile {
        id: p.id.clone(),
        kind: match p.kind {
            IdentityKind::Person => PbIdentityKind::Person,
            IdentityKind::Agent => PbIdentityKind::Agent,
        } as i32,
        name: p.name.clone(),
        handle: p.handle.clone(),
        tint_hex: p.tint_hex.clone(),
        glyph: p.glyph.clone(),
        owner: p.owner.clone(),
        bio: p.bio.clone(),
        owner_proof: p.owner_proof.as_ref().map(|proof| PbAgentOwnerProof {
            device: proof.device.clone(),
            cert: proof.cert.clone(),
            signature: proof.signature.clone(),
        }),
    }
}

pub(super) fn profile_from(p: PbProfile) -> Result<Identity, DecodeError> {
    Ok(Identity {
        id: p.id,
        kind: match PbIdentityKind::try_from(p.kind).map_err(|_| bad("identity kind"))? {
            PbIdentityKind::Person => IdentityKind::Person,
            PbIdentityKind::Agent => IdentityKind::Agent,
        },
        name: p.name,
        handle: p.handle,
        tint_hex: p.tint_hex,
        glyph: p.glyph,
        owner: p.owner,
        bio: p.bio,
        owner_proof: p.owner_proof.map(|proof| {
            Box::new(types::AgentOwnerProof {
                device: proof.device,
                cert: proof.cert,
                signature: proof.signature,
            })
        }),
    })
}

pub(super) fn envelope_to(e: &Envelope) -> PbEnvelope {
    PbEnvelope {
        content: e.content().to_vec(),
        sig: e.sig.clone(),
        cert: e.cert.clone(),
        invite: e.invite.clone(),
        pruned: e.legacy_pruned_hash().map(str::to_string),
    }
}

pub(super) fn envelope_from(e: PbEnvelope) -> Result<Envelope, DecodeError> {
    Envelope::new(e.content, e.sig, e.cert, e.invite)
        .and_then(|env| env.with_legacy_pruned(e.pruned))
        .ok_or_else(|| bad("envelope content"))
}

pub(super) fn ephemeral_to(space: &str, from: &str, k: &EphemeralKind) -> PbEphemeral {
    use pb_ephemeral::Kind;
    PbEphemeral {
        space: space.to_string(),
        from: from.to_string(),
        kind: Some(match k {
            EphemeralKind::Typing => Kind::Typing(PbEmpty {}),
            EphemeralKind::StoppedTyping => Kind::StoppedTyping(PbEmpty {}),
            EphemeralKind::Status { status } => Kind::Status(status.clone()),
            EphemeralKind::Read { seq } => Kind::Read(*seq),
            EphemeralKind::Delivered { seq } => Kind::Delivered(*seq),
        }),
    }
}

pub(super) fn ephemeral_kind(k: Option<pb_ephemeral::Kind>) -> Result<EphemeralKind, DecodeError> {
    use pb_ephemeral::Kind;
    Ok(match k.ok_or_else(|| bad("ephemeral kind"))? {
        Kind::Typing(_) => EphemeralKind::Typing,
        Kind::StoppedTyping(_) => EphemeralKind::StoppedTyping,
        Kind::Status(status) => EphemeralKind::Status { status },
        Kind::Read(seq) => EphemeralKind::Read { seq },
        Kind::Delivered(seq) => EphemeralKind::Delivered { seq },
    })
}

impl Sequenced {
    /// The stored form of a log entry: the same bytes the wire carries.
    pub fn encode(&self) -> Vec<u8> {
        PbSequenced {
            seq: self.seq,
            prev: self.prev.clone(),
            hash: self.hash.clone(),
            env: Some(envelope_to(&self.env)),
        }
        .encode_to_vec()
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, DecodeError> {
        let s = PbSequenced::decode(bytes).map_err(|e| DecodeError(e.to_string()))?;
        Ok(Sequenced {
            seq: s.seq,
            prev: s.prev,
            hash: s.hash,
            env: envelope_from(s.env.ok_or_else(|| bad("entry without envelope"))?)?,
        })
    }
}

pub(super) fn agent_req_to(a: &AgentRequest) -> PbAgentReq {
    let mut pb = PbAgentReq::default();
    match a {
        AgentRequest::State => pb.op = "state".into(),
        AgentRequest::Advance { space, seq } => {
            pb.op = "advance".into();
            pb.space = space.clone();
            pb.seq = *seq;
        }
        AgentRequest::Define {
            agent,
            version,
            kind,
            definition,
        } => {
            pb.op = "define".into();
            pb.agent = agent.clone();
            pb.version = *version;
            pb.kind = kind.clone();
            pb.definition = definition.clone();
        }
        AgentRequest::Status { agent } => {
            pb.op = "status".into();
            pb.agent = agent.clone();
        }
        AgentRequest::Control { agent, action } => {
            pb.op = action.id().into();
            pb.agent = agent.clone();
        }
    }
    pb
}

pub(super) fn agent_req_from(pb: PbAgentReq) -> Result<AgentRequest, DecodeError> {
    Ok(match pb.op.as_str() {
        "state" => AgentRequest::State,
        "advance" => AgentRequest::Advance {
            space: pb.space,
            seq: pb.seq,
        },
        "define" => AgentRequest::Define {
            agent: pb.agent,
            version: pb.version,
            kind: pb.kind,
            definition: pb.definition,
        },
        "status" => AgentRequest::Status { agent: pb.agent },
        other => AgentRequest::Control {
            agent: pb.agent,
            action: AgentAction::parse(other).ok_or_else(|| bad("agent op"))?,
        },
    })
}

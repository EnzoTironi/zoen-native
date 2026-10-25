//! Exact closed EventBody variant set from main6a5096ac before AppCurrentShared.
use serde::{Deserialize, Serialize};
use types::*;
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum LegacyEventBody {
    SpaceCreated {
        title: String,
        kind: SpaceKind,
        privacy: Privacy,
    },
    MemberAdded {
        identity: IdentityId,
        role: Role,
    },
    MemberRemoved {
        identity: IdentityId,
    },
    MessagePosted {
        message: ItemId,
        text: String,
        attaches: Option<ItemId>,
        /// Signed per-message presentation and explicit album membership. Omitted for legacy sends.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        media: Option<MessageMedia>,
        /// Inline reply or thread reply (see [`types::reply`]). Absent on plain messages, so their
        /// bytes are exactly what older peers expect.
        #[serde(default, skip_serializing_if = "Option::is_none")]
        reply: Option<ReplyRef>,
        /// Forwarded from another chat (shown as "Forwarded"). Absent when false, so plain
        /// messages keep their exact bytes.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        forwarded: bool,
    },
    /// A scoped App request uses its own discriminator: older consumers retain
    /// these signed bytes as Unsupported, never as an unrestricted chat prompt.
    AppEditRequested {
        message: ItemId,
        text: String,
        reply: ReplyRef,
    },
    /// Closed v1 signaling; carried inside MLS for an encrypted person-to-person DM.
    CallV1 {
        signal: types::calls::CallSignal,
    },
    /// Authenticated SDP/ICE for the two accepted devices, never clear relay ephemera.
    CallTransportV1 {
        signal: types::call_transport::CallTransportSignal,
    },
    /// How I keep one of my chats (pinned, archived, muted). Lives only in my Personal
    /// Space, so it syncs across my own devices and nobody else ever sees it. The whole
    /// record is restated each time: the latest one wins.
    ChatPrefsSet {
        space: SpaceId,
        prefs: ChatPrefs,
    },
    /// An edit, delete for everyone or reaction on an earlier message (see [`types::annotation`]).
    MessageAnnotated {
        target: String,
        note: Annotation,
    },
    ItemCreated {
        item: ItemId,
        kind: ItemKind,
        content: ItemContent,
        origin: String,
    },
    ItemVersioned {
        item: ItemId,
        content: ItemContent,
        note: String,
    },
    /// Desfazer/restaurar nunca apaga: cria uma versão nova igual a uma anterior.
    ItemReverted {
        item: ItemId,
        to_version: u32,
        note: String,
    },
    GrantIssued {
        grant: Grant,
    },
    GrantRevoked {
        grant: GrantId,
    },
    RequestOpened {
        request: AgentRequest,
    },
    /// The owner asks for preparation of another review of one frozen action.
    /// This event grants nothing and never changes the previous decision.
    ToolReviewRequested {
        request: RequestId,
        content_hash: String,
    },
    RequestResolved {
        request: RequestId,
        approved: bool,
        content_hash: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        resolution: Option<RequestResolution>,
    },
    /// Explicit read-only discovery, bound to one canonical pending image review.
    OwnerPolicyOptionsRequested {
        agent: IdentityId,
        signed: types::owner_policy_context::SignedOwnerPolicyInquiry,
    },
    OwnerPolicyOptionsRefused {
        source: String,
        inquiry_digest: String,
    },
    OwnerPolicyOptionsReported {
        source: String,
        options: types::owner_policy_context::OwnerPolicyOptions,
    },
    /// Dedicated owner control, never detected from ordinary message text.
    OwnerPolicySubmitted {
        agent: IdentityId,
        signed: types::owner_budget::SignedOwnerPolicy,
    },
    /// Authenticated host result for the exact original policy, not a new budget.
    OwnerPolicyReported {
        source: String,
        policy_digest: String,
        outcome: OwnerPolicyOutcome,
    },
    /// Uso de IA de um agente (debitado do orçamento do dono).
    UsageRecorded {
        agent: IdentityId,
        cents: i64,
        what: String,
    },
    /// The chat's background changed (shared with everyone in the chat). Photos travel as a
    /// content-addressed attachment: the event carries only its hash and framing.
    BackgroundSet {
        background: BackgroundSpec,
    },
    /// The author's profile key, sealed separately to each recipient's agreement key
    /// (ADR 0016). `version` is the first profile version the key opens. Only the named
    /// recipient can open a share; from M2 on the whole event rides inside MLS.
    ProfileKeyShared {
        version: u64,
        shares: Vec<ProfileKeyShare>,
    },
    /// A relay-readable Space becomes end-to-end from this entry on (ADR 0027). There is
    /// no event for the other direction: a Space's privacy only ever goes up.
    SpaceEncrypted,
    /// A member's statement about an end-to-end Space (ADR 0026): having applied the log
    /// through `upto`, its MLS group is at `epoch`, and `digest` is SHA-256 over a tag, the
    /// group id, the epoch and the epoch authenticator. Only members of that exact group can
    /// compute it, so members who were shown different groups (a forked relay) disagree in
    /// public. The relay checks `upto` is on its chain and keeps the newest per member.
    Checkpoint {
        upto: Seen,
        epoch: u64,
        digest: String,
    },
    /// The author's `device` needs a fresh leaf in this end-to-end Space's group from here
    /// (ADR 0026, ADR 0045): a device just linked, or one back after the pruning ceiling let
    /// history it never fetched go. Whoever may commit for it adds it (taking an old leaf
    /// out first); the relay holds pruning for it from this entry until it checkpoints.
    DeviceJoining {
        device: String,
    },
    /// An entry of an end-to-end Space this device keeps as the relay ordered it: an MLS
    /// handshake (`Commit`, `Welcome`), or ciphertext from before it joined. Derived from
    /// the signed outer bytes, never signed as a body itself.
    Sealed {
        kind: String,
    },
    /// A kind this build doesn't know yet (a newer client wrote it). The signed bytes are
    /// kept verbatim, so the event still verifies, syncs and chains; it just isn't shown.
    Unsupported {
        kind: String,
    },
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum OwnerPolicyOutcome {
    Installed,
    Refused,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq, Eq)]
pub struct ChatPrefs {
    #[serde(default)]
    pub pinned: bool,
    #[serde(default)]
    pub archived: bool,
    /// Muted until this time (ms since epoch); `i64::MAX` = always. Absent = not muted.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted_until_ms: Option<i64>,
}

#[test]
fn older_closed_decoder_retains_share_whole_as_unsupported_without_separate_grant() -> super::R<()>
{
    // The complete genuine payload, with its real normal Grant. Exercise the
    // exact pre-share enum above and the existing closed decode fallback rule.
    let mut f = super::late_word(":memory:", ":memory:")?;
    let share = super::wire::template(&mut f)?;
    let current = EventBody::AppCurrentShared {
        share: Box::new(share),
    };
    let bytes = serde_json::to_vec(&current).unwrap();
    assert_eq!(event_log::content::decode_body(&bytes), current);
    assert!(serde_json::from_slice::<LegacyEventBody>(&bytes).is_err());
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let kind = value.as_object().unwrap().keys().next().unwrap().clone();
    assert_eq!(kind, "AppCurrentShared");
    let body = EventBody::Unsupported { kind };
    assert!(matches!(body, EventBody::Unsupported { .. }));
    assert!(!matches!(
        body,
        EventBody::GrantIssued { .. } | EventBody::MessagePosted { .. }
    ));
    let ordinary = serde_json::to_vec(&super::post("legacy remains readable")).unwrap();
    assert!(matches!(
        serde_json::from_slice::<LegacyEventBody>(&ordinary).unwrap(),
        LegacyEventBody::MessagePosted { .. }
    ));
    Ok(())
}

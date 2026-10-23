//! # types
//!
//! Os **cinco primitivos** do domínio e o substrato de eventos.
//!
//! | Primitivo   | Aqui                                   |
//! |-------------|----------------------------------------|
//! | Identidade  | [`Identity`] (pessoa, agente ou Espaço) |
//! | Espaço      | [`EventBody::SpaceCreated`] + projeção  |
//! | Membro      | [`EventBody::MemberAdded`] / [`Role`]   |
//! | Item        | [`ItemContent`] versionado por eventos  |
//! | Concessão   | [`Grant`] / [`Capability`]              |
//!
//! Tudo que acontece num Espaço vira um [`Event`] assinado no log daquele Espaço
//! (ver `event-log`). Nenhuma tela mostra o log; ele é a fonte da verdade.

pub mod call_transport;
pub mod calls;
pub mod media;
pub use media::{
    AttachmentGrouping, AttachmentPresentation, AttachmentSendOptions, MediaGroup,
    MediaMetadataError, MessageMedia,
};
pub mod annotation;
pub mod owner_budget;
pub use annotation::Annotation;
pub mod reply;
pub use reply::ReplyRef;

mod ids;
pub use ids::{
    id_time_ms, new_id, new_secret_id, new_ulid, GrantId, IdentityId, ItemId, RequestId, SpaceId,
};
mod identity;
pub use identity::{AgentOwnerProof, Identity, IdentityKind, Privacy, Role, SpaceKind};
mod items;
pub use items::{
    AppDoc, ChunkRef, FileDoc, ItemContent, ItemKind, PageContent, PageManifest, PlanDoc, PlanLine,
    PlanManifestV1, PlanSection, VoiceMeta,
};
mod authority;
pub use authority::{
    ActionClass, AgentRequest, Capability, ConsentMode, Grant, GrantScope, ItemProposal,
    RequestResolution, ToolDecision, ToolProposal, ToolReviewLineage, TrustLevel,
};
mod events;
pub use events::{
    BackgroundSpec, ChainLink, ChatPrefs, Event, EventBody, MediaRef, ProfileKeyShare, Seen,
    EVENT_FORMAT, GENESIS_PREV,
};

#[cfg(test)]
mod tests;

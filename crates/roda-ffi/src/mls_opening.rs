use base64::{engine::general_purpose::STANDARD, Engine as _};
use roda_mls::Leaf;
use roda_proto::{Envelope, Sequenced};
use roda_store::Store;
use roda_types::Event;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub(crate) enum OpeningOutcome {
    MlsOpened {
        identity: String,
        device: String,
        cert: String,
        inner_digest: String,
    },
    OwnRetainedEcho,
    Opaque,
}

impl OpeningOutcome {
    pub(crate) fn from_application(event: &Event, from: &Leaf) -> Self {
        Self::MlsOpened {
            identity: from.identity.clone(),
            device: from.device.clone(),
            cert: from.cert.clone(),
            inner_digest: roda_log::content_hash_of(event),
        }
    }
}

#[derive(Serialize, Deserialize)]
struct StoredOpening {
    version: u8,
    seq: u64,
    prev: String,
    hash: String,
    content: String,
    sig: String,
    cert: Option<String>,
    outcome: OpeningOutcome,
}

fn key(hash: &str) -> String {
    format!("runtime.mls-opening.v1/{hash}")
}

/// Called only from actual MLS ingestion, inside its log/provider transaction.
/// A retained own outbox echo is deliberately not an authenticated peer opening.
pub(crate) fn record(
    store: &Store,
    sequenced: &Sequenced,
    outcome: OpeningOutcome,
) -> Result<(), roda_store::StoreError> {
    let retained = StoredOpening {
        version: 1,
        seq: sequenced.seq,
        prev: sequenced.prev.clone(),
        hash: sequenced.hash.clone(),
        content: STANDARD.encode(sequenced.env.content()),
        sig: sequenced.env.sig.clone(),
        cert: sequenced.env.cert.clone(),
        outcome,
    };
    store.set_meta(&key(&sequenced.hash), &serde_json::to_string(&retained)?)
}

/// The private custody loader authenticates the complete image before the
/// facade uses this historical receipt. Arbitrary SQLite bytes are not proof.
pub(crate) fn opened_leaf(store: &Store, event: &Event) -> Option<Leaf> {
    let retained = retained_application(store, event)?;
    let OpeningOutcome::MlsOpened {
        identity,
        device,
        cert,
        inner_digest,
    } = retained.outcome
    else {
        return None;
    };
    if inner_digest != roda_log::content_hash_of(event)
        || identity != event.author
        || event.device.as_deref() != Some(device.as_str())
        || event.cert.as_deref() != Some(cert.as_str())
    {
        return None;
    }
    let from = Leaf {
        identity,
        device,
        cert,
    };
    let signature_key = hex::decode(&from.device).ok()?;
    Leaf::verified(&from.encode(), &signature_key)
}

pub(crate) fn own_echo(store: &Store, event: &Event, account: &crate::sync::AccountMeta) -> bool {
    event.author == account.identity
        && event.device.as_deref() == Some(account.device.as_str())
        && event.cert.as_deref() == Some(account.cert.as_str())
        && retained_application(store, event)
            .is_some_and(|record| matches!(record.outcome, OpeningOutcome::OwnRetainedEcho))
}

fn retained_application(store: &Store, event: &Event) -> Option<StoredOpening> {
    let json = store.meta(&key(&event.hash)).ok()??;
    if json.len() > 131072 {
        return None;
    }
    let retained: StoredOpening = serde_json::from_str(&json).ok()?;
    if retained.version != 1
        || retained.seq != event.seq
        || retained.prev != event.prev
        || retained.hash != event.hash
    {
        return None;
    }
    let content = STANDARD.decode(&retained.content).ok()?;
    if content.len() > 65536 {
        return None;
    }
    let envelope = Envelope::new(content, retained.sig.clone(), retained.cert.clone(), None)?;
    envelope.verify().ok()?;
    let wire_hash = envelope.wire_hash();
    if event.sealed_wire.as_deref() != Some(wire_hash.as_str())
        || envelope.space() != event.space
        || envelope.author() != event.author
        || envelope.device() != event.device.as_deref()
        || envelope.cert != event.cert
        || envelope.client_id() != event.client_id
        || envelope.at_ms() != event.at_ms
        || envelope.seen() != event.seen
        || envelope.sealed_kind() != Some(roda_log::content::SealedKind::Application)
        || roda_log::chain_hash(&event.space, event.seq, &event.prev, &wire_hash) != event.hash
    {
        return None;
    }
    Some(retained)
}

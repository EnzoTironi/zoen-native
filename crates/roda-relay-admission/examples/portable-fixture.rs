//! Emits public deterministic fixtures for native/WASM equivalence smoke tests.
use base64::{engine::general_purpose::STANDARD, Engine};
use roda_log::{Author, Signer};
use roda_proto::{ClientFrame, Envelope};
use roda_relay_admission::portable::{
    admit_publish_json, FactSnapshot, ObservedHash, SessionContext, TrustedContext,
};
use roda_types::{EventBody, Privacy, Role, Seen, SpaceKind};

fn fixture(
    author: &Author,
    context: TrustedContext,
    body: EventBody,
    seen: Option<Seen>,
) -> serde_json::Value {
    let env = Envelope::plain(&author.sign_event(
        "sp_cloudflare_admission_fixture",
        "01ARZ3NDEKTSV4RRFFQ69G5FAV",
        1000,
        seen,
        body,
    ));
    let frame = ClientFrame::Publish { env: env.clone() }.encode();
    let mut invalid = env;
    invalid.sig = "00".repeat(64);
    let session = SessionContext {
        authenticated_identity: context.authenticated_identity.clone(),
        authenticated_device: context.authenticated_device.clone(),
    };
    let context = serde_json::to_string(&context).unwrap();
    serde_json::json!({
        "frame_base64": STANDARD.encode(&frame), "invalid_signature_frame_base64": STANDARD.encode(ClientFrame::Publish { env: invalid }.encode()),
        "session": session, "context": context,
        "expected": serde_json::from_str::<serde_json::Value>(&admit_publish_json(&frame, &context).unwrap()).unwrap(),
    })
}

fn main() {
    let author = Author::device(
        &Signer::from_secret(&[7; 32]),
        Signer::from_secret(&[9; 32]),
    );
    let mut context = TrustedContext {
        authenticated_identity: author.identity.clone(),
        authenticated_device: author.device.clone().unwrap(),
        facts: FactSnapshot {
            space: "sp_cloudflare_admission_fixture".into(),
            author: author.identity.clone(),
            ..Default::default()
        },
    };
    let create = fixture(
        &author,
        context.clone(),
        EventBody::SpaceCreated {
            title: "Shared Rust fixture".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
        None,
    );
    let seq = 9_007_199_254_740_993;
    let hash = "1".repeat(64);
    context.facts.head = Some(ObservedHash {
        seq: seq.to_string(),
        hash: hash.clone(),
    });
    context.facts.history = vec![ObservedHash {
        seq: seq.to_string(),
        hash: hash.clone(),
    }];
    context.facts.kind = Some(SpaceKind::Group);
    context.facts.privacy = Some(Privacy::Closed);
    context.facts.author_role = Some(Role::Member);
    context.facts.member_count = 3;
    let exact_u64 = fixture(
        &author,
        context,
        EventBody::MessagePosted {
            message: "m_fixture".into(),
            text: "Exact UInt64".into(),
            attaches: None,
            reply: None,
            forwarded: false,
        },
        Some(Seen { seq, hash }),
    );
    println!("{}", serde_json::json!({ "fixtures": [create, exact_u64] }));
}

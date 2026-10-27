//! Owner authorization crosses the real binary protocol and directory transaction.

mod common;
use common::{RawClient, World};
use event_log::{agent_owner, Author, Signer};
use protocol::{DeviceCertificate, Op, Reply};
use types::{Identity, IdentityKind};

fn profile(author: &Author, handle: &str, owner: Option<String>) -> Identity {
    Identity {
        owner_proof: None,
        id: author.identity.clone(),
        kind: if owner.is_some() {
            IdentityKind::Agent
        } else {
            IdentityKind::Person
        },
        name: "Registered agent".into(),
        handle: handle.into(),
        tint_hex: "#4F7CFF".into(),
        glyph: None,
        owner,
        bio: String::new(),
    }
}

async fn owner_with_root(
    w: &World,
    handle: &str,
) -> std::result::Result<(Signer, RawClient), Box<dyn std::error::Error>> {
    let root = Signer::generate()?;
    let author = Author::device(&root, Signer::generate()?);
    let mut owner = RawClient::reconnect(&w.relay_url(), author).await;
    owner
        .request(Op::Register {
            profile: profile(&owner.author, handle, None),
        })
        .await
        .unwrap();
    Ok((root, owner))
}

fn authorized_profile(owner: &Author, agent: &Author, handle: &str) -> Identity {
    let mut p = profile(agent, handle, Some(owner.identity.clone()));
    p.owner_proof = Some(Box::new(
        agent_owner::authorize(owner, &agent.identity).unwrap(),
    ));
    p
}

async fn lookup(client: &mut RawClient, handle: &str) -> Vec<Identity> {
    let Ok(Reply::Profiles(found)) = client
        .request(Op::Lookup {
            handle: handle.into(),
            prefix: false,
        })
        .await
    else {
        panic!("directory lookup failed")
    };
    found
}

include!("journey_agent_ownership/registration.rs");
include!("journey_agent_ownership/authorization.rs");

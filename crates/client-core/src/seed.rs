//! Roteiro de dados fictícios da demo (um elenco, uma história contínua).
//!
//! Todas as Identidades da demo assinam com chaves geradas neste aparelho — inclusive
//! as da Marina e do Organizador, que no produto real chegariam pelo relay
//! assinadas nos aparelhos delas. Isso é explicitamente uma simulação de pares.

use types::{new_id, Capability, EventBody, Grant, GrantScope, Identity, IdentityKind, PlanLine};

use crate::engine::{now_ms, Engine, R};
use crate::CoreError;

const MIN: i64 = 60_000;

fn person(name: &str, handle: &str, tint: &str, bio: &str) -> Identity {
    Identity {
        owner_proof: None,
        id: String::new(),
        kind: IdentityKind::Person,
        name: name.into(),
        handle: handle.into(),
        tint_hex: tint.into(),
        glyph: None,
        owner: None,
        bio: bio.into(),
    }
}

fn agent(name: &str, handle: &str, tint: &str, glyph: &str, owner: &str, bio: &str) -> Identity {
    Identity {
        owner_proof: None,
        id: String::new(),
        kind: IdentityKind::Agent,
        name: name.into(),
        handle: handle.into(),
        tint_hex: tint.into(),
        glyph: Some(glyph.into()),
        owner: Some(owner.into()),
        bio: bio.into(),
    }
}

fn line(text: &str, reais: i64) -> R<PlanLine> {
    Ok(PlanLine {
        id: new_id("ln")?,
        text: text.into(),
        cost_cents: reais * 100,
        done: false,
    })
}

fn grant(grantor: &str, grantee: &str, scope: GrantScope, capability: Capability) -> R<EventBody> {
    Ok(EventBody::GrantIssued {
        grant: Grant {
            id: new_id("gr")?,
            grantor: grantor.into(),
            grantee: Some(grantee.into()),
            scope,
            capability,
            expires_at_ms: None,
        },
    })
}

fn msg(text: &str) -> R<EventBody> {
    Ok(EventBody::MessagePosted {
        message: new_id("msg")?,
        text: text.into(),
        attaches: None,
        reply: None,
        media: None,
        forwarded: false,
    })
}

mod agent_chat;
mod cast;
mod coastal_hike;
mod community;
mod crew;
mod crew_apps;
mod marina_chat;
mod paraty;
mod personal;
mod team;
use cast::Cast;
pub(crate) fn seed(e: &mut Engine) -> Result<(), CoreError> {
    let t0 = now_ms();
    let cast = Cast::create(e)?;
    personal::seed(e, &cast, t0)?;
    let com = community::seed(e, &cast, t0)?;
    let team = team::seed(e, &cast, t0)?;
    let (turma, group_delivered, group_read) = crew::seed(e, &cast, t0)?;
    let dm_zoen = agent_chat::seed(e, &cast, t0)?;
    let (dm_marina, marina_delivered, sunscreen) = marina_chat::seed(e, &cast, t0)?;
    paraty::seed(e, &cast, t0)?;
    // Preserve the original simulated peer-watermark sequence after all chapters.
    e.net.receipts.clear();
    for member in [&cast.marina, &cast.lucas, &cast.ana, &cast.zoen] {
        e.receipt(&turma, member, group_delivered, group_read)?;
    }
    e.receipt(&dm_marina, &cast.marina, marina_delivered, sunscreen)?;
    for space in [&com, &team, &dm_zoen] {
        e.mark_read(space)?;
    }
    Ok(())
}

//! Real-model smoke (not a CI test): the Creator agent builds a barber's booking bot
//! on the cheap Workers AI tier, then the bot answers a client.
//!
//!   CLOUDFLARE_WORKERS_AI_TOKEN=… CLOUDFLARE_ACCOUNT_ID=… \
//!   cargo run -p zoen-agentd --example creator_smoke
//!
//! Prints only conversation text and the created definition; never the credential.

use roda_log::{Author, Signer};
use roda_types::{Identity, IdentityKind};
use zoen_agentd::citizen::{BrainOut, ModelBrain};
use zoen_agentd::{claim_citizen, creator_definition, Avatar, Definition, ModelTier, Turn, AgentKind};

#[tokio::main]
async fn main() {
    let Some(brain) = ModelBrain::workers_ai_from_env() else {
        eprintln!("set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID");
        std::process::exit(2);
    };
    println!("model: {}", brain.model());
    let root = Signer::generate();
    let owner_dev = Author::device(&root, Signer::generate());
    let owner = Identity {
        id: root.id(), kind: IdentityKind::Person, name: "Bruno".into(), handle: "bruno".into(),
        tint_hex: "#4F7CFF".into(), glyph: None, owner: None, bio: String::new(), owner_proof: None,
    };
    let creator = creator_definition();
    let says = [
        "Oi! Sou barbeiro, a Barbearia do Bruno. Quero um bot que marque horários pelos clientes. Atendo de terça a sábado, 9h às 19h. Corte 30 min R$40, barba 20 min R$30.",
        "Perfeito, pode chamar de Agenda do Bruno, @agenda_bruno. Pode criar.",
    ];
    let mut history: Vec<Turn> = Vec::new();
    let mut spec = None;
    for (i, text) in says.iter().enumerate() {
        let inbound = Turn { space: "dm".into(), from: owner.id.clone(), text: (*text).into() };
        println!("\nBruno: {text}");
        let mut out = brain.think(&owner.id, "creator", &creator, &history, &inbound).await
            .unwrap_or_else(|e| { eprintln!("model call failed: {e}"); std::process::exit(1) });
        if i == 1 && matches!(out, BrainOut::Say(_)) {
            // One nudge if the model asked another question instead of emitting JSON.
            history.push(inbound.clone());
            if let BrainOut::Say(t) = &out { println!("Creator: {t}"); history.push(Turn { space: "dm".into(), from: "creator".into(), text: t.clone() }); }
            let ok = Turn { space: "dm".into(), from: owner.id.clone(), text: "Sim, confirmo tudo. Crie agora.".into() };
            println!("\nBruno: {}", ok.text);
            out = brain.think(&owner.id, "creator", &creator, &history, &ok).await.unwrap_or_else(|e| { eprintln!("model call failed: {e}"); std::process::exit(1) });
        }
        match out {
            BrainOut::Say(t) => { println!("Creator: {t}"); history.push(inbound); history.push(Turn { space: "dm".into(), from: "creator".into(), text: t }); }
            c @ BrainOut::CreateAgent { .. } => { println!("Creator: (create_agent)"); spec = Some(c); break; }
        }
    }
    let Some(BrainOut::CreateAgent { name, handle, persona, rules, state, glyph, tint_hex }) = spec else {
        eprintln!("\nCreator did not emit create_agent"); std::process::exit(1);
    };
    let def = Definition { version: 1, kind: AgentKind::User, persona, rules, state, model_tier: ModelTier::Cheap };
    let bot = claim_citizen(&owner_dev, &owner, name, handle, Avatar::hand_drawn(tint_hex, glyph), def.clone()).expect("claim");
    println!("\ncitizen: {} @{} id {}… owner-proof ok", bot.profile.name, bot.profile.handle, &bot.profile.id[..12]);
    println!("persona: {}", def.persona);
    println!("rules: {:?}", def.rules);
    println!("state: {}", serde_json::to_string_pretty(&def.state).unwrap());
    let client = Turn { space: "client".into(), from: "client".into(), text: "Oi, queria cortar o cabelo sábado de manhã, tem horário?".into() };
    println!("\nCliente: {}", client.text);
    match brain.think(&owner.id, &bot.profile.id, &def, &[], &client).await {
        Ok(BrainOut::Say(t)) => println!("{}: {t}", bot.profile.name),
        Ok(other) => println!("{}: {:?}", bot.profile.name, matches!(other, BrainOut::CreateAgent{..})),
        Err(e) => { eprintln!("model call failed: {e}"); std::process::exit(1) }
    }
}

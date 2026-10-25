//! Real-model smoke (not a CI test): the Creator agent builds a barber's booking bot
//! on the cheap Workers AI tier, then the bot answers a client.
//!
//!   CLOUDFLARE_WORKERS_AI_TOKEN=… CLOUDFLARE_ACCOUNT_ID=… \
//!   cargo run -p zoen-agentd --example creator_smoke
//!
//! Prints only conversation text and the created definition; never the credential.

use event_log::{Author, Signer};
use types::{Identity, IdentityKind};
use zoen_agentd::citizen::{BrainOut, ModelBrain};
use zoen_agentd::{approve_agent, creator_definition, Brain, Turn, TurnContext};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let Some(brain) = ModelBrain::workers_ai_from_env() else {
        eprintln!("set CLOUDFLARE_WORKERS_AI_TOKEN and CLOUDFLARE_ACCOUNT_ID");
        std::process::exit(2);
    };
    println!("model: {}", brain.model());
    let root = Signer::generate()?;
    let owner_dev = Author::device(&root, Signer::generate()?);
    let owner = Identity {
        id: root.id(),
        kind: IdentityKind::Person,
        name: "Bruno".into(),
        handle: "bruno".into(),
        tint_hex: "#4F7CFF".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
        owner_proof: None,
    };
    let creator = creator_definition();
    let says = [
        "Oi! Sou barbeiro, a Barbearia do Bruno. Quero um bot que marque horários pelos clientes. Atendo de terça a sábado, 9h às 19h. Corte 30 min R$40, barba 20 min R$30.",
        "Perfeito, pode chamar de Agenda do Bruno, @agenda_bruno. Pode criar.",
    ];
    let mut history: Vec<Turn> = Vec::new();
    let mut spec = None;
    for (i, text) in says.iter().enumerate() {
        let inbound = Turn {
            space: "dm".into(),
            from: owner.id.clone(),
            text: (*text).into(),
        };
        println!("\nBruno: {text}");
        let mut out = brain
            .think(
                TurnContext {
                    agent: "creator",
                    device: "creator-device",
                    owner: &owner.id,
                    definition: &creator,
                },
                &history,
                &inbound,
            )
            .await
            .unwrap_or_else(|e| {
                eprintln!("model call failed: {e}");
                std::process::exit(1)
            });
        if i == 1 && matches!(out, BrainOut::Say(_)) {
            // One nudge if the model asked another question instead of emitting JSON.
            history.push(inbound.clone());
            if let BrainOut::Say(t) = &out {
                println!("Creator: {t}");
                history.push(Turn {
                    space: "dm".into(),
                    from: "creator".into(),
                    text: t.clone(),
                });
            }
            let ok = Turn {
                space: "dm".into(),
                from: owner.id.clone(),
                text: "Sim, confirmo tudo. Crie agora.".into(),
            };
            println!("\nBruno: {}", ok.text);
            out = brain
                .think(
                    TurnContext {
                        agent: "creator",
                        device: "creator-device",
                        owner: &owner.id,
                        definition: &creator,
                    },
                    &history,
                    &ok,
                )
                .await
                .unwrap_or_else(|e| {
                    eprintln!("model call failed: {e}");
                    std::process::exit(1)
                });
        }
        match out {
            BrainOut::Say(t) => {
                println!("Creator: {t}");
                history.push(inbound);
                history.push(Turn {
                    space: "dm".into(),
                    from: "creator".into(),
                    text: t,
                });
            }
            BrainOut::Silence | BrainOut::CallTools(_) => println!("Creator: (silence)"),
            c @ BrainOut::CreateAgent { .. } => {
                if let BrainOut::CreateAgent {
                    confirmation: Some(c),
                    ..
                } = &c
                {
                    println!("Creator: {c}");
                }
                println!("Creator: (create_agent → owner approval)");
                spec = Some(c);
                break;
            }
        }
    }
    let Some(BrainOut::CreateAgent { spec, .. }) = spec else {
        eprintln!("\nCreator did not emit create_agent");
        std::process::exit(1);
    };
    // Bruno's device approves the proposal: only the owner signs a new citizen.
    let bot = approve_agent(&owner_dev, &owner, &spec).expect("approve");
    let def = spec.definition.clone();
    println!(
        "\ncitizen: {} @{} id {}… owner-proof ok",
        bot.profile.name,
        bot.profile.handle,
        &bot.profile.id[..12]
    );
    println!("persona: {}", def.persona);
    println!("rules: {:?}", def.rules);
    println!(
        "state: {}",
        serde_json::to_string_pretty(&def.state).unwrap()
    );
    let client = Turn {
        space: "client".into(),
        from: "client".into(),
        text: "Oi, queria cortar o cabelo sábado de manhã, tem horário?".into(),
    };
    println!("\nCliente: {}", client.text);
    match brain
        .think(
            TurnContext {
                agent: &bot.profile.id,
                device: bot.device_id(),
                owner: &owner.id,
                definition: &def,
            },
            &[],
            &client,
        )
        .await
    {
        Ok(BrainOut::Say(t)) => println!("{}: {t}", bot.profile.name),
        Ok(other) => println!("{}: {other:?}", bot.profile.name),
        Err(e) => {
            eprintln!("model call failed: {e}");
            std::process::exit(1)
        }
    }
    Ok(())
}

//! Operations commands use the same already-open Client.
use super::*;

pub(super) async fn run(cmd: &str, cli: &mut Cli, context: Context) {
    let Context {
        e,
        timeout,
        for_secs,
        ..
    } = context;
    match cmd {
        "app" => {
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            match cli.args.get(1).map(String::as_str) {
                Some("install") if cli.args.len() >= 4 => {
                    let it = e
                        .install_app(space, cli.args[2].clone(), cli.args[3..].join(" "))
                        .unwrap_or_else(|err| die(err));
                    e.wait_until_idle(timeout).await;
                    println!("{}", it.id);
                }
                Some("view") => {
                    for it in e.items().into_iter().filter(|i| i.space_id == space) {
                        if let Some(app) = it.app {
                            println!("{}\t{}\t{}", it.id, app.app_id, app.view_json);
                        }
                    }
                }
                _ => usage(),
            }
        }
        "growth-sync" => {
            // zoen growth-sync [--health <sessions> <crashes>]
            let (share, s, c) = match cli.args.iter().position(|a| a == "--health") {
                Some(i) => (
                    true,
                    cli.args
                        .get(i + 1)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                    cli.args
                        .get(i + 2)
                        .and_then(|v| v.parse().ok())
                        .unwrap_or(0),
                ),
                None => (false, 0, 0),
            };
            let r = e
                .growth_sync(None, share, s, c)
                .await
                .unwrap_or_else(|err| die(err));
            println!("config={} reported={}", r.config_version, r.reported);
        }
        "variant" => {
            let flag = cli.args.first().cloned().unwrap_or_else(|| usage());
            println!(
                "{}",
                e.growth_variant(flag).unwrap_or_else(|| "unknown".into())
            );
        }
        "sync" => {
            let c = e.wait_until_idle(timeout).await;
            println!(
                "connection={} synced={} pending={}{}",
                c.state,
                c.synced,
                c.pending,
                c.error.map(|x| format!(" error={x}")).unwrap_or_default()
            );
        }
        "verify" => {
            let mut bad = 0;
            for r in e.verify_all() {
                if !r.valid {
                    bad += 1;
                }
                println!(
                    "{}\t{}\t{}\t{} events\t{}",
                    r.space_id,
                    r.space_title,
                    if r.valid { "ok" } else { "BROKEN" },
                    r.events,
                    r.error.unwrap_or_default()
                );
            }
            if bad > 0 {
                std::process::exit(3);
            }
        }
        "authorize-agent" => {
            let id = cli.args.first().cloned().unwrap_or_else(|| usage());
            println!(
                "{}",
                e.authorize_agent_json(id).unwrap_or_else(|err| die(err))
            );
        }
        // Owner controls over an agent you answer for: `agent-pause @bot`, `agent-resume @bot`,
        // `agent-revoke @bot`, `agent-define @bot --definition def.json --version N`.
        "agent-pause" | "agent-resume" | "agent-revoke" | "agent-define" => {
            let who = cli.args.first().cloned().unwrap_or_else(|| usage());
            let h = who.trim_start_matches('@').to_string();
            let id = if who.starts_with('@') {
                e.find_people(h.clone())
                    .await
                    .unwrap_or_else(|err| die(err))
                    .into_iter()
                    .find(|p| p.handle == h)
                    .map(|p| p.id)
                    .unwrap_or_else(|| die(format!("@{h} isn't on Zoen")))
            } else {
                who.clone()
            };
            let r = if cmd == "agent-define" {
                let def =
                    agent::read_definition(&cli.flag("--definition").unwrap_or_else(|| usage()));
                let version: u32 = cli
                    .flag("--version")
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(def.version);
                let bytes = def.encode().unwrap_or_else(|err| die(err));
                e.agent_define(id, version, def.kind.relay_kind().into(), bytes)
                    .await
            } else {
                e.agent_control(id, cmd.trim_start_matches("agent-").to_string())
                    .await
            };
            r.unwrap_or_else(|err| die(err));
            println!("{} {who}", cmd.trim_start_matches("agent-"));
        }
        "watch" => {
            println!(
                "watching as @{} (Ctrl-C to stop)",
                e.account().map(|a| a.handle).unwrap_or_default()
            );
            let wait = async {
                match for_secs {
                    Some(s) => tokio::time::sleep(Duration::from_secs(s)).await,
                    None => {
                        let _ = tokio::signal::ctrl_c().await;
                    }
                }
            };
            wait.await;
        }
        _ => usage(),
    }
}

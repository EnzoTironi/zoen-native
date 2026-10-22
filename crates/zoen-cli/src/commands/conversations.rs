//! Conversations commands use the same already-open Client.
use super::*;

pub(super) async fn run(cmd: String, cli: &mut Cli, context: Context) {
    let Context {
        e,
        timeout,
        offline,
        for_secs,
        ..
    } = context;
    match cmd.as_str() {
        "whoami" | "status" => {
            let a = e.account().unwrap_or_else(|| die("no account"));
            let c = e.connection();
            println!(
                "@{} {} ({}) relay={} registered={} connection={} synced={} pending={}{}",
                a.handle,
                a.name,
                &a.identity_id[..12],
                a.relay_url,
                a.registered,
                c.state,
                c.synced,
                c.pending,
                c.error.map(|x| format!(" error={x}")).unwrap_or_default()
            );
        }
        "people" => {
            let q = cli.args.first().cloned().unwrap_or_else(|| usage());
            for p in e.find_people(q).await.unwrap_or_else(|err| die(err)) {
                println!("@{}\t{}\t{}", p.handle, p.name, &p.id[..12]);
            }
        }
        "dm" => {
            let h = cli.args.first().cloned().unwrap_or_else(|| usage());
            let h = h.trim_start_matches('@').to_string();
            let people = e
                .find_people(h.clone())
                .await
                .unwrap_or_else(|err| die(err));
            let p = people
                .into_iter()
                .find(|p| p.handle == h)
                .unwrap_or_else(|| die(format!("@{h} isn't on Zoen")));
            let space = e.start_direct(p.id).unwrap_or_else(|err| die(err));
            if cli.args.len() > 1 {
                e.send_message(space.clone(), cli.args[1..].join(" "))
                    .unwrap_or_else(|err| die(err));
            }
            e.wait_until_idle(timeout).await;
            println!("{space}");
        }
        "group" => {
            let privacy = if cli.switch("--readable") {
                PrivacyDto::Closed
            } else {
                PrivacyDto::EndToEnd
            };
            if cli.args.is_empty() {
                usage();
            }
            let title = cli.args.remove(0);
            let mut ids = vec![];
            for h in &cli.args {
                let h = h.trim_start_matches('@').to_string();
                let p = e
                    .find_people(h.clone())
                    .await
                    .unwrap_or_else(|err| die(err))
                    .into_iter()
                    .find(|p| p.handle == h)
                    .unwrap_or_else(|| die(format!("@{h} isn't on Zoen")));
                ids.push(p.id);
            }
            let space = e
                .create_group_with(title, ids, privacy)
                .unwrap_or_else(|err| die(err));
            if !offline {
                e.wait_until_settled(timeout)
                    .await
                    .unwrap_or_else(|err| die(err));
            }
            println!("{space}");
        }
        "send" => {
            if cli.args.len() < 2 {
                usage();
            }
            let space = chat(&e, &cli.args[0]);
            let t = e
                .send_message(space, cli.args[1..].join(" "))
                .unwrap_or_else(|err| die(err));
            if !offline {
                e.wait_until_idle(timeout).await;
            }
            let c = e.connection();
            println!(
                "{} {}",
                if offline || c.pending > 0 {
                    "queued"
                } else {
                    "sent"
                },
                t.id
            );
        }
        "read" => {
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            if cli.switch("--older") {
                let wait: u64 = std::env::var("ZOEN_PAGE_TIMEOUT_MS")
                    .ok()
                    .and_then(|v| v.parse().ok())
                    .unwrap_or(8000);
                let older = e
                    .load_older(space.clone(), wait)
                    .await
                    .unwrap_or_else(|err| die(err));
                match older.message {
                    Some(m) => println!("({m})"),
                    None => println!(
                        "(older: {} more{})",
                        older.loaded,
                        if older.more { ", still more" } else { "" }
                    ),
                }
            }
            for t in e.timeline(space.clone()).unwrap_or_else(|err| die(err)) {
                println!("{}", line(&t));
            }
            let _ = e.mark_read(space);
        }
        "add" | "remove" => {
            let admin = cli.switch("--admin");
            if cli.args.len() != 2 {
                usage();
            }
            let space = chat(&e, &cli.args[0]);
            let h = cli.args[1].trim_start_matches('@').to_string();
            let who = e
                .find_people(h.clone())
                .await
                .unwrap_or_else(|err| die(err))
                .into_iter()
                .find(|p| p.handle == h)
                .unwrap_or_else(|| die(format!("@{h} isn't on Zoen")));
            match (cmd.as_str(), admin) {
                ("add", true) => e.add_admin(space, who.id),
                ("add", false) => e.add_member(space, who.id),
                _ => e.remove_member(space, who.id),
            }
            .unwrap_or_else(|err| die(err));
            e.wait_until_idle(timeout).await;
        }
        "encrypt" => {
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            e.encrypt_chat(space).unwrap_or_else(|err| die(err));
            e.wait_until_idle(timeout).await;
        }
        "keys" => {
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            match e.group_keys(space.clone()) {
                Some(k) => println!(
                    "epoch={}\tdigest={}\tmembers={}\tdevices={}",
                    k.epoch,
                    k.digest,
                    k.members.len(),
                    e.group_devices(space.clone()).len()
                ),
                None => die("no group keys for that chat on this device"),
            }
        }
        "chats" => {
            for s in e.spaces() {
                let where_ = if e.is_synced(s.id.clone()) {
                    "relay"
                } else {
                    "local"
                };
                let privacy = match s.privacy {
                    PrivacyDto::EndToEnd => "e2e",
                    PrivacyDto::Closed | PrivacyDto::Public => "readable",
                };
                println!(
                    "{}\t{}\t{:?}\t{}\t{}\tunread={}\t{}",
                    s.id, s.title, s.kind, where_, privacy, s.unread, s.last_preview
                );
            }
        }
        "invite" => {
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            let inv = e.create_invite(space).await.unwrap_or_else(|err| die(err));
            println!("{}\t{}", inv.code, inv.link);
        }
        "agent" => {
            let target = cli.args.first().cloned().unwrap_or_else(|| usage());
            let chat = e
                .open_agent_link(target)
                .await
                .unwrap_or_else(|err| die(err));
            e.wait_until_idle(timeout).await;
            println!("{chat}");
        }
        "join" => {
            let code = cli.args.first().cloned().unwrap_or_else(|| usage());
            let space = e.join_invite(code).await.unwrap_or_else(|err| die(err));
            e.wait_until_idle(timeout).await;
            println!("{space}");
        }
        "typing" => {
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            let secs = for_secs.unwrap_or(3);
            for _ in 0..secs.max(1) {
                e.set_typing(space.clone(), true);
                tokio::time::sleep(Duration::from_secs(1)).await;
            }
            e.set_typing(space, false);
            tokio::time::sleep(Duration::from_millis(200)).await;
        }
        _ => usage(),
    }
}

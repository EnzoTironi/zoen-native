//! Account commands use the same already-open Client.
use super::*;

pub(super) async fn run(cmd: String, cli: &mut Cli, context: Context) {
    let Context {
        e,
        vault,
        me,
        timeout,
        for_secs,
        ..
    } = context;
    match cmd.as_str() {
        "backup" => {
            let sub = if cli.args.is_empty() {
                "status".to_string()
            } else {
                cli.args.remove(0)
            };
            let show = |s: client_core::BackupStatusDto| {
                println!(
                    "backup {} mode={} bytes={} last={}",
                    if s.enabled { "on" } else { "off" },
                    if s.mode.is_empty() { "-" } else { &s.mode },
                    s.bytes,
                    s.last_backup_ms
                )
            };
            match sub.as_str() {
                "on" => {
                    if let Some(p) = cli.flag("--password") {
                        let s = e
                            .backup_turn_on_password(p, vault.clone())
                            .await
                            .unwrap_or_else(|err| die(err));
                        show(s);
                    } else if cli.switch("--recovery-key") {
                        let rk = e
                            .backup_turn_on_recovery_key(vault.clone())
                            .await
                            .unwrap_or_else(|err| die(err));
                        println!("recovery-key\t{rk}");
                        show(e.backup_status());
                    } else {
                        usage();
                    }
                }
                "now" => show(
                    e.backup_now(vault.clone())
                        .await
                        .unwrap_or_else(|err| die(err)),
                ),
                "off" => {
                    e.backup_turn_off(vault.clone())
                        .await
                        .unwrap_or_else(|err| die(err));
                    show(e.backup_status());
                }
                "status" => show(e.backup_status()),
                _ => usage(),
            }
        }
        "link" => {
            let code = cli.args.first().cloned().unwrap_or_else(|| usage());
            let linked = e
                .link_device(code, vault.clone())
                .await
                .unwrap_or_else(|err| die(err));
            println!("check: {}", linked.check);
            // The new device comes online and publishes key packages; then this device
            // adds it to every group, and only then seals the history for it.
            let deadline =
                std::time::Instant::now() + std::time::Duration::from_secs(for_secs.unwrap_or(120));
            loop {
                e.wait_until_idle(timeout).await;
                let p = e.link_progress(linked.device_id.clone()).await;
                if p[0] == p[1] {
                    println!("in {} of {} groups", p[0], p[1]);
                    break;
                }
                if std::time::Instant::now() > deadline {
                    die(format!("the new device is in {} of {} groups", p[0], p[1]));
                }
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            }
            let n = e
                .send_history(linked.device_id.clone(), Some(Arc::new(Progress)))
                .await
                .unwrap_or_else(|err| die(err));
            println!("history sent: {n} messages");
        }
        "history" => receive_history(&e, timeout).await,
        "devices" => {
            for d in e.devices().await.unwrap_or_else(|err| die(err)) {
                println!(
                    "{}{}{}",
                    d.device_id,
                    if d.this_device { " (this device)" } else { "" },
                    if d.revoked { " unlinked" } else { "" }
                );
            }
        }
        "unlink" => {
            let prefix = cli.args.first().cloned().unwrap_or_else(|| usage());
            let device = e
                .devices()
                .await
                .unwrap_or_else(|err| die(err))
                .into_iter()
                .find(|d| d.device_id.starts_with(&prefix) && !d.this_device && !d.revoked)
                .unwrap_or_else(|| die("no such linked device"));
            e.unlink_device(device.device_id.clone())
                .await
                .unwrap_or_else(|err| die(err));
            e.wait_until_idle(timeout).await;
            println!("unlinked {}", &device.device_id[..12]);
        }
        "background" => {
            let path = cli.flag("--photo").unwrap_or_else(|| usage());
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            let bytes = std::fs::read(&path).unwrap_or_else(|err| die(format!("{path}: {err}")));
            let m = e
                .put_media(bytes, "image/jpeg".into(), 1, 1)
                .unwrap_or_else(|err| die(err));
            let bg = client_core::BackgroundDto {
                style: "photo".into(),
                media: Some(m.clone()),
                zoom_pm: 1000,
                offset_x_pm: 0,
                offset_y_pm: 0,
                dim_pm: None,
                blur_pm: 0,
                appearance: "auto".into(),
            };
            e.set_background(space, bg).unwrap_or_else(|err| die(err));
            e.wait_until_idle(timeout).await;
            println!("{}", m.sha256);
        }
        "photo" => {
            let out = cli.flag("--out").unwrap_or_else(|| usage());
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            let bytes = e
                .wait_for_background_media(space, timeout)
                .await
                .unwrap_or_else(|| die("no photo yet"));
            std::fs::write(&out, &bytes).unwrap_or_else(|err| die(format!("{out}: {err}")));
            println!("{} bytes", bytes.len());
        }
        "profile" => {
            let sub = cli.args.first().cloned().unwrap_or_else(|| usage());
            match sub.as_str() {
                "set" => {
                    let current = e.get_profile(me.clone()).unwrap_or_else(|err| die(err));
                    let name = cli.flag("--name").or(current.name).unwrap_or_default();
                    let bio = cli.flag("--bio").or(current.bio).unwrap_or_default();
                    let photo = match (cli.flag("--photo"), cli.switch("--no-photo")) {
                        (Some(path), _) => {
                            let bytes = std::fs::read(&path)
                                .unwrap_or_else(|err| die(format!("{path}: {err}")));
                            let mime = if path.ends_with(".png") {
                                "image/png"
                            } else {
                                "image/jpeg"
                            };
                            PhotoChange::Set {
                                bytes,
                                mime: mime.into(),
                            }
                        }
                        (None, true) => PhotoChange::Remove,
                        (None, false) => PhotoChange::Keep,
                    };
                    let p = e
                        .update_my_profile(name, bio, photo)
                        .unwrap_or_else(|err| die(err));
                    e.wait_until_idle(timeout).await;
                    println!("{}", profile_line(&p));
                }
                "show" => {
                    let out = cli.flag("--out");
                    let id = match cli.args.get(1) {
                        Some(h) => person(&e, h).await,
                        None => me.clone(),
                    };
                    e.wait_until_idle(timeout).await;
                    let mut p = e.get_profile(id.clone()).unwrap_or_else(|err| die(err));
                    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
                    while out.is_some()
                        && p.photo_sha256.is_some()
                        && !p.photo_ready
                        && tokio::time::Instant::now() < deadline
                    {
                        tokio::time::sleep(Duration::from_millis(100)).await;
                        p = e.get_profile(id.clone()).unwrap_or_else(|err| die(err));
                    }
                    println!("{}", profile_line(&p));
                    if let (Some(out), Some(sha), true) = (out, &p.photo_sha256, p.photo_ready) {
                        let bytes = e
                            .media(sha.clone())
                            .ok()
                            .flatten()
                            .unwrap_or_else(|| die("photo not on this device"));
                        std::fs::write(&out, &bytes)
                            .unwrap_or_else(|err| die(format!("{out}: {err}")));
                    }
                }
                _ => usage(),
            }
        }
        "block" | "unblock" => {
            let h = cli.args.first().cloned().unwrap_or_else(|| usage());
            let id = person(&e, &h).await;
            if cmd == "block" {
                e.block_person(id).unwrap_or_else(|err| die(err));
            } else {
                e.unblock_person(id).unwrap_or_else(|err| die(err));
            }
            e.wait_until_idle(timeout).await;
            println!("{cmd}ed {h}");
        }
        _ => usage(),
    }
}

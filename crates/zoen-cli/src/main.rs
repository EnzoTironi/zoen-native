//! `zoen` — a person at a terminal, using the exact core the iPhone app uses.
//!
//! ```text
//! zoen --home ~/.zoen-ana init --name Ana --handle ana --relay http://127.0.0.1:8787
//! zoen --home ~/.zoen-ana dm @bruno "oi Bruno"
//! zoen --home ~/.zoen-bruno read @ana
//! ```
//! Every command is a fresh process: open the database, load the key from the vault,
//! connect, catch up, act, flush, exit. Killing it at any point loses nothing.

mod agent;
mod agent_enrollment;
mod agent_host_lock;
mod agent_inspect;
mod cli;
mod commands;
mod deckcraft;
mod file_vault;
mod image_protocol;
mod terminal;

use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use cli::{usage, Cli};
use client_core::{
    Client, ConnectionDto, CoreListener, Delivery, EntryKind, MarkdownFileDto, PhotoChange,
    PrivacyDto, ProfileDto, SecretVault, SpaceKindDto,
};
use file_vault::FileVault;
use terminal::{line, receive_history, Printer, Progress};

/// `@ana  Ana  bio=…  photo=<sha>|pending|none  v3`, or `(hidden)` without their key.
fn profile_line(p: &ProfileDto) -> String {
    let photo = match (&p.photo_sha256, p.photo_ready) {
        (Some(sha), true) => sha[..12].to_string(),
        (Some(_), false) => "pending".into(),
        (None, _) => "none".into(),
    };
    match &p.name {
        Some(name) => format!(
            "@{}\t{}\tbio={}\tphoto={}\tv{}{}",
            p.handle,
            name,
            p.bio.clone().unwrap_or_default(),
            photo,
            p.version,
            if p.blocked { "\tblocked" } else { "" }
        ),
        None => format!("@{}\t(hidden)", p.handle),
    }
}

/// The identity behind `@handle`: someone this device knows, else the directory.
async fn person(e: &Client, handle: &str) -> String {
    let h = handle.trim_start_matches('@').to_string();
    if let Some(p) = e.personas().into_iter().find(|p| p.handle == h) {
        return p.id;
    }
    e.find_people(h.clone())
        .await
        .unwrap_or_else(|err| die(err))
        .into_iter()
        .find(|p| p.handle == h)
        .map(|p| p.id)
        .unwrap_or_else(|| die(format!("@{h} isn't on Zoen")))
}

fn open(home: &Path) -> Arc<Client> {
    std::fs::create_dir_all(home).expect("home dir");
    let db = home.join("zoen.sqlite");
    match Client::open(
        db.to_string_lossy().into(),
        std::env::var("ZOEN_LOCALE").unwrap_or_else(|_| "en-US".into()),
    ) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("can't open {}: {e}", db.display());
            std::process::exit(1)
        }
    }
}

pub(crate) fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}

/// Resolves `@handle`, a title, or a space id (prefix ok) to a space id.
fn chat(e: &Client, q: &str) -> String {
    let spaces = e.spaces();
    if let Some(h) = q.strip_prefix('@') {
        if let Some(s) = spaces.iter().find(|s| {
            s.kind == SpaceKindDto::Direct
                && s.counterpart.as_ref().is_some_and(|c| c.handle == h)
                && e.is_synced(s.id.clone())
        }) {
            return s.id.clone();
        }
        die(format!("no chat with @{h} yet (try `zoen dm @{h}`)"));
    }
    spaces
        .iter()
        .find(|s| s.id == q || s.title == q || (q.len() >= 6 && s.id.starts_with(q)))
        .map(|s| s.id.clone())
        .unwrap_or_else(|| die(format!("no chat matches {q:?}")))
}

/// Resolves an Item by id (prefix ok), folder path or title.
fn item(e: &Client, q: &str) -> client_core::ItemDetail {
    let items = e.items();
    items
        .iter()
        .find(|i| i.id == q)
        .or_else(|| items.iter().find(|i| !i.path.is_empty() && i.path == q))
        .or_else(|| items.iter().find(|i| q.len() >= 6 && i.id.starts_with(q)))
        .or_else(|| items.iter().find(|i| i.title == q))
        .cloned()
        .unwrap_or_else(|| die(format!("no item matches {q:?}")))
}

/// Waits until this device has every piece of the page (or file) `id`.
async fn ready(e: &Client, id: &str, timeout: u64) -> bool {
    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
    loop {
        let ok = match e.item(id.to_string()) {
            Ok(d) if d.kind_id == "page" => {
                e.page(id.to_string()).map(|p| p.ready).unwrap_or(false)
            }
            Ok(d) => d.file.map(|f| f.ready).unwrap_or(true),
            Err(_) => false,
        };
        if ok || tokio::time::Instant::now() >= deadline {
            return ok;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

fn mime_of(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    match ext.as_str() {
        "md" | "markdown" => "text/markdown",
        "txt" => "text/plain",
        "pdf" => "application/pdf",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "heic" => "image/heic",
        "gif" => "image/gif",
        "mp4" => "video/mp4",
        "mov" => "video/quicktime",
        "mp3" => "audio/mpeg",
        "m4a" => "audio/mp4",
        "zip" => "application/zip",
        "csv" => "text/csv",
        "json" => "application/json",
        "docx" => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        "xlsx" => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        "pptx" => "application/vnd.openxmlformats-officedocument.presentationml.presentation",
        _ => "application/octet-stream",
    }
}

fn read_file(path: &str) -> Vec<u8> {
    std::fs::read(path).unwrap_or_else(|err| die(format!("{path}: {err}")))
}

fn write_file(path: &Path, bytes: &[u8]) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    std::fs::write(path, bytes).unwrap_or_else(|err| die(format!("{}: {err}", path.display())));
}

/// Applies one block change to a page and saves it as a version.
fn edit_page(
    e: &Client,
    id: &str,
    f: impl FnOnce(&mut Vec<client_core::PageBlockDto>) -> Vec<String>,
) {
    let p = e.page(id.to_string()).unwrap_or_else(|err| die(err));
    let mut blocks = p.blocks;
    let changed_ids = f(&mut blocks);
    let order: Vec<String> = blocks.iter().map(|b| b.id.clone()).collect();
    let changed: Vec<_> = blocks
        .into_iter()
        .filter(|b| changed_ids.contains(&b.id))
        .collect();
    e.page_apply(id.to_string(), order, changed)
        .unwrap_or_else(|err| die(err));
    let saved = e
        .page_commit(id.to_string(), String::new())
        .unwrap_or_else(|err| die(err));
    println!("{}", if saved { "saved" } else { "unchanged" });
}

fn new_block_id() -> String {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!("{:016x}", (n as u64) ^ ((std::process::id() as u64) << 40))
}

#[tokio::main(flavor = "multi_thread", worker_threads = 2)]
async fn main() {
    let mut cli = Cli {
        args: std::env::args().skip(1).collect(),
    };
    let home = cli
        .flag("--home")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("ZOEN_HOME").map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from(".zoen"));
    let timeout: u64 = cli
        .flag("--timeout")
        .and_then(|t| t.parse().ok())
        .unwrap_or(8000);
    let offline = cli.switch("--offline");
    let for_secs: Option<u64> = cli.flag("--for").and_then(|t| t.parse().ok());
    if cli.args.is_empty() {
        usage();
    }
    let cmd = cli.args.remove(0);
    let agent_source = if cmd == "agent-host" {
        agent::Source::parse(&std::env::args().skip(1).collect::<Vec<_>>())
            .unwrap_or_else(|error| die(error))
    } else {
        None
    };
    if agent_source.is_some() {
        agent_inspect::existing_home(&home)
            .unwrap_or_else(|_| die("agent_host_source_existing_home_required"));
    }
    if cmd == "agent-inspect" {
        agent_inspect::validate_args(&std::env::args().skip(1).collect::<Vec<_>>())
            .unwrap_or_else(|error| die(error));
        agent_inspect::existing_home(&home).unwrap_or_else(|error| die(error));
    }
    // Hold the local host's OS lock through sync shutdown, before opening any
    // account state or provider. Early returns and process death release it too.
    let enrollment = agent_enrollment::Action::parse(&cmd, &cli.args, timeout, offline)
        .unwrap_or_else(|err| die(err));
    let agent_host_lock = if agent_enrollment::requires_home_lock(&cmd) {
        Some(
            agent_host_lock::HostLock::acquire(&home).unwrap_or_else(|err| {
                if cmd == "agent-inspect" {
                    die("agent_inspect_home_locked_or_unavailable");
                }
                die(err)
            }),
        )
    } else {
        None
    };
    let agent_readiness = if cmd == "agent-host" {
        Some(
            agent::Readiness::reset(
                &home,
                agent_host_lock.as_ref().expect("agent-host home lock"),
            )
            .unwrap_or_else(|error| die(error)),
        )
    } else {
        None
    };
    let concrete_vault = Arc::new(FileVault {
        dir: home.join("vault"),
    });
    if let Some(action) = &enrollment {
        action
            .preflight(&concrete_vault)
            .unwrap_or_else(|err| die(err));
    }
    let vault: Arc<dyn SecretVault> = concrete_vault.clone();
    let e = if cmd == "agent-inspect" {
        agent_inspect::open_existing(&home).unwrap_or_else(|error| die(error))
    } else {
        open(&home)
    };
    if cmd == "agent-inspect" {
        let result = async {
            if !e
                .unlock(vault.clone())
                .map_err(|_| "agent_inspect_unlock_failed")?
            {
                return Err("agent_inspect_missing_account");
            }
            agent_inspect::preflight(&e)?;
            e.start_sync(None)
                .map_err(|_| "agent_inspect_sync_start_failed")?;
            agent_inspect::inspect(&e, timeout).await
        }
        .await;
        e.stop_sync();
        println!("{}", result.unwrap_or_else(|error| die(error)));
        return;
    }
    if let Some(action) = enrollment {
        let result = action.run(&e, concrete_vault).await;
        e.stop_sync();
        println!("{}", result.unwrap_or_else(|err| die(err)));
        return;
    }

    // Growth (ADR 0044): what a fresh install does before there is an account.
    if cmd == "open" {
        let link = cli.args.first().cloned().unwrap_or_else(|| usage());
        let a = e.growth_capture_link(link);
        println!(
            "source={} campaign={} target={}",
            a.kind,
            a.campaign.unwrap_or_default(),
            a.target.unwrap_or_default()
        );
        return;
    }
    if cmd == "onboarding" {
        let relay = cli
            .flag("--relay")
            .or_else(|| std::env::var("ZOEN_RELAY").ok());
        if relay.is_some() || e.account().is_some() {
            let _ = e.growth_sync(relay, false, 0, 0).await;
        }
        let p = match e.growth_onboarding_plan() {
            Ok(plan) => plan,
            Err(error) => die(error),
        };
        println!("flow={}", p.flow);
        println!("steps={}", p.steps.join(","));
        println!("landing={}", p.landing);
        println!("target={}", p.landing_target.unwrap_or_default());
        println!("experiment={}", p.experiment.unwrap_or_default());
        let mut copy: Vec<_> = p.copy.into_iter().collect();
        copy.sort();
        for (k, v) in copy {
            println!("copy.{k}={v}");
        }
        return;
    }

    // Agent citizens (server-side runtime). Keys first, so the owner can authorize the id.
    if cmd == "agent-keys" {
        let root = match vault.load("agent_root".into()) {
            Some(r) => r,
            None => {
                let mut r = [0u8; 32];
                getrandom_fill(&mut r);
                if !vault.save("agent_root".into(), r.to_vec()) {
                    die("vault");
                }
                r.to_vec()
            }
        };
        println!(
            "{}",
            client_core::agent_id_for_root(root).unwrap_or_else(|err| die(err))
        );
        return;
    }
    if cmd == "agent-init" {
        let name = cli.flag("--name").unwrap_or_else(|| usage());
        let handle = cli.flag("--handle").unwrap_or_else(|| usage());
        let owner = cli.flag("--owner").unwrap_or_else(|| usage());
        let proof = cli.flag("--proof").unwrap_or_else(|| usage());
        let def_path = cli.flag("--definition").unwrap_or_else(|| usage());
        let relay = cli
            .flag("--relay")
            .or_else(|| std::env::var("ZOEN_RELAY").ok())
            .unwrap_or_else(|| "http://127.0.0.1:8787".into());
        let root = vault
            .load("agent_root".into())
            .unwrap_or_else(|| die("run agent-keys first"));
        e.create_agent_account(name, handle, relay, root, owner, proof, vault.clone())
            .unwrap_or_else(|err| die(err));
        e.start_sync(None).unwrap_or_else(|err| die(err));
        let c = e.wait_until_idle(timeout).await;
        let a = e.account().unwrap_or_else(|| die("account"));
        if a.registered {
            // The real kind and definition, recorded on the relay at registration.
            agent::define_first(&e, &def_path).await;
        }
        println!(
            "agent @{} ({}) {}",
            a.handle,
            &a.identity_id[..12],
            if a.registered {
                "registered".to_string()
            } else {
                c.error.unwrap_or_default()
            }
        );
        e.stop_sync();
        return;
    }

    if cmd == "init" {
        let name = cli.flag("--name").unwrap_or_else(|| usage());
        let handle = cli.flag("--handle").unwrap_or_else(|| usage());
        let relay = cli
            .flag("--relay")
            .or_else(|| std::env::var("ZOEN_RELAY").ok())
            .unwrap_or_else(|| "http://127.0.0.1:8787".into());
        let acct = e
            .create_account(name, handle, relay, vault.clone())
            .unwrap_or_else(|err| die(err));
        e.start_sync(None).unwrap_or_else(|err| die(err));
        let c = e.wait_until_idle(timeout).await;
        let acct2 = e.account().unwrap_or(acct);
        println!(
            "@{} ({}) on {} — {}",
            acct2.handle,
            &acct2.identity_id[..12],
            acct2.relay_url,
            if acct2.registered {
                "registered"
            } else {
                c.error
                    .unwrap_or_else(|| "not registered yet (offline?)".into())
                    .as_str()
                    .to_string()
                    .leak()
            }
        );
        e.stop_sync();
        return;
    }

    if cmd == "recover" {
        if cli.args.is_empty() {
            usage();
        }
        let handle = cli.args.remove(0);
        let relay = cli
            .flag("--relay")
            .or_else(|| std::env::var("ZOEN_RELAY").ok())
            .unwrap_or_else(|| "http://127.0.0.1:8787".into());
        let secret = cli
            .flag("--password")
            .or_else(|| cli.flag("--recovery-key"))
            .unwrap_or_else(|| usage());
        let acct = e
            .restore_backup(relay, handle, secret, vault.clone())
            .await
            .unwrap_or_else(|err| die(err));
        e.start_sync(None).unwrap_or_else(|err| die(err));
        let c = e.wait_until_idle(timeout).await;
        println!(
            "@{} ({}) restored on a new device {} — {} chats, connection={}",
            acct.handle,
            &acct.identity_id[..12],
            &acct.device_id[..12],
            e.spaces().len(),
            c.state
        );
        e.stop_sync();
        return;
    }

    if cmd == "link-request" {
        let relay = cli
            .flag("--relay")
            .or_else(|| std::env::var("ZOEN_RELAY").ok())
            .unwrap_or_else(|| "http://127.0.0.1:8787".into());
        let req = e
            .link_request(relay, vault.clone())
            .unwrap_or_else(|err| die(err));
        println!("code: {}", req.code);
        println!("check: {}", req.check);
        use std::io::Write;
        let _ = std::io::stdout().flush();
        let wait = for_secs.map(|s| s * 1000).unwrap_or(120_000);
        let acct = e
            .link_wait(vault.clone(), wait)
            .await
            .unwrap_or_else(|err| die(err));
        e.start_sync(None).unwrap_or_else(|err| die(err));
        e.wait_until_idle(timeout).await;
        println!(
            "linked: @{} ({}) device {}",
            acct.handle,
            &acct.identity_id[..12],
            &acct.device_id[..12]
        );
        let _ = std::io::stdout().flush();
        receive_history(&e, wait).await;
        e.wait_until_idle(timeout).await;
        e.stop_sync();
        return;
    }

    if !e.unlock(vault.clone()).unwrap_or_else(|err| {
        if agent_source.is_some() {
            die("agent_host_source_unlock_failed");
        }
        die(err)
    }) {
        if agent_source.is_some() {
            die("agent_host_source_account_unavailable");
        }
        die(format!(
            "no account in {} (run `zoen init`)",
            home.display()
        ));
    }
    if agent_source.is_some() {
        agent::Source::preflight(&e).unwrap_or_else(|error| die(error));
    }
    let me = e.account().map(|a| a.identity_id).unwrap_or_default();
    let watching = cmd == "watch";
    let mut sync_budget = commands::SyncBudget::new(timeout);
    if !offline {
        let listener: Option<Arc<dyn CoreListener>> = if watching {
            Some(Arc::new(Printer {
                me: me.clone(),
                engine: Arc::downgrade(&e),
            }))
        } else {
            None
        };
        e.start_sync(listener).unwrap_or_else(|err| die(err));
        let c = sync_budget.wait_startup(&e).await;
        if c.state != "online"
            && !matches!(
                cmd.as_str(),
                "status" | "read" | "chats" | "verify" | "whoami"
            )
        {
            die(format!("offline: {}", c.error.unwrap_or_default()));
        }
    }

    match cmd.as_str() {
        "agent-host" => {
            if let Err(error) = agent::host(
                &e,
                &home,
                agent_readiness.as_ref().expect("agent-host readiness"),
                for_secs,
                agent_source,
            )
            .await
            {
                e.stop_sync();
                drop(agent_readiness);
                drop(agent_host_lock);
                die(error);
            }
        }
        _ => {
            commands::run(
                cmd,
                &mut cli,
                commands::Context {
                    e: e.clone(),
                    vault: vault.clone(),
                    me,
                    timeout,
                    sync_budget,
                    offline,
                    for_secs,
                },
            )
            .await
        }
    }
    e.stop_sync();
    drop(agent_readiness);
    drop(agent_host_lock);
}

fn getrandom_fill(buf: &mut [u8]) {
    let mut f = std::fs::File::open("/dev/urandom").unwrap_or_else(|err| die(err));
    std::io::Read::read_exact(&mut f, buf).unwrap_or_else(|err| die(err));
}

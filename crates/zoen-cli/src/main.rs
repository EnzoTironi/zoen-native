//! `zoen` — a person at a terminal, using the exact core the iPhone app uses.
//!
//! ```text
//! zoen --home ~/.zoen-ana init --name Ana --handle ana --relay http://127.0.0.1:8787
//! zoen --home ~/.zoen-ana dm @bruno "oi Bruno"
//! zoen --home ~/.zoen-bruno read @ana
//! ```
//! Every command is a fresh process: open the database, load the key from the vault,
//! connect, catch up, act, flush, exit. Killing it at any point loses nothing.

use std::{
    io::Write,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use roda_ffi::{
    ConnectionDto, CoreListener, Delivery, EntryKind, MarkdownFileDto, PhotoChange, PrivacyDto,
    ProfileDto, RodaEngine, SecretVault, SpaceKindDto,
};

struct FileVault {
    dir: PathBuf,
}

impl SecretVault for FileVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        std::fs::read(self.dir.join(key)).ok()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        if std::fs::create_dir_all(&self.dir).is_err() {
            return false;
        }
        let path = self.dir.join(key);
        let mut opts = std::fs::OpenOptions::new();
        opts.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            opts.mode(0o600);
        }
        opts.open(path)
            .and_then(|mut f| f.write_all(&value))
            .is_ok()
    }
    fn delete(&self, key: String) {
        let _ = std::fs::remove_file(self.dir.join(key));
    }
}

struct Printer {
    me: String,
    engine: std::sync::Weak<RodaEngine>,
}

impl CoreListener for Printer {
    fn on_change(&self, space_ids: Vec<String>) {
        let Some(e) = self.engine.upgrade() else {
            return;
        };
        for s in space_ids {
            if let Ok(entries) = e.timeline(s.clone()) {
                if let Some(last) = entries.last() {
                    if last.author.id != self.me {
                        println!("[{}] {}", title_of(&e, &s), line(last));
                    }
                }
            }
        }
    }
    fn on_ephemeral(&self, space_id: String, from_id: String, kind: String, detail: String) {
        let Some(e) = self.engine.upgrade() else {
            return;
        };
        let who = e
            .personas()
            .into_iter()
            .find(|p| p.id == from_id)
            .map(|p| p.name)
            .unwrap_or_else(|| "someone".into());
        let what = match kind.as_str() {
            "typing" => "is typing…".to_string(),
            "stopped" => "stopped typing".to_string(),
            "status" => format!("is {detail}"),
            other => format!("{other} {detail}"),
        };
        println!("[{}] {who} {what}", title_of(&e, &space_id));
    }
    fn on_presence(&self, identity_id: String, online: bool) {
        let Some(e) = self.engine.upgrade() else {
            return;
        };
        if let Some(p) = e.personas().into_iter().find(|p| p.id == identity_id) {
            println!(
                "· {} is {}",
                p.name,
                if online { "online" } else { "offline" }
            );
        }
    }
    fn on_connection(&self, _status: ConnectionDto) {}
    fn on_error(&self, message: String) {
        eprintln!("! relay refused: {message}");
    }
    fn on_profile_changed(&self, identity_id: String) {
        let Some(e) = self.engine.upgrade() else {
            return;
        };
        if let Ok(p) = e.get_profile(identity_id) {
            println!("· @{} updated their profile", p.handle);
        }
    }
}

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
async fn person(e: &RodaEngine, handle: &str) -> String {
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

fn title_of(e: &RodaEngine, space: &str) -> String {
    e.space(space.to_string())
        .map(|s| s.title)
        .unwrap_or_else(|_| space.chars().take(10).collect())
}

fn line(t: &roda_ffi::TimelineEntry) -> String {
    let body = match &t.kind {
        EntryKind::Message { text, .. } => text.clone(),
        EntryKind::ItemEdited { title, note, .. } => format!("✎ {title}: {note}"),
        EntryKind::Request { title, .. } => format!("? {title}"),
        EntryKind::System { text } => format!("· {text}"),
        EntryKind::Background { background } => format!("· background: {}", background.style),
    };
    let mark = match t.delivery {
        Delivery::Sending => " (sending)",
        Delivery::Failed => " (failed)",
        _ => "",
    };
    format!("{}: {body}{mark}", t.author.name)
}

fn usage() -> ! {
    eprintln!(
        "usage: zoen [--home DIR] <command>\n\n\
         init --name NAME --handle HANDLE [--relay URL]\n\
         whoami | status | chats | verify\n\
         people QUERY\n\
         dm @HANDLE [TEXT]                end-to-end (MLS): the relay holds ciphertext\n\
         group TITLE @HANDLE... [--readable]   end-to-end unless --readable\n\
         encrypt CHAT                     make a relay-readable chat end-to-end, for good\n\
         add CHAT @HANDLE [--admin]       bring someone in (an admin can add and remove too)\n\
         remove CHAT @HANDLE              take someone out (end-to-end: they read nothing after)\n\
         keys CHAT                        an end-to-end chat's group: epoch, digest, members\n\
         send CHAT TEXT [--offline]      CHAT = @handle, title or space id\n\
         read CHAT\n\
         invite CHAT\n\
         join CODE\n\
         sync [--timeout MS]\n\
         watch [--for SECONDS]\n\
         typing CHAT [--for SECONDS]\n\
         background CHAT --photo FILE     photo background for everyone (encrypted on the relay)\n\
         photo CHAT --out FILE            save the chat's background photo\n\
         profile set [--name N] [--bio B] [--photo FILE | --no-photo]\n\
         profile show [@HANDLE] [--out FILE]   what this device can read (FILE = their photo)\n\
         block @HANDLE | unblock @HANDLE\n\
         page import CHAT FILE... [--root DIR]  Markdown files as pages (path relative to DIR)\n\
         page export ITEM --out FILE | page export-all CHAT --dir DIR\n\
         page show ITEM | page check ITEM N | page add ITEM TEXT | page set ITEM N TEXT\n\
         page new CHAT TITLE | pages CHAT\n\
         file add CHAT FILE [--thumb PNG] [--path DIR] | files CHAT\n\
         file get ITEM --out FILE [--version N] [--thumb-out PNG]\n\
         file version ITEM FILE [--thumb PNG]\n\
         versions ITEM | restore ITEM N      ITEM = id (prefix ok), path or title"
    );
    std::process::exit(2)
}

struct Cli {
    args: Vec<String>,
}

impl Cli {
    fn flag(&mut self, name: &str) -> Option<String> {
        let i = self.args.iter().position(|a| a == name)?;
        let v = self.args.get(i + 1).cloned();
        self.args.drain(i..(i + 2).min(self.args.len()));
        v
    }
    fn switch(&mut self, name: &str) -> bool {
        match self.args.iter().position(|a| a == name) {
            Some(i) => {
                self.args.remove(i);
                true
            }
            None => false,
        }
    }
}

fn open(home: &Path) -> Arc<RodaEngine> {
    std::fs::create_dir_all(home).expect("home dir");
    let db = home.join("zoen.sqlite");
    match RodaEngine::open(
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

fn die(msg: impl std::fmt::Display) -> ! {
    eprintln!("error: {msg}");
    std::process::exit(1)
}

/// Resolves `@handle`, a title, or a space id (prefix ok) to a space id.
fn chat(e: &RodaEngine, q: &str) -> String {
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
fn item(e: &RodaEngine, q: &str) -> roda_ffi::ItemDetail {
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
async fn ready(e: &RodaEngine, id: &str, timeout: u64) -> bool {
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
    e: &RodaEngine,
    id: &str,
    f: impl FnOnce(&mut Vec<roda_ffi::PageBlockDto>) -> Vec<String>,
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
    let vault: Arc<dyn SecretVault> = Arc::new(FileVault {
        dir: home.join("vault"),
    });
    let e = open(&home);

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

    if !e.unlock(vault.clone()).unwrap_or_else(|err| die(err)) {
        die(format!(
            "no account in {} (run `zoen init`)",
            home.display()
        ));
    }
    let me = e.account().map(|a| a.identity_id).unwrap_or_default();
    let watching = cmd == "watch";
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
        let c = e.wait_until_idle(timeout).await;
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
            e.wait_until_idle(timeout).await;
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
            match e.group_keys(space) {
                Some(k) => println!(
                    "epoch={}\tdigest={}\tmembers={}",
                    k.epoch,
                    k.digest,
                    k.members.len()
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
        "join" => {
            let code = cli.args.first().cloned().unwrap_or_else(|| usage());
            let space = e.join_invite(code).await.unwrap_or_else(|err| die(err));
            e.wait_until_idle(timeout).await;
            println!("{space}");
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
            let bg = roda_ffi::BackgroundDto {
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
        "pages" | "files" => {
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            let kind = if cmd == "pages" { "page" } else { "file" };
            for i in e
                .items()
                .into_iter()
                .filter(|i| i.space_id == space && i.kind_id == kind)
            {
                let extra = match &i.file {
                    Some(f) => format!("\t{} bytes\t{}/{} here", f.bytes, f.chunks_here, f.chunks),
                    None => String::new(),
                };
                println!("{}\t{}\tv{}\t{}{}", i.id, i.path, i.version, i.title, extra);
            }
        }
        "page" => {
            if cli.args.is_empty() {
                usage();
            }
            let sub = cli.args.remove(0);
            match sub.as_str() {
                "import" => {
                    let root = cli.flag("--root").map(PathBuf::from);
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let space = chat(&e, &cli.args[0]);
                    let mut files = Vec::new();
                    for f in &cli.args[1..] {
                        let bytes = read_file(f);
                        let md = String::from_utf8(bytes)
                            .unwrap_or_else(|_| die(format!("{f}: not UTF-8 text")));
                        let fp = PathBuf::from(f);
                        let path = match &root {
                            Some(r) => fp
                                .strip_prefix(r)
                                .unwrap_or(&fp)
                                .to_string_lossy()
                                .into_owned(),
                            None => fp
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        };
                        files.push(MarkdownFileDto { path, markdown: md });
                    }
                    let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
                    let items = e
                        .pages_import_markdown(space.clone(), files)
                        .unwrap_or_else(|err| die(format!("import: {err}")));
                    for (d, path) in items.iter().zip(&paths) {
                        println!("{}\t{}", d.id, path);
                    }
                    e.wait_until_idle(timeout).await;
                }
                "new" => {
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let space = chat(&e, &cli.args[0]);
                    let d = e
                        .page_create(space, cli.args[1..].join(" "))
                        .unwrap_or_else(|err| die(err));
                    e.wait_until_idle(timeout).await;
                    println!("{}", d.id);
                }
                "export" => {
                    let out = cli.flag("--out").unwrap_or_else(|| usage());
                    let it = item(
                        &e,
                        cli.args
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| usage()),
                    );
                    if !ready(&e, &it.id, timeout).await {
                        die("the page is still arriving");
                    }
                    let md = e.page_markdown(it.id).unwrap_or_else(|err| die(err));
                    write_file(Path::new(&out), md.as_bytes());
                    println!("{} bytes", md.len());
                }
                "export-all" => {
                    let dir = PathBuf::from(cli.flag("--dir").unwrap_or_else(|| usage()));
                    let space = chat(
                        &e,
                        cli.args
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| usage()),
                    );
                    let (mut n, mut bytes, mut missing) = (0, 0, 0);
                    for i in e
                        .items()
                        .into_iter()
                        .filter(|i| i.space_id == space && i.kind_id == "page")
                    {
                        if !ready(&e, &i.id, timeout).await {
                            missing += 1;
                            continue;
                        }
                        let md = e.page_markdown(i.id.clone()).unwrap_or_else(|err| die(err));
                        let name = if i.path.is_empty() {
                            format!("{}.md", i.id)
                        } else {
                            i.path.clone()
                        };
                        write_file(&dir.join(name), md.as_bytes());
                        n += 1;
                        bytes += md.len();
                    }
                    println!("{n} pages, {bytes} bytes, {missing} still arriving");
                }
                "show" => {
                    let it = item(
                        &e,
                        cli.args
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| usage()),
                    );
                    let p = e.page(it.id).unwrap_or_else(|err| die(err));
                    println!(
                        "{} (v{}{})",
                        p.title,
                        p.version,
                        if p.unsaved { ", unsaved" } else { "" }
                    );
                    for (n, b) in p.blocks.iter().enumerate() {
                        let mark = match b.kind.as_str() {
                            "task" => {
                                if b.checked {
                                    "[x] "
                                } else {
                                    "[ ] "
                                }
                            }
                            _ => "",
                        };
                        println!(
                            "{}\t{}\t{mark}{}",
                            n + 1,
                            b.kind,
                            b.text.replace('\u{2028}', " / ")
                        );
                    }
                }
                "check" => {
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let it = item(&e, &cli.args[0]);
                    let n: usize = cli.args[1].parse().unwrap_or_else(|_| usage());
                    edit_page(&e, &it.id, |blocks| {
                        let tasks: Vec<usize> = (0..blocks.len())
                            .filter(|&i| blocks[i].kind == "task")
                            .collect();
                        let i = *tasks
                            .get(n.wrapping_sub(1))
                            .unwrap_or_else(|| die(format!("no task {n}")));
                        blocks[i].checked = !blocks[i].checked;
                        vec![blocks[i].id.clone()]
                    });
                    e.wait_until_idle(timeout).await;
                }
                "add" => {
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let it = item(&e, &cli.args[0]);
                    let text = cli.args[1..].join(" ");
                    edit_page(&e, &it.id, |blocks| {
                        let id = new_block_id();
                        blocks.push(roda_ffi::PageBlockDto {
                            id: id.clone(),
                            kind: "paragraph".into(),
                            level: 0,
                            indent: 0,
                            number: 0,
                            checked: false,
                            lang: String::new(),
                            url: String::new(),
                            alt: String::new(),
                            text,
                            spans: vec![],
                        });
                        vec![id]
                    });
                    e.wait_until_idle(timeout).await;
                }
                "set" => {
                    if cli.args.len() < 3 {
                        usage();
                    }
                    let it = item(&e, &cli.args[0]);
                    let n: usize = cli.args[1].parse().unwrap_or_else(|_| usage());
                    let text = cli.args[2..].join(" ");
                    edit_page(&e, &it.id, |blocks| {
                        let b = blocks
                            .get_mut(n.wrapping_sub(1))
                            .unwrap_or_else(|| die(format!("no block {n}")));
                        b.text = text;
                        b.spans.clear();
                        vec![b.id.clone()]
                    });
                    e.wait_until_idle(timeout).await;
                }
                _ => usage(),
            }
        }
        "file" => {
            if cli.args.is_empty() {
                usage();
            }
            let sub = cli.args.remove(0);
            let thumb = cli.flag("--thumb").map(|p| read_file(&p));
            match sub.as_str() {
                "add" => {
                    let dir = cli.flag("--path").unwrap_or_default();
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let space = chat(&e, &cli.args[0]);
                    let f = &cli.args[1];
                    let name = Path::new(f)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| f.clone());
                    let path = if dir.is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{name}", dir.trim_end_matches('/'))
                    };
                    let d = e
                        .file_add(space, path, name, mime_of(f).into(), read_file(f), thumb)
                        .unwrap_or_else(|err| die(err));
                    e.wait_until_idle(timeout).await;
                    let fi = d.file.unwrap_or_else(|| die("not a file"));
                    println!("{}\t{} bytes\t{} pieces", d.id, fi.bytes, fi.chunks);
                }
                "version" => {
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let it = item(&e, &cli.args[0]);
                    let d = e
                        .file_new_version(it.id, read_file(&cli.args[1]), thumb, String::new())
                        .unwrap_or_else(|err| die(err));
                    e.wait_until_idle(timeout).await;
                    println!("v{}", d.version);
                }
                "get" => {
                    let out = cli.flag("--out").unwrap_or_else(|| usage());
                    let version: Option<u32> = cli.flag("--version").and_then(|v| v.parse().ok());
                    let thumb_out = cli.flag("--thumb-out");
                    let it = item(
                        &e,
                        cli.args
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| usage()),
                    );
                    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
                    let bytes = loop {
                        if let Some(b) = e
                            .file_bytes(it.id.clone(), version)
                            .unwrap_or_else(|err| die(err))
                        {
                            break b;
                        }
                        if tokio::time::Instant::now() >= deadline {
                            die("the file is still arriving");
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    };
                    write_file(Path::new(&out), &bytes);
                    if let Some(t) = thumb_out {
                        let tb = e
                            .file_thumbnail(it.id.clone())
                            .unwrap_or_else(|err| die(err))
                            .unwrap_or_else(|| die("no preview"));
                        write_file(Path::new(&t), &tb);
                    }
                    println!("{} bytes", bytes.len());
                }
                _ => usage(),
            }
        }
        "versions" => {
            let it = item(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            for v in &it.versions {
                println!("v{}\t{}\t{}", v.number, v.author.name, v.note);
            }
        }
        "restore" => {
            if cli.args.len() < 2 {
                usage();
            }
            let it = item(&e, &cli.args[0]);
            let n: u32 = cli.args[1].parse().unwrap_or_else(|_| usage());
            let d = e.restore_version(it.id, n).unwrap_or_else(|err| die(err));
            e.wait_until_idle(timeout).await;
            println!("v{}", d.version);
        }
        _ => usage(),
    }
    e.stop_sync();
}

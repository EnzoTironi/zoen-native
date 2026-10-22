//! Terminal output keeps its original weak Client observation handle.
use super::*;

/// Prints a history transfer's progress.
pub(crate) struct Progress;

impl client_core::TransferListener for Progress {
    fn on_progress(&self, done: u32, total: u32) {
        println!("history {done}/{total}");
    }
}

/// Takes the first history bundle from the device that linked this one.
pub(crate) async fn receive_history(e: &Client, timeout: u64) {
    match e
        .receive_history(timeout.max(30_000), Some(Arc::new(Progress)))
        .await
    {
        Ok(h) => println!(
            "history: {} messages in {} chunks ({} resumed)",
            h.messages, h.chunks, h.resumed
        ),
        Err(err) => die(err),
    }
}

pub(crate) struct Printer {
    pub(crate) me: String,
    pub(crate) engine: std::sync::Weak<Client>,
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
    fn on_connection(&self, status: ConnectionDto) {
        if std::env::var_os("ZOEN_NET_DEBUG").is_some() {
            eprintln!(
                "[zoen-net] connection={} synced={} pending={} error={:?}",
                status.state, status.synced, status.pending, status.error
            );
        }
    }
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

pub(crate) fn title_of(e: &Client, space: &str) -> String {
    e.space(space.to_string())
        .map(|s| s.title)
        .unwrap_or_else(|_| space.chars().take(10).collect())
}

pub(crate) fn line(t: &client_core::TimelineEntry) -> String {
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

//! Families share clones of the same Client/vault handles; no additional store,
//! projection, sync task or authority is created. Host guards stay in main.
use crate::*;
mod account;
mod conversations;
mod documents;
mod operations;

pub(crate) struct Context {
    pub e: Arc<Client>,
    pub vault: Arc<dyn SecretVault>,
    pub me: String,
    pub timeout: u64,
    pub offline: bool,
    pub for_secs: Option<u64>,
}

pub(crate) async fn run(cmd: String, cli: &mut Cli, context: Context) {
    match cmd.as_str() {
        "whoami" | "status" | "people" | "dm" | "group" | "send" | "read" | "add" | "remove"
        | "encrypt" | "keys" | "chats" | "invite" | "agent" | "join" | "typing" => {
            conversations::run(cmd, cli, context).await
        }
        "backup" | "link" | "history" | "devices" | "unlink" | "background" | "photo"
        | "profile" | "block" | "unblock" => account::run(cmd, cli, context).await,
        "app" | "growth-sync" | "variant" | "sync" | "verify" | "authorize-agent"
        | "agent-pause" | "agent-resume" | "agent-revoke" | "agent-define" | "watch" => {
            operations::run(cmd, cli, context).await
        }
        "pages" | "files" | "page" | "file" | "versions" | "restore" => {
            documents::run(cmd, cli, context).await
        }
        _ => usage(),
    }
}

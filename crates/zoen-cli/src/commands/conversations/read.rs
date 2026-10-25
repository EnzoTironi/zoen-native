//! Existing conversation DTOs at the terminal; no new authority or storage.
use crate::{Cli, Client};
use client_core::MemberRoleDto;

#[derive(Default)]
pub(super) struct Options {
    pub json: bool,
    pub peek: bool,
    pub older: bool,
}
impl Options {
    pub fn parse(cli: &mut Cli) -> Result<Self, String> {
        let options = Self {
            json: cli.switch("--json"),
            peek: cli.switch("--peek"),
            older: cli.switch("--older"),
        };
        if cli.args.len() != 1 || cli.args[0].starts_with("--") {
            return Err("read expects CHAT [--json] [--peek] [--older]".into());
        }
        if options.peek && options.older {
            return Err("--peek cannot request an older history page".into());
        }
        Ok(options)
    }
}

pub(super) fn members(client: &Client, space: &str) -> Result<Vec<MemberRoleDto>, String> {
    let account = client
        .account()
        .filter(|a| a.unlocked && !a.setup_pending)
        .ok_or("This conversation requires an unlocked account")?;
    let roles = client
        .member_roles(space.into())
        .map_err(|e| e.to_string())?;
    if !roles.iter().any(|r| r.identity_id == account.identity_id) {
        return Err("This account is not a current member of this conversation".into());
    }
    Ok(roles)
}

pub(super) fn members_json(client: &Client, space: &str) -> Result<String, String> {
    let roles = members(client, space)?;
    serde_json::to_string(&serde_json::json!({"space_id":space,"members":roles}))
        .map_err(|e| e.to_string())
}

pub(super) fn keys_json(client: &Client, space: &str) -> Result<String, String> {
    members(client, space)?;
    let group = client
        .group_keys(space.into())
        .ok_or("No group keys for that chat on this device")?;
    // The existing DTO contains only epoch/digest/member IDs; devices are public
    // leaf locators. No private MLS/device material crosses this projection.
    let devices = client.group_devices(space.into());
    serde_json::to_string(&serde_json::json!({"space_id":space,"group":group,"devices":devices}))
        .map_err(|e| e.to_string())
}

pub(super) async fn read(client: &Client, space: &str, options: Options) -> Result<String, String> {
    if options.json || options.peek {
        members(client, space)?;
    }
    let mut prefix = String::new();
    if options.older {
        let wait = std::env::var("ZOEN_PAGE_TIMEOUT_MS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(8000);
        let older = client
            .load_older(space.into(), wait)
            .await
            .map_err(|e| e.to_string())?;
        if !options.json {
            prefix = match older.message {
                Some(message) => format!("({message})\n"),
                None => format!(
                    "(older: {} more{})\n",
                    older.loaded,
                    if older.more { ", still more" } else { "" }
                ),
            };
        }
    }
    // Client::timeline includes thread messages; the JSON app timeline page has
    // different presentation filtering and is deliberately not used here.
    let entries = client.timeline(space.into()).map_err(|e| e.to_string())?;
    let output = if options.json {
        serde_json::to_string(&entries).map_err(|e| e.to_string())?
    } else {
        prefix
            + &entries
                .iter()
                .map(crate::line)
                .collect::<Vec<_>>()
                .join("\n")
    };
    if !options.peek {
        let _ = client.mark_read(space.into());
    }
    Ok(output)
}

#[cfg(test)]
mod tests;

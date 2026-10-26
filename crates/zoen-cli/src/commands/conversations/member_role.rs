//! Public reviewed membership, using the same context/choice as the native sheet.
use super::*;
use client_core::{MemberActionState, SpaceMemberContext};
use tokio::time::Instant;

struct Options {
    chat: String,
    person: String,
    desired: Option<String>,
    json: bool,
}
impl Options {
    fn parse(cli: &mut Cli) -> Result<Self, String> {
        let json = cli.switch("--json");
        if !(2..=3).contains(&cli.args.len())
            || cli.args.iter().any(|value| value.starts_with("--"))
        {
            return Err("member-role expects CHAT PERSON [member|reader|admin] [--json]".into());
        }
        let desired = cli.args.get(2).cloned();
        if desired
            .as_deref()
            .is_some_and(|role| !matches!(role, "member" | "reader" | "admin"))
        {
            return Err("member role must be member, reader or admin; owner is protected".into());
        }
        Ok(Self {
            chat: cli.args[0].clone(),
            person: cli.args[1].clone(),
            desired,
            json,
        })
    }
}

pub(super) async fn run(
    client: &Client,
    cli: &mut Cli,
    offline: bool,
    deadline: Instant,
) -> Result<(), String> {
    let options = Options::parse(cli)?;
    let account = client
        .account()
        .filter(|account| account.unlocked && !account.setup_pending)
        .ok_or("member roles require an unlocked account")?;
    let space = chat(client, &options.chat);
    let target = if options.person.len() == 64
        && options
            .person
            .bytes()
            .all(|value| value.is_ascii_hexdigit())
    {
        options.person
    } else {
        person(client, &options.person).await
    };
    let context = || {
        client
            .space_member_context(
                space.clone(),
                target.clone(),
                account.identity_id.clone(),
                account.device_id.clone(),
            )
            .map_err(|error| error.to_string())
    };
    let before = context()?;
    let requested = options.desired.clone();
    let result = match options.desired {
        None => before,
        Some(role)
            if (before.role.as_deref() == Some(role.as_str())
                && before.state == MemberActionState::Active)
                || before.pending_role.as_deref() == Some(role.as_str()) =>
        {
            before
        }
        Some(role) => {
            let choice = before
                .role_options
                .iter()
                .find(|choice| choice.role == role)
                .ok_or("this role is not currently assignable to this member")?;
            client
                .set_member_role_reviewed(
                    space.clone(),
                    target.clone(),
                    account.identity_id.clone(),
                    account.device_id.clone(),
                    choice.revision.clone(),
                    role,
                )
                .map_err(|error| error.to_string())?;
            if !offline {
                client
                    .wait_until_idle(super::super::sync_budget::remaining_ms(deadline))
                    .await;
            }
            context()?
        }
    };
    show(&result, options.json)?;
    if requested.is_some() && result.role_change_failed && result.role != requested {
        return Err("member role assignment was refused; the confirmed role is unchanged".into());
    }
    Ok(())
}

fn show(context: &SpaceMemberContext, json: bool) -> Result<(), String> {
    if json {
        println!(
            "{}",
            serde_json::to_string(context).map_err(|error| error.to_string())?
        );
    } else {
        println!(
            "{}\trole={}\tstate={:?}{}{}",
            context.member_id,
            context.role.as_deref().unwrap_or("absent"),
            context.state,
            context
                .pending_role
                .as_ref()
                .map_or_else(String::new, |role| format!("\tpending={role}")),
            if context.role_change_failed {
                "\trefused"
            } else {
                ""
            }
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn explicit_query_and_only_supported_roles_are_public() {
        for args in [
            vec!["room", "@person"],
            vec!["room", "@person", "reader"],
            vec!["room", "@person", "member", "--json"],
            vec!["room", "@person", "admin"],
        ] {
            assert!(Options::parse(&mut Cli {
                args: args.into_iter().map(String::from).collect()
            })
            .is_ok());
        }
        for args in [
            vec!["room"],
            vec!["room", "@person", "owner"],
            vec!["room", "@person", "Reader"],
            vec!["room", "@person", "--admin"],
            vec!["room", "@person", "member", "extra"],
        ] {
            assert!(Options::parse(&mut Cli {
                args: args.into_iter().map(String::from).collect()
            })
            .is_err());
        }
    }
}

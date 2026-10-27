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
    fn parse(cli: &mut Cli, adding: bool) -> Result<Self, String> {
        let json = cli.switch("--json");
        let admin = adding && cli.switch("--admin");
        let explicit_role = if adding && cli.args.iter().any(|arg| arg == "--role") {
            Some(
                cli.flag("--role")
                    .filter(|role| !role.starts_with("--"))
                    .ok_or("--role requires member, reader or admin")?,
            )
        } else {
            None
        };
        if adding && (cli.args.len() != 2 || (admin && explicit_role.is_some())) {
            return Err("add expects CHAT PERSON [--role member|reader|admin] [--json]; --admin is an alternative to --role".into());
        }
        if !(2..=3).contains(&cli.args.len())
            || cli.args.iter().any(|value| value.starts_with("--"))
        {
            return Err("member-role expects CHAT PERSON [member|reader|admin] [--json]".into());
        }
        let desired = if adding {
            Some(explicit_role.unwrap_or_else(|| if admin { "admin" } else { "member" }.into()))
        } else {
            cli.args.get(2).cloned()
        };
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
    adding: bool,
) -> Result<(), String> {
    let options = Options::parse(cli, adding)?;
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
    if options.desired.is_some() {
        if adding && before.role.is_some() && before.role != options.desired {
            return Err(
                "this person is already a member; use member-role to change their role".into(),
            );
        }
        if !adding && before.role.is_none() && before.pending_role.is_none() {
            return Err(
                "this person is absent; use add --role to choose their initial access".into(),
            );
        }
    }
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
    if requested.is_some()
        && (result.role_change_failed || result.addition_failed)
        && result.role != requested
    {
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
            if context.role_change_failed || context.addition_failed {
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
            assert!(Options::parse(
                &mut Cli {
                    args: args.into_iter().map(String::from).collect()
                },
                false
            )
            .is_ok());
        }
        for args in [
            vec!["room"],
            vec!["room", "@person", "owner"],
            vec!["room", "@person", "Reader"],
            vec!["room", "@person", "--admin"],
            vec!["room", "@person", "member", "extra"],
        ] {
            assert!(Options::parse(
                &mut Cli {
                    args: args.into_iter().map(String::from).collect()
                },
                false
            )
            .is_err());
        }
    }
    #[test]
    fn addition_chooses_the_initial_role_and_rejects_ambiguous_or_incomplete_flags() {
        for (args, desired) in [
            (vec!["room", "@person"], "member"),
            (
                vec!["room", "@person", "--role", "reader", "--json"],
                "reader",
            ),
            (vec!["room", "@person", "--role", "admin"], "admin"),
            (vec!["room", "@person", "--admin"], "admin"),
        ] {
            let parsed = Options::parse(
                &mut Cli {
                    args: args.into_iter().map(String::from).collect(),
                },
                true,
            )
            .expect("supported public addition");
            assert_eq!(parsed.desired.as_deref(), Some(desired));
        }
        for args in [
            vec!["room", "@person", "--role"],
            vec!["room", "@person", "--role", "owner"],
            vec!["room", "@person", "--role", "reader", "--admin"],
            vec!["room", "@person", "reader"],
            vec!["room", "@person", "--role", "--json"],
        ] {
            assert!(Options::parse(
                &mut Cli {
                    args: args.into_iter().map(String::from).collect()
                },
                true
            )
            .is_err());
        }
    }
}

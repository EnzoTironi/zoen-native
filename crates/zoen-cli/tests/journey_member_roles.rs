//! Normal public commands, real relay and independent certified MLS identities.
mod common;
use common::World;
use serde_json::Value;

type TestResult<T> = Result<T, Box<dyn std::error::Error>>;
fn json(w: &World, who: &str, args: &[&str]) -> TestResult<Value> {
    Ok(serde_json::from_str(&w.zoen(who, args))?)
}
fn local_role_events(
    w: &World,
    who: &str,
    space: &str,
    table: &str,
) -> TestResult<Vec<types::Event>> {
    assert!(matches!(table, "events" | "outbox"));
    let db = rusqlite::Connection::open(w.dir.join(who).join("zoen.sqlite"))?;
    let mut query = db.prepare(&format!("SELECT json FROM {table} WHERE space=?1"))?;
    let mut events = Vec::new();
    for row in query.query_map([space], |row| row.get::<_, String>(0))? {
        let event: types::Event = serde_json::from_str(&row?)?;
        if matches!(event.body, types::EventBody::MemberAdded { .. }) {
            events.push(event);
        }
    }
    Ok(events)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn public_member_roles_preserve_owner_pending_receipts_readers_and_e2ee() -> TestResult<()> {
    for encrypted in [false, true] {
        let w = World::new(if encrypted {
            "member_roles_e2e"
        } else {
            "member_roles_closed"
        })
        .await;
        for (who, name) in [("ana", "Ana"), ("bruno", "Bruno"), ("claire", "Claire")] {
            w.init(who, name);
        }
        let mut args = vec!["group", "Role controls", "@bruno", "@claire"];
        if !encrypted {
            args.push("--readable");
        }
        let space = w.zoen("ana", &args).trim().to_owned();
        let first = json(&w, "ana", &["member-role", &space, "@bruno", "--json"])?;
        assert_eq!(first["role"], "member");
        let target = first["member_id"]
            .as_str()
            .ok_or("target missing")?
            .to_owned();
        let before_keys = if encrypted {
            Some(json(&w, "ana", &["keys", &space, "--json"])?["group"].clone())
        } else {
            None
        };
        let pending = json(
            &w,
            "ana",
            &[
                "--offline",
                "member-role",
                &space,
                "@bruno",
                "reader",
                "--json",
            ],
        )?;
        assert_eq!(pending["role"], "member");
        assert_eq!(pending["pending_role"], "reader");
        assert_eq!(pending["state"], "Updating");
        assert!(pending["role_options"]
            .as_array()
            .ok_or("options missing")?
            .is_empty());
        let original = local_role_events(&w, "ana", &space, "outbox")?;
        assert_eq!(original.len(), 1);
        event_log::verify_author(&original[0])?;
        let again = json(
            &w,
            "ana",
            &[
                "--offline",
                "member-role",
                &space,
                "@bruno",
                "reader",
                "--json",
            ],
        )?;
        assert_eq!(again["pending_role"], "reader");
        assert_eq!(
            local_role_events(&w, "ana", &space, "outbox")?,
            original,
            "new CLI process reading pending role must not re-sign"
        );
        let canonical = json(&w, "ana", &["--offline", "members", &space, "--json"])?;
        assert!(canonical["members"]
            .as_array()
            .ok_or("members missing")?
            .iter()
            .any(|member| member["identity_id"] == target && member["role"] == "member"));
        w.zoen("ana", &["sync"]);
        let installed = json(&w, "ana", &["member-role", &space, "@bruno", "--json"])?;
        assert_eq!(installed["role"], "reader");
        assert_eq!(installed["state"], "Active");
        assert!(installed["pending_role"].is_null());
        let stored = local_role_events(&w, "ana", &space, "events")?;
        let replay = json(
            &w,
            "ana",
            &["member-role", &space, "@bruno", "reader", "--json"],
        )?;
        assert_eq!(replay["role"], "reader");
        assert_eq!(local_role_events(&w, "ana", &space, "events")?, stored);
        let reader = json(&w, "bruno", &["member-role", &space, "@bruno", "--json"])?;
        assert_eq!(reader["role"], "reader");
        assert!(reader["role_options"]
            .as_array()
            .ok_or("reader options missing")?
            .is_empty());
        for command in [
            vec!["send", space.as_str(), "Reader must not send"],
            vec!["page", "new", space.as_str(), "Reader must not create"],
        ] {
            assert!(w.try_zoen("bruno", &command).is_err());
        }
        assert!(local_role_events(&w, "bruno", &space, "outbox")?.is_empty());
        w.zoen(
            "ana",
            &["send", &space, "Readers still receive this actual message"],
        );
        assert!(w
            .zoen("bruno", &["read", &space, "--peek"])
            .contains("Readers still receive this actual message"));
        if let Some(before) = before_keys {
            let owner = json(&w, "ana", &["keys", &space, "--json"])?;
            let peer = json(&w, "bruno", &["keys", &space, "--json"])?;
            assert_eq!(owner["group"], peer["group"]);
            assert_eq!(
                owner["group"]["epoch"], before["epoch"],
                "role replacement does not remove a reader's MLS leaf"
            );
        }
        let admin = json(
            &w,
            "ana",
            &["member-role", &space, "@bruno", "admin", "--json"],
        )?;
        assert_eq!(admin["role"], "admin");
        let managed = json(
            &w,
            "bruno",
            &["member-role", &space, "@claire", "reader", "--json"],
        )?;
        assert_eq!(managed["role"], "reader");
        let event_count = local_role_events(&w, "bruno", &space, "events")?.len();
        assert!(w
            .try_zoen(
                "bruno",
                &["member-role", &space, "@ana", "member", "--json"]
            )
            .is_err());
        assert!(
            w.try_zoen("bruno", &["add", &space, "@ana", "--admin"])
                .is_err(),
            "legacy API cannot bypass protected owner"
        );
        assert_eq!(
            local_role_events(&w, "bruno", &space, "events")?.len(),
            event_count
        );
        assert!(local_role_events(&w, "bruno", &space, "outbox")?.is_empty());
        assert_eq!(
            json(&w, "ana", &["member-role", &space, "@ana", "--json"])?["role"],
            "owner"
        );
        assert_eq!(
            json(
                &w,
                "ana",
                &["member-role", &space, "@bruno", "member", "--json"]
            )?["role"],
            "member"
        );
        assert_eq!(
            json(&w, "bruno", &["member-role", &space, "@bruno", "--json"])?["role"],
            "member"
        );
    }
    println!("member roles: public CLI/context -> one pending signed role -> cold canonical receipt -> Reader read/no-send/no-Page -> existing MLS leaves -> allowed Admin assignment/protected Owner PASS");
    Ok(())
}

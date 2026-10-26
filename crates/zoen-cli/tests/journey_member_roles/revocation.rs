//! A queued real public review cannot silently turn into a new admission.
use super::*;
use common::RawClient;
use event_log::{Author, Signer};
use protocol::Envelope;
use types::{EventBody, Role};

fn author(w: &World, who: &str) -> TestResult<Author> {
    let key = |name: &str| -> TestResult<Signer> {
        let bytes: [u8; 32] = std::fs::read(w.dir.join(who).join("vault").join(name))?
            .try_into()
            .map_err(|_| "synthetic fixture key length")?;
        Ok(Signer::from_secret(&bytes))
    };
    Ok(Author::device(
        &key("zoen.identity.v1")?,
        key("zoen.device.v1")?,
    ))
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn removed_members_require_new_inclusion_even_when_an_old_role_review_arrives_later(
) -> TestResult<()> {
    let w = World::new("member_role_revocation").await;
    for (who, name) in [("ana", "Ana"), ("bruno", "Bruno"), ("claire", "Claire")] {
        w.init(who, name);
    }
    let space = w
        .zoen(
            "ana",
            &["group", "Revocation", "@bruno", "@claire", "--readable"],
        )
        .trim()
        .to_owned();
    w.zoen("ana", &["member-role", &space, "@bruno", "admin"]);
    let queued = json(
        &w,
        "ana",
        &[
            "--offline",
            "member-role",
            &space,
            "@claire",
            "admin",
            "--json",
        ],
    )?;
    assert_eq!(queued["role"], "member");
    assert_eq!(queued["pending_role"], "admin");
    let original = local_role_events(&w, "ana", &space, "outbox")?
        .into_iter()
        .next()
        .ok_or("role review missing")?;
    event_log::verify_author(&original)?;
    assert!(matches!(
        original.body,
        EventBody::MemberAdded {
            role: Role::Admin,
            ..
        }
    ));
    let target = queued["member_id"].as_str().ok_or("Claire missing")?;
    w.zoen("bruno", &["remove", &space, "@claire"]);
    let mut owner = RawClient::reconnect(&w.relay_url(), author(&w, "ana")?).await;
    let refused = owner.publish_env(Envelope::plain(&original)?).await;
    assert!(
        refused.is_err(),
        "a delayed change must not readmit the removed person; accepted seq={:?}",
        refused.as_ref().ok().map(|event| event.seq)
    );
    assert!(refused
        .err()
        .ok_or("missing refusal")?
        .contains("membership review"));
    let members = json(&w, "bruno", &["members", &space, "--json"])?;
    assert!(!members["members"]
        .as_array()
        .ok_or("members missing")?
        .iter()
        .any(|member| member["identity_id"] == target));
    drop(owner);
    let mut owner = RawClient::reconnect(&w.relay_url(), author(&w, "ana")?).await;
    assert!(
        owner
            .publish_env(Envelope::plain(&original)?)
            .await
            .is_err(),
        "cold socket cannot bypass revocation"
    );
    // A new explicit inclusion observes the removal and grants exactly Reader.
    let joined = json(
        &w,
        "bruno",
        &["add", &space, "@claire", "--role", "reader", "--json"],
    )?;
    assert_eq!(joined["role"], "reader");
    assert!(
        owner
            .publish_env(Envelope::plain(&original)?)
            .await
            .is_err(),
        "remove/rejoin ABA cannot revive an old Admin grant"
    );
    assert_eq!(
        json(&w, "claire", &["member-role", &space, "@claire", "--json"])?["role"],
        "reader"
    );
    assert!(w
        .try_zoen("claire", &["send", &space, "not an admin"])
        .is_err());
    // Normal CLI consumes the old refusal; a fresh role review is a distinct act.
    assert_eq!(
        json(&w, "ana", &["member-role", &space, "@claire", "--json"])?["role"],
        "reader"
    );
    let changed = json(
        &w,
        "ana",
        &[
            "--offline",
            "member-role",
            &space,
            "@claire",
            "member",
            "--json",
        ],
    )?;
    assert_eq!(changed["pending_role"], "member");
    w.zoen(
        "bruno",
        &[
            "send",
            &space,
            "Unrelated chat must preserve a valid role review",
        ],
    );
    w.zoen("ana", &["sync"]);
    assert_eq!(
        json(&w, "claire", &["member-role", &space, "@claire", "--json"])?["role"],
        "member"
    );
    // The author's old membership generation is fenced too, even when Admin
    // was explicitly granted again after removal.
    let pending = json(
        &w,
        "bruno",
        &[
            "--offline",
            "member-role",
            &space,
            "@claire",
            "admin",
            "--json",
        ],
    )?;
    assert_eq!(pending["pending_role"], "admin");
    let old_admin = local_role_events(&w, "bruno", &space, "outbox")?.into_iter().find(|event|
        matches!(&event.body, EventBody::MemberAdded { identity, role: Role::Admin } if identity == target))
        .ok_or("admin's old role intent missing")?;
    w.zoen("ana", &["remove", &space, "@bruno"]);
    let restored = json(
        &w,
        "ana",
        &["add", &space, "@bruno", "--role", "admin", "--json"],
    )?;
    assert_eq!(restored["role"], "admin");
    let mut returning_admin = RawClient::reconnect(&w.relay_url(), author(&w, "bruno")?).await;
    assert!(
        returning_admin
            .publish_env(Envelope::plain(&old_admin)?)
            .await
            .is_err(),
        "restored administration cannot revive a pre-removal signature"
    );
    assert_eq!(
        json(&w, "claire", &["member-role", &space, "@claire", "--json"])?["role"],
        "member"
    );
    assert_eq!(
        json(
            &w,
            "bruno",
            &["member-role", &space, "@claire", "reader", "--json"]
        )?["role"],
        "reader",
        "a new review in the restored generation is allowed"
    );
    Ok(())
}

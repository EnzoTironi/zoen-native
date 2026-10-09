//! Milestone 2 journey: an end-to-end group on the real relay, Postgres and FoundationDB,
//! where the relay holds only ciphertext (ADR 0026). Every `zoen` call is a fresh process,
//! so every step is also a kill-and-relaunch of that device.
//!
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:55432/postgres \
//!   eval "$(scripts/fdb.sh env)" && cargo test -p zoen-cli --test journey_m2

mod common;
use common::*;
use roda_types::EventBody;
use sqlx::Connection;

/// Every row of every table in the relay's database, as text, with bytea as hex.
async fn postgres_text(w: &World) -> String {
    let tables: Vec<String> = {
        let mut c = sqlx::postgres::PgConnection::connect(&w.db_url)
            .await
            .unwrap();
        sqlx::query_scalar(
            "SELECT table_name::text FROM information_schema.tables WHERE table_schema = 'public'",
        )
        .fetch_all(&mut c)
        .await
        .unwrap()
    };
    let mut all = String::new();
    for t in tables {
        all += &w
            .scalar(&format!(
                "SELECT coalesce(string_agg(row_to_json(t)::text, '\n'), '') FROM \"{t}\" t"
            ))
            .await;
    }
    all
}

fn keys(out: &str) -> (u64, String) {
    let mut epoch = None;
    let mut digest = None;
    for field in out.trim().split('\t') {
        match field.split_once('=') {
            Some(("epoch", v)) => epoch = v.parse().ok(),
            Some(("digest", v)) => digest = Some(v.to_string()),
            _ => {}
        }
    }
    (epoch.expect("epoch"), digest.expect("digest"))
}

#[tokio::test]
async fn an_end_to_end_group_leaves_the_relay_only_ciphertext() {
    let w = World::new("m2").await;
    for (h, n) in [("ana", "Ana"), ("bruno", "Bruno"), ("carla", "Carla")] {
        w.init(h, n);
    }
    // Each device published its key packages when it signed up: 32 plus a last-resort one.
    assert_eq!(w.count("SELECT count(*) FROM key_packages").await, 3 * 33);

    let space = w.zoen("ana", &["group", "Cofre", "@bruno", "--e2e"]);
    let space = space.trim();
    // Ana's device claimed one of Bruno's packages and committed him into the group.
    assert_eq!(
        w.count("SELECT count(*) FROM key_packages WHERE NOT last_resort")
            .await,
        3 * 32 - 1
    );

    let secret = "a senha do cofre é tucano-4512";
    let reply = "anotado, só nós dois lemos isso";
    w.zoen("ana", &["send", "Cofre", secret]);
    assert_eq!(
        w.zoen("bruno", &["read", "Cofre"]).trim(),
        format!("Ana: {secret}")
    );
    w.zoen("bruno", &["send", "Cofre", reply]);
    assert_eq!(
        w.zoen("ana", &["read", "Cofre"]).trim(),
        format!("Ana: {secret}\nBruno: {reply}")
    );
    // Carla isn't in it.
    assert!(!w.zoen("carla", &["chats"]).contains("Cofre"));

    // ── what the relay holds ──
    let stored = w.events_in(space).await;
    let mut sealed = 0;
    for ev in &stored {
        match ev.env.body() {
            None => sealed += 1,
            Some(
                EventBody::SpaceCreated { .. }
                | EventBody::MemberAdded { .. }
                | EventBody::ProfileKeyShared { .. }
                | EventBody::Checkpoint { .. },
            ) => {}
            Some(other) => panic!("the relay stored plaintext: {other:?}"),
        }
    }
    assert_eq!(sealed, 4, "a commit, a welcome and two messages");
    let needles = [secret, reply, "tucano-4512", "anotado"];
    for ev in &stored {
        let bytes = String::from_utf8_lossy(ev.env.content()).to_string();
        for n in needles {
            assert!(!bytes.contains(n), "FoundationDB holds {n:?} in the clear");
        }
    }
    let pg = postgres_text(&w).await;
    for n in needles {
        assert!(!pg.contains(n), "Postgres holds {n:?} in the clear");
        assert!(
            !pg.contains(&hex::encode(n)),
            "Postgres holds {n:?} as bytes"
        );
    }

    // A client writing plaintext into the group is refused.
    let mut mallory = RawClient::connect(&w.relay_url(), "mallory").await;
    let refused = mallory.publish_message(space, "texto aberto").await;
    assert!(refused.contains("end-to-end"), "{refused}");

    // ── members agree on the group ──
    let (ae, ad) = keys(&w.zoen("ana", &["keys", "Cofre"]));
    let (be, bd) = keys(&w.zoen("bruno", &["keys", "Cofre"]));
    assert_eq!((ae, &ad), (be, &bd), "one group");
    assert_eq!(ae, 1, "one commit so far");
    let checkpoints: Vec<(String, u64, String)> = stored
        .iter()
        .filter_map(|ev| match ev.env.body() {
            Some(EventBody::Checkpoint { epoch, digest, .. }) => {
                Some((ev.env.author().to_string(), epoch, digest))
            }
            _ => None,
        })
        .collect();
    assert_eq!(
        checkpoints.len(),
        2,
        "Ana after her commit, Bruno after joining"
    );
    assert!(checkpoints.iter().all(|(_, e, d)| *e == 1 && *d == ad));

    // Both devices re-verify every signature and the chain over ciphertext from disk.
    for who in ["ana", "bruno"] {
        let v = w.zoen(who, &["verify"]);
        assert!(!v.contains("BROKEN"), "{who}: {v}");
    }

    // ── a newcomer reads from their welcome on ──
    let code = w.zoen("ana", &["invite", "Cofre"]);
    let code = code.split('\t').next().unwrap().trim().to_string();
    w.zoen("carla", &["join", &format!("zoen://join/{code}")]);
    w.zoen("ana", &["sync"]); // Ana's device commits Carla in
    w.zoen("carla", &["sync"]);
    let later = "Carla chegou; daqui pra frente ela lê";
    w.zoen("bruno", &["send", "Cofre", later]);
    let carla = w.zoen("carla", &["read", "Cofre"]);
    assert_eq!(
        carla.trim(),
        format!("Bruno: {later}"),
        "nothing from before she joined"
    );
    let ana = w.zoen("ana", &["read", "Cofre"]);
    assert!(ana.contains(secret) && ana.contains(later), "{ana}");
    let (ce, cd) = keys(&w.zoen("carla", &["keys", "Cofre"]));
    let (be, bd) = keys(&w.zoen("bruno", &["keys", "Cofre"]));
    assert_eq!((ce, &cd), (be, &bd));
    assert_eq!(ce, 2);
    assert!(!w.zoen("carla", &["verify"]).contains("BROKEN"));
    let pg = postgres_text(&w).await;
    assert!(!pg.contains(later));
    for ev in w.events_in(space).await {
        assert!(!String::from_utf8_lossy(ev.env.content()).contains("daqui pra frente"));
    }
}

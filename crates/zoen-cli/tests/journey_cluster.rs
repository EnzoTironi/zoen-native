//! S5 journeys: two relay nodes on one Postgres directory and one FoundationDB cell. Ana is
//! connected to node A, Bruno to node B. The log is shared, so stored events would reach Bruno
//! on his next sync anyway; what only the bus can carry is the live path: typing (never
//! stored), presence, and an event pushed to a device that is already watching.
//!
//! Needs ZOEN_TEST_PG, FoundationDB and NATS:
//!   eval "$(scripts/fdb.sh env)"; eval "$(scripts/nats.sh env)"
//!   cargo test -p zoen-cli --test journey_cluster

mod common;
use common::*;
use std::{process::Child, thread::sleep, time::Duration};

fn watch_output(w: Child) -> String {
    let out = w.wait_with_output().expect("watch");
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn two_people_on_two_nodes(w: &mut World) -> u16 {
    let b = w.start_node();
    w.init("ana", "Ana");
    let out = w.zoen_at(
        b,
        "bruno",
        &["init", "--name", "Bruno", "--handle", "bruno"],
    );
    assert!(out.contains("registered"), "{out}");
    w.zoen("ana", &["dm", "@bruno", "oi Bruno"]);
    assert!(w
        .zoen_at(b, "bruno", &["read", "@ana"])
        .contains("Ana: oi Bruno"));
    b
}

#[tokio::test]
async fn live_traffic_crosses_nodes_through_nats() {
    let mut w = World::with_nats("cluster").await;
    let b = two_people_on_two_nodes(&mut w);

    let watcher = w.spawn_zoen_at(b, "bruno", &["watch", "--for", "7"]);
    sleep(Duration::from_millis(2500));
    w.zoen("ana", &["typing", "@bruno", "--for", "2"]);
    w.zoen("ana", &["send", "@bruno", "chegando pelo outro nó!"]);
    let out = watch_output(watcher);

    assert!(
        out.contains("· Ana is online"),
        "presence crosses nodes:\n{out}"
    );
    assert!(
        out.contains("[Ana] Ana is typing…"),
        "typing crosses nodes:\n{out}"
    );
    assert!(
        out.contains("[Ana] Ana: chegando pelo outro nó!"),
        "a live event crosses nodes:\n{out}"
    );
    let log = w.relay_log_text();
    assert_eq!(
        log.matches("bus=\"nats\"").count() + log.matches("bus=nats").count(),
        2,
        "{log}"
    );
    assert!(!w.zoen("ana", &["verify"]).contains("BROKEN"));
    assert!(!w.zoen_at(b, "bruno", &["verify"]).contains("BROKEN"));
}

/// The control: the same two nodes without a bus. Stored events still converge through the
/// shared log, but nothing live crosses. This is what proves the bus is the carrier above.
#[tokio::test]
async fn without_a_bus_nothing_live_crosses_nodes() {
    let mut w = World::new("nobus").await;
    let b = two_people_on_two_nodes(&mut w);

    let watcher = w.spawn_zoen_at(b, "bruno", &["watch", "--for", "6"]);
    sleep(Duration::from_millis(2500));
    w.zoen("ana", &["typing", "@bruno", "--for", "2"]);
    let out = watch_output(watcher);
    assert!(!out.contains("is typing"), "{out}");
    assert!(!out.contains("Ana is online"), "{out}");
    assert!(w
        .zoen_at(b, "bruno", &["read", "@ana"])
        .contains("Ana: oi Bruno"));
}

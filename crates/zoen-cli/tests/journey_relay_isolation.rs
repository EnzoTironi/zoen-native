//! Real relay startup ownership and cross-process port reservations across an outage.

mod common;

use common::{RawClient, World};
use std::process::Command;

#[tokio::test]
async fn a_competing_world_is_not_the_restarted_relays_readiness() {
    let mut own = World::with_env("startup_ownership", &[("LOG_FORMAT", "json")]).await;
    own.init("bruno", "Bruno");
    let original_identity = own.id_of("bruno").await;
    let port = own.port;
    own.stop_relay();

    let mut competing = World::new("startup_competitor").await;
    competing.init("bruno", "Another Bruno");
    let other_identity = competing.id_of("bruno").await;
    assert_ne!(original_identity, other_identity);
    competing.stop_relay();
    // Deliberately bypass the cooperative allocator to model a foreign service.
    // The original World still owns the lifetime reservation for this port.
    competing.port = port;
    competing.start_relay();
    let (_stranger, registration) = RawClient::connect_registering(&own.relay_url(), "bruno").await;
    assert_eq!(registration, Err("handle_taken".into()));

    // relay.log also contains the original child's successful listening record.
    // Neither that stale record nor the foreign listener may satisfy this restart.
    let error = own
        .try_start_relay()
        .expect_err("restart accepted a competing World's listener");
    assert!(
        error.contains("relay child") && error.contains("exited with"),
        "{error}"
    );
    assert!(error.contains("before listening"), "{error}");
    assert!(!error.contains("relay didn't start"), "{error}");
    assert_eq!(own.id_of("bruno").await, original_identity);
    assert_eq!(competing.id_of("bruno").await, other_identity);

    competing.stop_relay();
    own.start_relay();
    let status = own.zoen("bruno", &["sync"]);
    assert!(
        status.contains("connection=online") && status.contains("synced=true"),
        "{status}"
    );
    assert_eq!(own.id_of("bruno").await, original_identity);
}

fn reservation_process(port: u16, expected: &str) {
    let output = Command::new(std::env::current_exe().expect("journey executable"))
        .args([
            "--exact",
            "port_reservation_worker",
            "--ignored",
            "--nocapture",
        ])
        .env("ZOEN_ISOLATION_PORT", port.to_string())
        .env("ZOEN_ISOLATION_EXPECTED", expected)
        .output()
        .expect("spawn reservation worker");
    assert!(
        output.status.success(),
        "reservation worker ({expected}) failed: {}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stdout).contains("reservation world synced"),
        "worker did not finish its real relay/account journey"
    );
}

#[tokio::test]
async fn a_stopped_world_reserves_its_port_across_test_processes() {
    let mut original = World::new("port_lifetime").await;
    let port = original.port;
    original.stop_relay();

    // The socket is gone, but a different test process must reject this exact
    // allocator candidate and bring its own database online on another port.
    reservation_process(port, "different");
    original.start_relay();
    original.init("bruno", "Bruno");
    original.stop_relay();

    // Child wait precedes reservation release. The same candidate becomes usable
    // after dropping the World, without stale lockfiles blocking future journeys.
    drop(original);
    reservation_process(port, "same");
}

#[tokio::test]
#[ignore = "worker launched by the cross-process reservation journey"]
async fn port_reservation_worker() {
    let port = std::env::var("ZOEN_ISOLATION_PORT")
        .expect("worker candidate port")
        .parse::<u16>()
        .expect("numeric port");
    let expected = std::env::var("ZOEN_ISOLATION_EXPECTED").expect("worker expectation");
    let w = World::new_preferring_port("port_worker", port).await;
    match expected.as_str() {
        "different" => assert_ne!(w.port, port, "stopped World lost its reservation"),
        "same" => assert_eq!(w.port, port, "finished World retained its reservation"),
        _ => panic!("unknown reservation expectation"),
    }
    w.init("bruno", "Bruno");
    let status = w.zoen("bruno", &["sync"]);
    assert!(
        status.contains("connection=online") && status.contains("synced=true"),
        "{status}"
    );
    println!("reservation world synced");
}

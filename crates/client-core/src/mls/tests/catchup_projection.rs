//! The same incremental frontier must work after actual MLS opening and restart.
use super::*;
use crate::sync::replay_probe::{projection, Home};

#[test]
fn encrypted_ordinary_pages_are_single_pass_and_cold_reopen_retains_exact_source() {
    let home = Home(std::env::temp_dir().join(types::new_id("sealed-page-catchup")));
    std::fs::create_dir(&home.0).unwrap();
    let path = home.0.join("peer.sqlite");
    let (mut owner, _, _) = member(":memory:");
    let (mut peer, _, peer_secret) = member(path.to_str().unwrap());
    owner.net.profiles.supported = true;
    let space = "sp_encrypted_page_catchup";
    for frame in claim_group(&mut owner, space, std::slice::from_mut(&mut peer)) {
        assert_eq!(peer.ingest(frame), Ingest::Applied);
    }
    owner
        .send_message(space, "Establish the actual encrypted group")
        .unwrap();
    let clock =
        Some(protocol::KeyPackageClaimClock::from_server_ms(crate::engine::now_ms()).unwrap());
    let (_, targets, operation) = owner.mls_to_claim(clock).unwrap().unwrap();
    let records = claimed_records(std::slice::from_mut(&mut peer), &targets);
    owner.mls_claimed(space, &operation, Ok(records));
    relay_ready_to_peer(&mut owner, &mut peer);
    assert_eq!(owner.outbox_len(), 0);
    assert_eq!(peer.outbox_len(), 0);
    assert!(peer.device().unwrap().has_group(space));
    crate::sync::replay_probe::reset();
    let mut expected = Vec::new();
    for index in 0..8 {
        let markdown = format!("# Encrypted page {index}\n\nExact CR\r\nUnicode 😀\n");
        let item = owner
            .page_import_markdown(space, &format!("page-{index}.md"), &markdown)
            .unwrap();
        expected.push((item, markdown));
    }
    let frames = relay_ready_to_peer(&mut owner, &mut peer);
    assert_eq!(frames.len(), 8);
    assert!(frames.iter().all(|frame| frame.env.is_sealed()));
    for (item, markdown) in &expected {
        assert_eq!(peer.page_markdown(item).unwrap(), *markdown);
    }
    let work = crate::sync::replay_probe::steps();
    let projected = projection(&peer);
    let cursor = peer.next_seq(space);
    let epoch = peer.device().unwrap().epoch(space).unwrap();
    assert!(peer.logs[space].verify().is_ok());
    drop(peer);
    let mut cold = Engine::open(path.to_str().unwrap()).unwrap();
    assert!(cold.unlock(Some(peer_secret.to_vec())).unwrap());
    assert_eq!(projection(&cold), projected);
    assert_eq!(cold.next_seq(space), cursor);
    assert_eq!(cold.device().unwrap().epoch(space).unwrap(), epoch);
    for (item, markdown) in expected {
        assert_eq!(cold.page_markdown(&item).unwrap(), markdown);
    }
    assert_eq!(cold.store.outbox_len().unwrap(), 0);
    assert!(cold.logs[space].verify().is_ok());
    assert_eq!(
        work, 0,
        "MLS application opening must not re-fold the old ordinary Page prefix"
    );
    eprintln!(
        "encrypted page catch-up work: pages=8 full_replay_steps={work} cold_projection_exact=true"
    );
}

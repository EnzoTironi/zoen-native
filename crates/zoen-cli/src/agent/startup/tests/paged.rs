use super::*;
use protocol::complete_reads::{Cursor, Device, Header, Kind, Page, Request};
pub(super) fn response(
    mode: Mode,
    r: Request,
    identity: &str,
    device: &str,
    digest: &mut Vec<u8>,
    remote: &Mutex<BTreeMap<String, u64>>,
) -> Result<Reply, String> {
    if r.cancel {
        return Ok(Reply::Done);
    }
    if r.ordinal == 0 {
        digest.clear();
    }
    if r.kind == Kind::Devices as i32 {
        let mut p = Page {
            kind: r.kind,
            snapshot: "c".repeat(64),
            ordinal: 0,
            identity: identity.into(),
            device: device.into(),
            header: None,
            devices: vec![Device {
                device: device.into(),
                revoked: false,
            }],
            cursors: Vec::new(),
            complete: true,
            count: 1,
            digest: Vec::new(),
        };
        p.digest = p.rolling_digest(&[]);
        return Ok(Reply::CompletePage(p));
    }
    if r.ordinal > 0 && matches!(mode, Mode::PageLost) {
        return Err("synthetic missing State terminal".into());
    }
    let rows = remote
        .lock()
        .unwrap()
        .clone()
        .into_iter()
        .collect::<Vec<_>>();
    assert!(rows.len() > 1);
    let cursors = if r.ordinal == 0 {
        rows[..1].to_vec()
    } else {
        rows[1..].to_vec()
    };
    let mut p = Page {
        kind: Kind::AgentState as i32,
        snapshot: "c".repeat(64),
        ordinal: r.ordinal,
        identity: if r.ordinal > 0 && matches!(mode, Mode::PageForeign) {
            "d".repeat(64)
        } else {
            identity.into()
        },
        device: device.into(),
        header: (r.ordinal == 0).then(|| Header {
            status: "active".into(),
            kind: "User".into(),
            definition_version: 1,
            definition: b"{}".to_vec(),
        }),
        devices: Vec::new(),
        cursors: cursors
            .into_iter()
            .map(|(space, seq)| Cursor { space, seq })
            .collect(),
        complete: r.ordinal > 0,
        count: if r.ordinal == 0 { 1 } else { rows.len() as u32 },
        digest: Vec::new(),
    };
    p.digest = p.rolling_digest(digest);
    *digest = p.digest.clone();
    Ok(Reply::CompletePage(p))
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn partial_or_foreign_state_never_seeds_any_omitted_space_or_publishes_readiness() {
    for mode in [Mode::PageLost, Mode::PageForeign] {
        let mut relay = relay(mode).await;
        let citizen = citizen(&relay.url);
        for n in 0..3 {
            let space = citizen
                .0
                .create_group_with(format!("Pending {n}"), vec![], PrivacyDto::Closed)
                .unwrap();
            citizen
                .0
                .send_message(
                    space.clone(),
                    "Unhandled source after the existing cursor".into(),
                )
                .unwrap();
            relay.cursors.lock().unwrap().insert(space, 0);
        }
        let before = relay.cursors.lock().unwrap().clone();
        let home = Home::new();
        let lock = HostLock::acquire(&home.0).unwrap();
        let readiness = Readiness::reset(&home.0, &lock).unwrap();
        online(&citizen.0).await;
        assert!(prepared(&citizen.0, &readiness).await.is_err());
        assert!(!home.marker().exists());
        assert_eq!(*relay.cursors.lock().unwrap(), before);
        while let Ok(call) = relay.calls.try_recv() {
            assert_eq!(
                call,
                AgentRequest::State,
                "a partial State seeded omitted spaces"
            );
        }
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn complete_state_preserves_every_existing_cursor_before_host_readiness() {
    let mut relay = relay(Mode::Paged).await;
    let citizen = citizen(&relay.url);
    for n in 0..3 {
        let space = citizen
            .0
            .create_group_with(format!("Existing {n}"), vec![], PrivacyDto::Closed)
            .unwrap();
        citizen
            .0
            .send_message(space.clone(), "Unhandled source".into())
            .unwrap();
        relay.cursors.lock().unwrap().insert(space, 0);
    }
    let before = relay.cursors.lock().unwrap().clone();
    let home = Home::new();
    let lock = HostLock::acquire(&home.0).unwrap();
    let readiness = Readiness::reset(&home.0, &lock).unwrap();
    online(&citizen.0).await;
    // Establish this positive fixture's actual sync precondition before testing
    // complete-state preservation with the original short readiness budget.
    // Separate cases retain the unsettled/MLS-refused readiness negatives.
    citizen.0.wait_until_settled(3000).await.unwrap();
    let result = prepared(&citizen.0, &readiness).await.unwrap();
    assert_eq!(result, before);
    assert_eq!(*relay.cursors.lock().unwrap(), before);
    assert!(home.marker().is_file());
    while let Ok(call) = relay.calls.try_recv() {
        assert_eq!(call, AgentRequest::State);
    }
}

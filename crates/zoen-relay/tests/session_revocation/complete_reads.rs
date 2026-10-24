//! Actual PostgreSQL snapshots + authenticated sockets. FDB runtime coverage lives
//! in the CLI journey; Devices must not depend on the test's blocked log at all.
use super::*;
use protocol::complete_reads::{self as reads, Assembly, Kind, Page, Request};

async fn capable(test: &TestRelay, author: &Author) -> Socket {
    let (mut ws, _) = tokio_tungstenite::connect_async(&test.ws_url)
        .await
        .unwrap();
    send(
        &mut ws,
        ClientFrame::Hello {
            protocol: PROTOCOL_VERSION,
            capabilities: vec![reads::CAPABILITY.into()],
            identity: author.identity.clone(),
            device: author.device.clone().unwrap(),
            cert: author.cert.clone().unwrap(),
        },
    )
    .await;
    let Some(ServerFrame::Challenge {
        nonce,
        relay,
        capabilities,
        ..
    }) = recv(&mut ws).await
    else {
        panic!("challenge")
    };
    assert!(capabilities.iter().any(|s| s == reads::CAPABILITY));
    send(
        &mut ws,
        ClientFrame::Auth {
            sig: author.key.sign(&auth_message(&nonce, &relay)),
        },
    )
    .await;
    assert!(matches!(
        recv(&mut ws).await,
        Some(ServerFrame::Ready { .. })
    ));
    ws
}
async fn setup(test: &TestRelay, root: &Signer) -> (Author, Socket, Vec<Author>) {
    let author = Author::device(root, Signer::generate());
    let mut socket = capable(test, &author).await;
    request(
        &mut socket,
        1,
        Op::Register {
            profile: profile(&author, "paged_directory"),
        },
    )
    .await
    .unwrap();
    let mut devices = (0..257)
        .map(|n| {
            let mut key = [0_u8; 32];
            key[..8].copy_from_slice(&(n as u64 + 1000).to_le_bytes());
            Author::device(root, Signer::from_secret(&key))
        })
        .collect::<Vec<_>>();
    devices.sort_by(|a, b| a.device.cmp(&b.device));
    let late = devices.remove(0);
    let mut tx = test.state.pool.begin().await.unwrap();
    for device in &devices {
        zoen_relay::db::enroll_device(
            &mut tx,
            &author.identity,
            device.device.as_deref().unwrap(),
            device.cert.as_deref().unwrap(),
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    devices.push(late);
    (author, socket, devices)
}
async fn next(socket: &mut Socket, id: u64, request_: Request) -> Page {
    let Reply::CompletePage(page) = request(socket, id, Op::CompleteRead(request_))
        .await
        .unwrap()
    else {
        panic!("partial legacy reply")
    };
    assert!(page.valid());
    assert!(page.devices.len() <= reads::PAGE_ROWS);
    assert!(page.wire_len() <= reads::PAGE_BYTES);
    page
}
async fn whole(socket: &mut Socket, author: &Author, id: u64) -> Vec<protocol::DeviceRecord> {
    let mut stage = Assembly::new(
        Kind::Devices,
        author.identity.clone(),
        author.device.clone().unwrap(),
    );
    let mut req = Request::begin(Kind::Devices);
    loop {
        let p = next(socket, id, req).await;
        match stage.accept(p).unwrap() {
            Some(r) => req = r,
            None => break,
        }
    }
    let Reply::Devices(devices) = stage.finish().unwrap() else {
        panic!("kind")
    };
    devices
}

#[tokio::test]
async fn complete_devices_keeps_one_snapshot_across_late_commit_and_revocation() {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let (author, mut socket, mut devices) = setup(&test, &root).await;
    let late = devices.pop().unwrap();
    let target = devices.last().unwrap();
    // A transaction started before the first page but commits behind its keyset.
    let mut tx = test.state.pool.begin().await.unwrap();
    zoen_relay::db::enroll_device(
        &mut tx,
        &author.identity,
        late.device.as_deref().unwrap(),
        late.cert.as_deref().unwrap(),
    )
    .await
    .unwrap();
    let first = next(&mut socket, 10, Request::begin(Kind::Devices)).await;
    assert!(!first.complete);
    assert!(late.device.as_ref().unwrap() < &first.devices.last().unwrap().device);
    tx.commit().await.unwrap();
    let mut controller = connect(&test.ws_url, &author).await;
    request(
        &mut controller,
        11,
        Op::Unlink {
            device: target.device.clone().unwrap(),
        },
    )
    .await
    .unwrap();
    let mut stage = Assembly::new(
        Kind::Devices,
        author.identity.clone(),
        author.device.clone().unwrap(),
    );
    let mut req = stage.accept(first).unwrap().unwrap();
    loop {
        let p = next(&mut socket, 12, req).await;
        match stage.accept(p).unwrap() {
            Some(r) => req = r,
            None => break,
        }
    }
    let Reply::Devices(original) = stage.finish().unwrap() else {
        panic!("complete")
    };
    assert_eq!(original.len(), 257);
    assert!(!original
        .iter()
        .any(|d| Some(&d.device) == late.device.as_ref()));
    assert!(
        !original
            .iter()
            .find(|d| Some(&d.device) == target.device.as_ref())
            .unwrap()
            .revoked
    );
    let current = whole(&mut socket, &author, 13).await;
    assert_eq!(current.len(), 258);
    assert!(current
        .iter()
        .any(|d| Some(&d.device) == late.device.as_ref()));
    assert!(
        current
            .iter()
            .find(|d| Some(&d.device) == target.device.as_ref())
            .unwrap()
            .revoked
    );
    // Legacy meaning is still complete, using the same bounded snapshot internals.
    let Reply::Devices(legacy) = request(&mut controller, 14, Op::Devices).await.unwrap() else {
        panic!("legacy")
    };
    assert_eq!(legacy, current);
    drop(socket);
    drop(controller);
    test.close().await;
}
#[tokio::test]
async fn snapshot_tokens_are_connection_bound_and_cancel_releases_the_global_slot() {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let (author, mut first, _) = setup(&test, &root).await;
    let p = next(&mut first, 20, Request::begin(Kind::Devices)).await;
    assert!(
        request(&mut first, 19, Op::Devices)
            .await
            .unwrap_err()
            .contains("complete_read_busy"),
        "legacy read overlapped its connection's retained snapshot"
    );
    let mut second = capable(&test, &author).await;
    let foreign = Request {
        kind: p.kind,
        snapshot: p.snapshot.clone(),
        ordinal: 1,
        cancel: false,
    };
    assert!(request(&mut second, 21, Op::CompleteRead(foreign.clone()))
        .await
        .is_err());
    let mut sockets = vec![first];
    let mut tokens = vec![foreign];
    for n in 1..zoen_relay::read_snapshots::MAX_SNAPSHOTS {
        let mut s = capable(&test, &author).await;
        let p = next(&mut s, 30 + n as u64, Request::begin(Kind::Devices)).await;
        tokens.push(Request {
            kind: p.kind,
            snapshot: p.snapshot,
            ordinal: 1,
            cancel: false,
        });
        sockets.push(s);
    }
    assert!(request(
        &mut second,
        40,
        Op::CompleteRead(Request::begin(Kind::Devices))
    )
    .await
    .unwrap_err()
    .contains("complete_read_busy"));
    // Dedicated read lanes cannot starve fresh device admission or the writer.
    send(&mut sockets[0], ClientFrame::Ping).await;
    loop {
        if matches!(recv(&mut sockets[0]).await, Some(ServerFrame::Pong { .. })) {
            break;
        }
    }
    let cancel = Request {
        cancel: true,
        ..tokens.remove(0)
    };
    assert_eq!(
        request(&mut sockets[0], 41, Op::CompleteRead(cancel))
            .await
            .unwrap(),
        Reply::Done
    );
    let p = next(&mut second, 42, Request::begin(Kind::Devices)).await;
    assert!(!p.complete);
    for s in sockets {
        drop(s);
    }
    drop(second);
    test.close().await;
}
#[tokio::test]
async fn every_page_rechecks_the_current_requesting_device_after_unlink() {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let (author, mut reader, devices) = setup(&test, &root).await;
    let controller_author = &devices[0];
    let mut controller = connect(&test.ws_url, controller_author).await;
    let page = next(&mut reader, 50, Request::begin(Kind::Devices)).await;
    request(
        &mut controller,
        51,
        Op::Unlink {
            device: author.device.clone().unwrap(),
        },
    )
    .await
    .unwrap();
    send(
        &mut reader,
        ClientFrame::Req {
            id: 52,
            op: Op::CompleteRead(Request {
                kind: page.kind,
                snapshot: page.snapshot,
                ordinal: 1,
                cancel: false,
            }),
        },
    )
    .await;
    while let Some(frame) = recv(&mut reader).await {
        assert!(!matches!(
            frame,
            ServerFrame::Res {
                id: 52,
                result: Ok(Reply::CompletePage(_))
            }
        ));
    }
    drop(controller);
    test.close().await;
}
#[tokio::test]
async fn negotiated_read_expiry_is_explicit_and_does_not_pin_its_pool_afterward() {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let (author, mut reader, _) = setup(&test, &root).await;
    let page = next(&mut reader, 60, Request::begin(Kind::Devices)).await;
    tokio::time::sleep(zoen_relay::read_snapshots::MAX_AGE + Duration::from_millis(50)).await;
    let stale = Request {
        kind: page.kind,
        snapshot: page.snapshot,
        ordinal: 1,
        cancel: false,
    };
    assert!(request(&mut reader, 61, Op::CompleteRead(stale))
        .await
        .unwrap_err()
        .contains("complete_read_stale"));
    let current = whole(&mut reader, &author, 62).await;
    assert_eq!(current.len(), 257);
    drop(reader);
    test.close().await;
}

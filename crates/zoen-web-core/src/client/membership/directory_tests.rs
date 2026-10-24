use super::*;
use crate::client::tests::{account, HostSigner};
use event_log::Signer as NativeSigner;
use protocol::complete_reads::{self as reads, Device, Kind, Page};

fn fixture() -> Client<HostSigner> {
    let root = NativeSigner::from_secret(&[201; 32]);
    let key = NativeSigner::from_secret(&[202; 32]);
    let mut client = Client::new(account(&root, &key), HostSigner(key), "").unwrap();
    client
        .saved
        .membership
        .revoked
        .insert(NativeSigner::from_secret(&[203; 32]).id());
    client.view_now_ms = 1000;
    client.read_capability(true);
    assert!(matches!(
        client.device_directory_begin().unwrap(),
        Op::CompleteRead(_)
    ));
    client.membership_net.devices_requested = true;
    client.membership_net.devices_due = 16_000;
    client
}
fn page(client: &Client<HostSigner>, ordinal: u32, previous: &[u8], cut: u8) -> Page {
    let mut list = [
        Device {
            device: client.account.device.clone(),
            revoked: false,
        },
        Device {
            device: NativeSigner::from_secret(&[204; 32]).id(),
            revoked: true,
        },
    ];
    list.sort_by(|a, b| a.device.cmp(&b.device));
    let mut p = Page {
        kind: Kind::Devices as i32,
        snapshot: "e".repeat(64),
        ordinal,
        identity: if cut == 1 {
            "f".repeat(64)
        } else {
            client.account.identity.clone()
        },
        device: if cut == 2 {
            "f".repeat(64)
        } else {
            client.account.device.clone()
        },
        header: None,
        devices: vec![list[ordinal as usize].clone()],
        cursors: Vec::new(),
        complete: ordinal == 1,
        count: ordinal + 1,
        digest: Vec::new(),
    };
    p.digest = p.rolling_digest(previous);
    p
}
#[test]
fn browser_pages_are_ephemeral_until_complete_and_cold_snapshot_retains_last_full_set() {
    let mut client = fixture();
    let before = client.saved.membership.revoked.clone();
    let first = page(&client, 0, &[], 0);
    let mut outgoing = Vec::new();
    client
        .device_directory_reply(&Ok(Reply::CompletePage(first.clone())), &mut outgoing)
        .unwrap();
    assert_eq!(client.saved.membership.revoked, before);
    assert_eq!(outgoing.len(), 1);
    assert!(
        matches!(ClientFrame::decode(&outgoing[0]).unwrap(),ClientFrame::Req {id,op:Op::CompleteRead(request)} if id==client.membership_net.devices_request && request.ordinal==1)
    );
    let snapshot = serde_json::to_string(&client.saved).unwrap();
    let cold = Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    assert_eq!(cold.saved.membership.revoked, before);
    let end = page(&client, 1, &first.digest, 0);
    client
        .device_directory_reply(&Ok(Reply::CompletePage(end)), &mut outgoing)
        .unwrap();
    let expected = BTreeSet::from([NativeSigner::from_secret(&[204; 32]).id()]);
    assert_eq!(client.saved.membership.revoked, expected);
    let snapshot = serde_json::to_string(&client.saved).unwrap();
    let cold = Client::new(client.account.clone(), client.signer.clone(), &snapshot).unwrap();
    assert_eq!(cold.saved.membership.revoked, expected);
}
#[test]
fn browser_missing_duplicate_foreign_device_and_error_pages_keep_the_old_directory() {
    for cut in 0..5 {
        let mut client = fixture();
        let before = client.saved.membership.revoked.clone();
        let first = page(&client, 0, &[], 0);
        let mut outgoing = Vec::new();
        client
            .device_directory_reply(&Ok(Reply::CompletePage(first.clone())), &mut outgoing)
            .unwrap();
        let result = match cut {
            0 => Err("lost page".into()),
            1 | 2 => Ok(Reply::CompletePage(page(&client, 1, &first.digest, cut))),
            3 => Ok(Reply::CompletePage(first.clone())),
            _ => {
                let mut p = page(&client, 1, &first.digest, 0);
                p.count = 0;
                p.digest = p.rolling_digest(&first.digest);
                Ok(Reply::CompletePage(p))
            }
        };
        client
            .device_directory_reply(&result, &mut outgoing)
            .unwrap();
        assert_eq!(client.saved.membership.revoked, before, "cut{cut}");
        assert!(!client.membership_net.devices_requested);
    }
}
#[test]
fn browser_reconnect_or_expired_page_cannot_revive_or_replace_old_complete_state() {
    for reconnect in [false, true] {
        let mut client = fixture();
        let before = client.saved.membership.revoked.clone();
        let first = page(&client, 0, &[], 0);
        let mut outgoing = Vec::new();
        client
            .device_directory_reply(&Ok(Reply::CompletePage(first.clone())), &mut outgoing)
            .unwrap();
        if reconnect {
            client.membership_net.restart();
        } else {
            client.view_now_ms = 16_001;
        }
        client
            .device_directory_reply(
                &Ok(Reply::CompletePage(page(&client, 1, &first.digest, 0))),
                &mut outgoing,
            )
            .unwrap();
        assert_eq!(client.saved.membership.revoked, before);
    }
}

#[test]
fn browser_new_read_uses_a_new_correlation_and_rejects_late_old_terminal() {
    let mut client = fixture();
    let before = client.saved.membership.revoked.clone();
    let old_id = client.membership_net.devices_request;
    let first = page(&client, 0, &[], 0);
    let mut old_terminal = first.clone();
    old_terminal.complete = true;
    old_terminal.digest = old_terminal.rolling_digest(&[]);
    client.membership_net.devices_requested = false;
    client.membership_net.devices_assembly = None;
    client.device_directory_begin().unwrap();
    client.membership_net.devices_requested = true;
    assert_ne!(client.membership_net.devices_request, old_id);
    let mut outgoing = Vec::new();
    assert!(client
        .membership_reply(
            old_id,
            &Ok(Reply::CompletePage(old_terminal)),
            &mut outgoing
        )
        .unwrap());
    assert_eq!(client.saved.membership.revoked, before);
    assert!(outgoing.is_empty());
    assert!(client.membership_net.devices_assembly.is_some());
}

#[test]
fn browser_fixed_deadline_cancels_the_accepted_snapshot_before_another_read_can_start() {
    let mut client = fixture();
    let before = client.saved.membership.revoked.clone();
    let first = page(&client, 0, &[], 0);
    let mut frames = Vec::new();
    client.view_now_ms = 15_999;
    client
        .device_directory_reply(&Ok(Reply::CompletePage(first.clone())), &mut frames)
        .unwrap();
    assert_eq!(
        client.membership_net.devices_due, 16_000,
        "deadline renewed per page"
    );
    frames.clear();
    client.synced = true;
    client.view_now_ms = 16_001;
    let old_id = client.membership_net.devices_request;
    client.membership_frames(&mut frames).unwrap();
    assert_eq!(frames.len(), 1);
    let ClientFrame::Req {
        id,
        op: Op::CompleteRead(cancel),
    } = ClientFrame::decode(&frames[0]).unwrap()
    else {
        panic!("snapshot cancellation")
    };
    assert_eq!(id, old_id);
    assert!(cancel.cancel);
    assert_eq!(cancel.snapshot, first.snapshot);
    assert_eq!(cancel.ordinal, 1);
    assert_eq!(client.saved.membership.revoked, before);
    assert!(client.membership_net.devices_assembly.is_none());
    assert!(!client.membership_net.devices_requested);
}

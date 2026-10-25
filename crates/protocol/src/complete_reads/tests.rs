use super::*;
use crate::{ClientFrame, Op, ServerFrame};

fn page(kind: Kind, ordinal: u32, count: u32, complete: bool) -> Page {
    Page {
        kind: kind as i32,
        snapshot: "a".repeat(64),
        ordinal,
        identity: "b".repeat(64),
        device: "c".repeat(64),
        header: (kind == Kind::AgentState && ordinal == 0).then(|| Header {
            status: "active".into(),
            kind: "User".into(),
            definition_version: 3,
            definition: vec![7; 32],
        }),
        devices: Vec::new(),
        cursors: Vec::new(),
        complete,
        count,
        digest: Vec::new(),
    }
}
fn seal(mut p: Page, previous: &[u8]) -> Page {
    p.digest = p.rolling_digest(previous);
    p
}
fn assembly(kind: Kind) -> Assembly {
    Assembly::new(kind, "b".repeat(64), "c".repeat(64))
}

#[test]
fn bounded_pages_roundtrip_and_explicit_empty_terminal_preserve_all_cursors() {
    let mut first = page(Kind::AgentState, 0, 128, false);
    first.cursors = (0..128)
        .map(|n| Cursor {
            space: format!("sp_{n:04}"),
            seq: n,
        })
        .collect();
    let first = seal(first, &[]);
    let wire = ServerFrame::Res {
        id: 17,
        result: Ok(Reply::CompletePage(first.clone())),
    }
    .encode();
    assert_eq!(
        ServerFrame::decode(&wire).unwrap(),
        ServerFrame::Res {
            id: 17,
            result: Ok(Reply::CompletePage(first.clone()))
        }
    );
    let mut staged = assembly(Kind::AgentState);
    let next = staged.accept(first.clone()).unwrap().unwrap();
    assert_eq!(next.ordinal, 1);
    let request = ClientFrame::Req {
        id: 17,
        op: Op::CompleteRead(next),
    };
    assert_eq!(ClientFrame::decode(&request.encode()).unwrap(), request);
    let terminal = seal(page(Kind::AgentState, 1, 128, true), &first.digest);
    assert!(staged.accept(terminal).unwrap().is_none());
    let Reply::AgentState(full) = staged.finish().unwrap() else {
        panic!("wrong complete kind")
    };
    assert_eq!(full.cursors.len(), 128);
    assert_eq!(full.cursors[127], ("sp_0127".into(), 127));
    assert_eq!(full.definition, vec![7; 32]);
}
#[test]
fn partial_and_poisoned_assemblies_never_publish_success() {
    let first = seal(page(Kind::AgentState, 0, 0, false), &[]);
    let mut partial = assembly(Kind::AgentState);
    partial.accept(first.clone()).unwrap();
    assert!(partial.finish().is_err());
    let mut duplicate = assembly(Kind::AgentState);
    duplicate.accept(first.clone()).unwrap();
    assert!(duplicate.accept(first.clone()).is_err());
    let terminal = seal(page(Kind::AgentState, 1, 0, true), &first.digest);
    assert!(duplicate.accept(terminal).is_err());
    assert!(duplicate.finish().is_err());
}
#[test]
fn owner_device_token_kind_ordinal_count_header_and_digest_are_bound() {
    for cut in 0..8 {
        let mut p = page(Kind::AgentState, 0, 0, true);
        match cut {
            0 => p.identity = "d".repeat(64),
            1 => p.device = "d".repeat(64),
            2 => p.kind = Kind::Devices as i32,
            3 => p.ordinal = 1,
            4 => p.count = 1,
            5 => p.header = None,
            6 => p.header.as_mut().unwrap().definition = vec![0; DEFINITION_BYTES + 1],
            _ => {}
        }
        let mut p = seal(p, &[]);
        if cut == 7 {
            p.digest[0] ^= 1;
        }
        let mut a = assembly(Kind::AgentState);
        assert!(a.accept(p).is_err(), "cut{cut}");
        assert!(a.finish().is_err());
    }
    let first = seal(page(Kind::AgentState, 0, 0, false), &[]);
    for cut in 0..3 {
        let mut a = assembly(Kind::AgentState);
        a.accept(first.clone()).unwrap();
        let mut end = page(Kind::AgentState, 1, 0, true);
        match cut {
            0 => end.snapshot = "d".repeat(64),
            1 => end.header = first.header.clone(),
            _ => end.ordinal = 2,
        }
        let end = seal(end, &first.digest);
        assert!(a.accept(end).is_err());
    }
}
#[test]
fn row_order_and_unique_keys_are_checked_across_page_boundaries() {
    for next in ["sp_a", "sp_0"] {
        let mut first = page(Kind::AgentState, 0, 1, false);
        first.cursors.push(Cursor {
            space: "sp_a".into(),
            seq: 3,
        });
        let first = seal(first, &[]);
        let mut a = assembly(Kind::AgentState);
        a.accept(first.clone()).unwrap();
        let mut last = page(Kind::AgentState, 1, 2, true);
        last.cursors.push(Cursor {
            space: next.into(),
            seq: 4,
        });
        assert!(a.accept(seal(last, &first.digest)).is_err());
    }
}
#[test]
fn large_device_directory_is_complete_but_the_operational_profile_is_finite(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut a = assembly(Kind::Devices);
    let mut digest = Vec::new();
    for n in 0..DEVICE_ROWS / PAGE_ROWS {
        let mut p = page(
            Kind::Devices,
            u32::try_from(n)?,
            u32::try_from((n + 1) * PAGE_ROWS)?,
            false,
        );
        p.devices = (n * PAGE_ROWS..(n + 1) * PAGE_ROWS)
            .map(|i| Device {
                device: format!("{i:064x}"),
                revoked: i % 2 == 1,
            })
            .collect();
        let p = seal(p, &digest);
        digest = p.digest.clone();
        assert!(a.accept(p).unwrap().is_some());
    }
    let mut extra = page(
        Kind::Devices,
        u32::try_from(DEVICE_ROWS / PAGE_ROWS)?,
        u32::try_from(DEVICE_ROWS + 1)?,
        true,
    );
    extra.devices.push(Device {
        device: format!("{DEVICE_ROWS:064x}"),
        revoked: false,
    });
    assert_eq!(a.accept(seal(extra, &digest)), Err(PROFILE_EXCEEDED));
    assert!(a.finish().is_err());
    Ok(())
}
#[test]
fn query_page_and_encoding_limits_are_independent_of_total_assembly(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut oversized = page(Kind::AgentState, 0, u32::try_from(PAGE_ROWS + 1)?, true);
    oversized.cursors = (0..=PAGE_ROWS)
        .map(|n| Cursor {
            space: format!("sp_{n:04}"),
            seq: 0,
        })
        .collect();
    let mut a = assembly(Kind::AgentState);
    assert!(a.accept(seal(oversized, &[])).is_err());
    let mut oversized = page(Kind::AgentState, 0, 1, true);
    oversized.cursors.push(Cursor {
        space: "s".repeat(KEY_BYTES + 1),
        seq: 0,
    });
    let mut a = assembly(Kind::AgentState);
    assert!(a.accept(seal(oversized, &[])).is_err());
    assert!(!Request {
        ordinal: 2,
        ..Request::begin(Kind::Devices)
    }
    .valid());
    assert!(!Request {
        snapshot: "x".repeat(64),
        ..Request::begin(Kind::Devices)
    }
    .valid());
    Ok(())
}

#[test]
fn cancellation_names_only_the_last_accepted_snapshot_and_never_a_completed_one() {
    let mut a = assembly(Kind::AgentState);
    assert!(a.cancel_request().is_none());
    let first = seal(page(Kind::AgentState, 0, 0, false), &[]);
    a.accept(first.clone()).unwrap();
    let cancel = a.cancel_request().unwrap();
    assert!(cancel.cancel);
    assert!(cancel.valid());
    assert_eq!(cancel.snapshot, first.snapshot);
    assert_eq!(cancel.ordinal, 1);
    a.accept(seal(page(Kind::AgentState, 1, 0, true), &first.digest))
        .unwrap();
    assert!(a.cancel_request().is_none());
    assert!(a.finish().is_ok());
}

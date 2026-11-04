use super::*;

fn request() -> Request {
    Request {
        namespace: "native-runtime".into(),
        nonce: vec![23; 32],
        managed: Some(ManagedCheckpoint {
            generation: 1,
            sequence: 4,
            digest: "ab".repeat(32),
            source_fence: Some(SourceFence {
                space: "private-conversation".into(),
                head: types::Seen {
                    seq: 7,
                    hash: "cd".repeat(32),
                },
                run: "run-original".into(),
                attempt: "attempt-original".into(),
                expires_at_ms: Some(5000),
            }),
        }),
    }
}
fn expected(request: Request) -> Result<Expectation, Box<dyn std::error::Error>> {
    Expectation::new(
        &"11".repeat(32),
        request,
        "source",
        "relay",
        "agent",
        &"22".repeat(32),
    )
    .ok_or_else(|| "invalid test request".into())
}

#[test]
fn source_fence_roundtrips_and_mac_binds_every_fact_and_absence(
) -> Result<(), Box<dyn std::error::Error>> {
    let original = request();
    let proof = expected(original.clone())?.proof();
    let frame = crate::ClientFrame::Req {
        id: 3,
        op: crate::Op::RuntimeBinding(original.clone()),
    };
    assert_eq!(crate::ClientFrame::decode(&frame.encode()), Ok(frame));
    for change in 0..7 {
        let mut candidate = original.clone();
        let checkpoint = candidate
            .managed
            .as_mut()
            .ok_or("missing managed test checkpoint")?;
        if change == 0 {
            checkpoint.source_fence = None;
        } else {
            let fence = checkpoint
                .source_fence
                .as_mut()
                .ok_or("missing test source fence")?;
            match change {
                1 => fence.space.push('2'),
                2 => fence.head.seq += 1,
                3 => fence.head.hash = "ef".repeat(32),
                4 => fence.run.push('2'),
                5 => fence.attempt.push('2'),
                6 => fence.expires_at_ms = None,
                _ => panic!("unexpected test mutation"),
            }
        }
        assert!(!expected(candidate.clone())?.verify(&proof));
        assert!(!expected(original.clone())?.verify(&expected(candidate)?.proof()));
    }
    Ok(())
}

#[test]
fn source_fence_rejects_invalid_fields_and_cannot_attach_to_bootstrap(
) -> Result<(), Box<dyn std::error::Error>> {
    for change in 0..8 {
        let mut candidate = request();
        let checkpoint = candidate
            .managed
            .as_mut()
            .ok_or("missing managed test checkpoint")?;
        let fence = checkpoint
            .source_fence
            .as_mut()
            .ok_or("missing test source fence")?;
        match change {
            0 => fence.space.clear(),
            1 => fence.space = "a".repeat(257),
            2 => fence.head.hash = "CD".repeat(32),
            3 => fence.head.seq = u64::MAX,
            4 => fence.run.clear(),
            5 => fence.attempt = "invalid/key".into(),
            6 => checkpoint.sequence = 0,
            7 => fence.expires_at_ms = Some(-1),
            _ => panic!("unexpected mutation"),
        }
        assert!(!candidate.valid());
        let frame = crate::ClientFrame::Req {
            id: 5,
            op: crate::Op::RuntimeBinding(candidate),
        };
        assert!(crate::ClientFrame::decode(&frame.encode()).is_err());
    }
    let mut legacy = request();
    let checkpoint = legacy
        .managed
        .as_mut()
        .ok_or("missing managed test checkpoint")?;
    checkpoint.source_fence = None;
    assert!(serde_json::to_value(&legacy)
        .is_ok_and(|json| json["managed"].get("source_fence").is_none()));
    Ok(())
}

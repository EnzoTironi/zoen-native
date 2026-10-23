use super::*;
use crate::{ClientFrame, Op, Reply, ServerFrame};

fn request() -> Request {
    Request {
        namespace: "runtime-a".into(),
        nonce: vec![19; 32],
        managed: None,
    }
}

#[test]
fn managed_checkpoint_roundtrip_binds_every_field_and_cannot_be_stripped() {
    let checkpoint = ManagedCheckpoint {
        generation: 2,
        sequence: 17,
        digest: "ab".repeat(32),
    };
    let mut managed = request();
    managed.managed = Some(checkpoint.clone());
    let device = "34".repeat(32);
    let proof = expectation(managed.clone(), "cell-a", "relay-a", "agent-a", &device).proof();
    assert!(!expectation(request(), "cell-a", "relay-a", "agent-a", &device).verify(&proof));
    let frame = ClientFrame::Req {
        id: 7,
        op: Op::RuntimeBinding(managed.clone()),
    };
    assert_eq!(ClientFrame::decode(&frame.encode()), Ok(frame));
    let mut changed = vec![checkpoint.clone(); 3];
    changed[0].generation += 1;
    changed[1].sequence += 1;
    changed[2].digest = "cd".repeat(32);
    for checkpoint in changed {
        let mut candidate = managed.clone();
        candidate.managed = Some(checkpoint);
        assert!(!expectation(candidate, "cell-a", "relay-a", "agent-a", &device).verify(&proof));
    }
    let mut invalid = vec![checkpoint; 5];
    invalid[0].generation = 0;
    invalid[1].sequence = 0;
    invalid[2].generation = u64::MAX;
    invalid[3].sequence = u64::MAX;
    invalid[4].digest = "AB".repeat(32);
    for checkpoint in invalid {
        let mut candidate = managed.clone();
        candidate.managed = Some(checkpoint);
        let frame = ClientFrame::Req {
            id: 7,
            op: Op::RuntimeBinding(candidate),
        };
        assert!(ClientFrame::decode(&frame.encode()).is_err());
    }
}

fn expectation(
    request: Request,
    cell: &str,
    relay: &str,
    identity: &str,
    device: &str,
) -> Expectation {
    Expectation::new(&"12".repeat(32), request, cell, relay, identity, device)
        .expect("valid fixture")
}

#[test]
fn binds_every_route_store_and_actor_field() {
    let device = "34".repeat(32);
    let expected = expectation(request(), "cell-a", "relay-a", "agent-a", &device);
    let proof = expected.proof();
    assert!(expected.verify(&proof));
    let candidates = [
        expectation(
            Request {
                namespace: "runtime-b".into(),
                ..request()
            },
            "cell-a",
            "relay-a",
            "agent-a",
            &device,
        ),
        expectation(
            Request {
                nonce: vec![20; 32],
                ..request()
            },
            "cell-a",
            "relay-a",
            "agent-a",
            &device,
        ),
        expectation(request(), "cell-b", "relay-a", "agent-a", &device),
        expectation(request(), "cell-a", "relay-b", "agent-a", &device),
        expectation(request(), "cell-a", "relay-a", "agent-b", &device),
        expectation(request(), "cell-a", "relay-a", "agent-a", &"56".repeat(32)),
        Expectation::new(
            &"78".repeat(32),
            request(),
            "cell-a",
            "relay-a",
            "agent-a",
            &device,
        )
        .expect("valid other pairing"),
    ];
    for wrong in candidates {
        assert!(!wrong.verify(&proof));
    }
    for length in [0, 1, 31, 33, 64] {
        assert!(!expected.verify(&Proof {
            cell: "cell-a".into(),
            tag: vec![0; length]
        }));
    }
}

#[test]
fn wire_roundtrip_preserves_proof_and_rejects_bad_bounds() {
    let request = request();
    let frame = ClientFrame::Req {
        id: 7,
        op: Op::RuntimeBinding(request.clone()),
    };
    assert_eq!(ClientFrame::decode(&frame.encode()), Ok(frame));
    let expected = expectation(
        request.clone(),
        "cell-a",
        "relay-a",
        "agent-a",
        &"34".repeat(32),
    );
    let frame = ServerFrame::Res {
        id: 7,
        result: Ok(Reply::RuntimeBinding(expected.proof())),
    };
    assert_eq!(ServerFrame::decode(&frame.encode()), Ok(frame));
    for namespace in [
        String::new(),
        "x".repeat(129),
        "../other".into(),
        "white space".into(),
    ] {
        let frame = ClientFrame::Req {
            id: 7,
            op: Op::RuntimeBinding(Request {
                namespace,
                ..request.clone()
            }),
        };
        assert!(ClientFrame::decode(&frame.encode()).is_err());
    }
    for length in [0, 31, 33] {
        let frame = ClientFrame::Req {
            id: 7,
            op: Op::RuntimeBinding(Request {
                nonce: vec![1; length],
                ..request.clone()
            }),
        };
        assert!(ClientFrame::decode(&frame.encode()).is_err());
        let frame = ServerFrame::Res {
            id: 7,
            result: Ok(Reply::RuntimeBinding(Proof {
                cell: "cell-a".into(),
                tag: vec![0; length],
            })),
        };
        assert!(ServerFrame::decode(&frame.encode()).is_err());
    }
}

#[test]
fn bound_upload_signature_cannot_be_downgraded_or_retargeted() {
    let key = event_log::Signer::from_secret(&[23; 32]);
    let expected = expectation(request(), "cell-a", "relay-a", "agent-a", &key.id());
    let binding = BlobBinding {
        request: request(),
        proof: expected.proof(),
        identity: "agent-a".into(),
    };
    let sha = "ab".repeat(32);
    let message = binding
        .message(&sha, 123, "relay-a")
        .expect("valid binding");
    let signature = key.sign(&message);
    assert!(event_log::verify_sig(&key.id(), &message, &signature));
    assert!(!event_log::verify_sig(
        &key.id(),
        &crate::blob_put_message(&sha, 123, "relay-a"),
        &signature
    ));
    let mut changed = binding.clone();
    changed.proof.cell = "cell-b".into();
    assert!(!event_log::verify_sig(
        &key.id(),
        &changed
            .message(&sha, 123, "relay-a")
            .expect("valid changed binding"),
        &signature
    ));
    assert!(!event_log::verify_sig(
        &key.id(),
        &binding
            .message(&sha, 124, "relay-a")
            .expect("valid timestamp"),
        &signature
    ));
    let json = serde_json::to_string(&binding).expect("JSON fixture");
    assert!(json.len() < 2048);
    let altered = json.replacen('{', "{\"unexpected\":true,", 1);
    assert!(serde_json::from_str::<BlobBinding>(&altered).is_err());
}

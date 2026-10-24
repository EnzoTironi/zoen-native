use protocol::runtime_binding::{BlobBinding, Expectation, Request};
use relay_admission::blobs::{put_bound_policy, put_policy};

#[test]
fn bound_signature_is_required_for_the_complete_binding() {
    let key = event_log::Signer::from_secret(&[71; 32]);
    let request = Request {
        namespace: "runtime-a".into(),
        nonce: vec![12; 32],
        managed: None,
    };
    let expected = Expectation::new(
        &"19".repeat(32),
        request.clone(),
        "cell-a",
        "relay-a",
        "agent-a",
        &key.id(),
    )
    .unwrap();
    let binding = BlobBinding {
        request,
        proof: expected.proof(),
        identity: "agent-a".into(),
    };
    let sha = "a1".repeat(32);
    let at = 12345;
    let signature = key.sign(&binding.message(&sha, at, "relay-a").unwrap());
    assert!(put_bound_policy(
        &sha,
        &key.id(),
        &at.to_string(),
        &signature,
        "relay-a",
        at,
        &binding
    )
    .is_ok());
    assert_eq!(
        put_policy(&sha, &key.id(), &at.to_string(), &signature, "relay-a", at)
            .unwrap_err()
            .status,
        401
    );
    let legacy = key.sign(&protocol::blob_put_message(&sha, at, "relay-a"));
    assert!(put_policy(&sha, &key.id(), &at.to_string(), &legacy, "relay-a", at).is_ok());
    assert_eq!(
        put_bound_policy(
            &sha,
            &key.id(),
            &at.to_string(),
            &legacy,
            "relay-a",
            at,
            &binding
        )
        .unwrap_err()
        .status,
        401
    );
    let mut changed = binding.clone();
    changed.proof.cell = "cell-b".into();
    assert_eq!(
        put_bound_policy(
            &sha,
            &key.id(),
            &at.to_string(),
            &signature,
            "relay-a",
            at,
            &changed
        )
        .unwrap_err()
        .status,
        401
    );
    assert_eq!(
        put_bound_policy(
            &sha,
            &key.id(),
            &at.to_string(),
            &signature,
            "relay-b",
            at,
            &binding
        )
        .unwrap_err()
        .status,
        401
    );
    assert_eq!(
        put_bound_policy(
            &sha,
            &key.id(),
            &at.to_string(),
            &signature,
            "relay-a",
            at + 300_001,
            &binding
        )
        .unwrap_err()
        .status,
        401
    );
}

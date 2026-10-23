use super::*;
use crate::UsageEvidence;

#[test]
fn poisoned_capture_retains_bytes_but_never_certifies_completion_or_usage() {
    let capture = Arc::new(Mutex::new(Capture {
        status: Some(200),
        request_id: Some("retained-provider-attempt".into()),
        body: br#"{"usage":{"prompt_tokens":5,"completion_tokens":2,"total_tokens":7}}"#.to_vec(),
        complete: true,
        failure: None,
    }));
    let poisoned = capture.clone();
    let panic = std::thread::spawn(move || {
        let _guard = poisoned.lock().unwrap();
        panic!("injected capture writer panic");
    })
    .join();
    assert!(panic.is_err());
    let retained = match snapshot(&capture) {
        Ok(_) => panic!("poison must remain an explicit snapshot error"),
        Err(incomplete) => incomplete.into_incomplete(),
    };
    assert!(!retained.complete);
    assert_eq!(retained.failure, Some(OutputFailure::Transport));
    assert_eq!(retained.status, Some(200));
    assert_eq!(
        retained.request_id.as_deref(),
        Some("retained-provider-attempt")
    );
    assert!(!retained.body.is_empty());
    let (receipt, raw) = crate::receipt::receipt(&retained);
    assert!(raw.is_none());
    assert_eq!(receipt.usage, UsageEvidence::Missing);
}

#[test]
fn successful_capture_snapshot_preserves_exact_evidence() {
    let capture = Mutex::new(Capture {
        status: Some(200),
        request_id: Some("provider-attempt".into()),
        body: b"original bounded response".to_vec(),
        complete: true,
        failure: None,
    });
    let captured = match snapshot(&capture) {
        Ok(captured) => captured,
        Err(_) => panic!("healthy mutex must not become an incomplete attempt"),
    };
    assert_eq!(captured.body, b"original bounded response");
    assert!(captured.complete);
    assert_eq!(captured.failure, None);
}

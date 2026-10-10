use super::*;

#[derive(Clone)]
struct LogWriter(Arc<Mutex<Vec<u8>>>);
impl std::io::Write for LogWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn actual_trace_and_debug_never_contain_provider_or_content_canaries() {
    // Global collection also observes transport tasks on other runtime threads.
    // The external marker proves TRACE is enabled; an empty/suppressed collector
    // cannot falsely pass the canary assertions.
    let logs = Arc::new(Mutex::new(Vec::new()));
    let output = logs.clone();
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .without_time()
        .with_ansi(false)
        .with_writer(move || LogWriter(output.clone()))
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();
    tracing::trace!("zoen-outside-trace-marker");
    for (status, body, lost) in [
        (200, reply().to_string(), false),
        (200, format!("malformed {KEY} {PROMPT} {BODY}"), false),
        (500, format!("provider-error {KEY} {PROMPT} {BODY}"), false),
        (200, String::new(), true),
    ] {
        let fixture = Fixture::serve(
            status,
            body,
            "x-request-id: privacy-test\r\n",
            false,
            Duration::ZERO,
            lost,
        )
        .await;
        let cfg = config(&fixture.base);
        let config_debug = format!("{cfg:?}");
        let gateway = ModelGateway::new(cfg).unwrap();
        let input = request();
        let request_debug = format!("{input:?}");
        let result = gateway
            .complete(input, &TestAuthority::default())
            .await
            .unwrap();
        let debug = format!(
            "{config_debug} {request_debug} {gateway:?} {result:?} {:?} {:?} {:?}",
            result.receipt, result.output, result.evidence
        );
        for canary in [KEY, PROMPT, BODY] {
            assert!(!debug.contains(canary), "redacted DTO Debug");
        }
    }
    tracing::trace!("zoen-after-trace-marker");
    let logs = String::from_utf8(logs.lock().unwrap().clone()).unwrap();
    assert!(logs.contains("zoen-outside-trace-marker"));
    assert!(logs.contains("zoen-after-trace-marker"));
    for canary in [KEY, PROMPT, BODY] {
        assert!(
            !logs.contains(canary),
            "private content escaped TRACE: {:?}",
            logs.lines()
                .filter(|line| line.contains(canary))
                .collect::<Vec<_>>()
        );
    }
}

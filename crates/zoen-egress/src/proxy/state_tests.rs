use super::*;

struct NoSecrets;
impl SecretSource for NoSecrets {
    fn secret(&self, _: &str, _: &str) -> Option<String> {
        None
    }
}
struct NoCards;
impl ApprovalSink for NoCards {
    fn opened(&self, _: &str, _: &AgentRequest) {}
}

#[test]
fn invalid_operator_roots_do_not_create_a_proxy() {
    let config = EgressConfig {
        extra_upstream_roots_pem: vec!["invalid PEM".into()],
        ..EgressConfig::default()
    };
    assert!(Egress::new(
        config,
        Resolver::System,
        Arc::new(NoSecrets),
        Arc::new(NoCards)
    )
    .is_err());
}

#[test]
fn poisoned_state_never_reports_revocation_empty_log_or_missing_certificate() {
    let egress = Egress::new(
        EgressConfig::default(),
        Resolver::System,
        Arc::new(NoSecrets),
        Arc::new(NoCards),
    )
    .unwrap();
    let poisoned = egress.clone();
    assert!(std::thread::spawn(move || {
        let _guard = poisoned.state.lock().unwrap();
        panic!("simulate owner crash while state is held");
    })
    .join()
    .is_err());
    assert!(egress.revoke("lease").is_err());
    assert!(egress.deny("request").is_err());
    assert!(egress.log().is_err());
    assert!(egress.lease_ca_pem("lease").is_err());
    let policy = LeasePolicy {
        lease: "lease".into(),
        token: "token".into(),
        owner: "owner".into(),
        agent: "agent".into(),
        tool: "tool".into(),
        rules: vec![],
        secrets: vec![],
        sign_requests: false,
        browser: false,
    };
    assert!(egress.register(policy.clone()).is_err());
    assert_eq!(
        egress.allowed(&policy, "example.invalid", 443, "GET"),
        Err(Decision::UpstreamError)
    );
    assert!(egress
        .approve("request", ApprovalScope::AlwaysForTool)
        .is_err());
}

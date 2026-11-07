use super::*;
use ::protocol::managed_agents::Request;
type TestResult = Result<(), Box<dyn std::error::Error>>;

fn config(home: &std::path::Path) -> Result<Config, serde_json::Error> {
    serde_json::from_value(
        serde_json::json!({"namespace":"managed-service-tests", "agent":"", "device":"", "home":home,
        "continuity":home.join("anchor"), "admission_config":home.join("operator.json"),
        "postgres_key":"pg", "evidence_key":"evidence", "source_cell":"fixture", "relay_url":"http://127.0.0.1:8880",
        "relay_key":"fixture-relay", "key_version":1, "chat_credential_key":"chat", "gate":{"provider":"offline"},
        "managed_service":{"id":"01".repeat(32), "zoen_binary":"/operator/zoen", "definition":
            r#"{"version":1,"kind":"user","persona":"Help","rules":[],"state":{},"model_tier":"cheap"}"#},
        "chat_price":{"version":"chat-v1","endpoint":"https://provider.example/v1/chat/completions","model":"fixture",
            "credential_ref":"chat","currency":"USD","scale":6,"input_units_per_million":1000000,"output_units_per_million":1000000,
            "max_billable_input_tokens":100,"max_output_tokens":10,"max_request_bytes":32768,"max_response_bytes":8192}}),
    )
}

#[test]
fn managed_service_scope_and_homes_are_private_and_request_bound() -> TestResult {
    let root = std::env::temp_dir().join(types::new_id("managed-service-homes")?);
    let mut builder = std::fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&root)?;
    let mut config = config(&root)?;
    let service = service(&config)?;
    let request = Request {
        operation: "02".repeat(32),
        service: Some(service),
        name: "Helper".into(),
        handle: "helper".into(),
        recipient_public_key: "03".repeat(32),
    };
    let record = Record {
        request: Some(request),
        owner: "04".repeat(32),
        state: "queued".into(),
        agent: String::new(),
        offer_digest: String::new(),
        sealed_offer: vec![],
        expires_ms: 0,
    };
    let homes = homes::Homes::prepare(&config, &record)?;
    let stored = std::fs::read(homes.root.join("request.json"))?;
    assert_eq!(homes::Homes::prepare(&config, &record)?.root, homes.root);
    let mut altered = record.clone();
    altered.request.as_mut().ok_or("request")?.name = "Changed".into();
    assert!(homes::Homes::prepare(&config, &altered).is_err());
    altered.request.as_mut().ok_or("request")?.operation = "../foreign".into();
    assert!(homes::Homes::prepare(&config, &altered).is_err());
    assert_eq!(std::fs::read(homes.root.join("request.json"))?, stored);
    config.agent = "05".repeat(32);
    assert!(super::service(&config).is_err());
    config.agent.clear();
    config.supervised = true;
    assert!(super::service(&config).is_err());
    config.supervised = false;
    config.admission_config = None;
    assert!(super::service(&config).is_err());
    #[cfg(unix)]
    {
        use std::os::unix::fs::{symlink, PermissionsExt};
        std::fs::set_permissions(&homes.executor, std::fs::Permissions::from_mode(0o777))?;
        assert!(homes::Homes::prepare(&config, &record).is_err());
        std::fs::remove_dir(&homes.executor)?;
        let foreign = root.join("foreign");
        std::fs::create_dir(&foreign)?;
        std::fs::write(foreign.join("preserve"), b"original")?;
        symlink(&foreign, &homes.executor)?;
        assert!(homes::Homes::prepare(&config, &record).is_err());
        assert_eq!(std::fs::read(foreign.join("preserve"))?, b"original");
    }
    std::fs::remove_dir_all(root)?;
    Ok(())
}

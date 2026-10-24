use super::*;
use std::collections::BTreeMap;
use zoen_models::{AttemptContext, ImageRequest, ModelGateway};

#[derive(Default)]
struct Vault(BTreeMap<String, Vec<u8>>);
impl SecretVault for Vault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.get(&key).cloned()
    }
    fn save(&self, _: String, _: Vec<u8>) -> bool {
        panic!("the host must not change the provider credential")
    }
    fn delete(&self, _: String) {
        panic!("the host must not change the provider credential")
    }
}
fn config() -> ImageHostConfig {
    ImageHostConfig {
        agent: "trusted-agent".into(),
        device: "trusted-device".into(),
        definition_version: "trusted-image-v1".into(),
        price: crate::tests::image_tasks::profile(),
        credential_key: "operator-vault-image-key".into(),
        timeout: Duration::from_secs(3),
        connect_timeout: Duration::from_secs(1),
        target: ImageDeliveryTarget::configured(
            "http://127.0.0.1:1".into(),
            "uncontacted".into(),
            "same-cell".into(),
        )
        .unwrap(),
    }
}

#[test]
fn trusted_host_derives_exact_review_and_resolves_only_server_vault_locator() {
    let c = config();
    let d = definition(&c).unwrap();
    assert_eq!(d.price_digest, c.price.digest().unwrap());
    assert_eq!(d.size, "1024x1024");
    assert_eq!(d.quality, "low");
    assert_eq!(d.background, "opaque");
    assert_eq!(d.max_image_bytes, c.price.image.max_image_bytes);
    assert_eq!(
        gateway(&c, &Vault::default()).unwrap_err(),
        RuntimeError::Unavailable
    );
    let vault = Vault(BTreeMap::from([
        (
            c.price.credential_ref.clone(),
            b"must-not-use-model-ref-as-key".to_vec(),
        ),
        (
            c.credential_key.clone(),
            b"controlled-vault-secret".to_vec(),
        ),
    ]));
    let g = gateway(&c, &vault).unwrap();
    assert_eq!(g.api_key, "controlled-vault-secret");
    assert_eq!(g.credential_ref, c.price.credential_ref);
    assert_eq!(g.base_url, "https://api.openai.com/v1");
    assert!(!format!("{g:?}").contains("controlled-vault-secret"));
    let model = ModelGateway::new(g).unwrap();
    let dispatch = model
        .preflight_image(&ImageRequest {
            context: AttemptContext {
                run_id: "run".into(),
                attempt_id: "attempt".into(),
                owner: "owner".into(),
                agent: c.agent.clone(),
                device: c.device.clone(),
                space: "space".into(),
                authority_version: "frontier".into(),
                definition_version: c.definition_version.clone(),
                price_version: c.price.version.clone(),
            },
            prompt: "A user's requested picture".into(),
            size: c.price.image.size,
            quality: c.price.image.quality,
            background: c.price.image.background,
            max_image_bytes: c.price.image.max_image_bytes,
        })
        .unwrap();
    assert_eq!(dispatch.descriptor.endpoint, c.price.endpoint);
    assert_eq!(dispatch.descriptor.requested_output_tokens, 0);
    c.price.quote(&dispatch).unwrap();
}

#[test]
fn invalid_operator_definition_refuses_before_credential_lookup() {
    for case in 0..5 {
        let mut c = config();
        match case {
            0 => c.definition_version = "bad version".into(),
            1 => c.timeout = Duration::ZERO,
            2 => c.connect_timeout = c.timeout + Duration::from_secs(1),
            3 => c.price.endpoint = "http://127.0.0.1/v1/images/generations".into(),
            4 => c.credential_key.push('\n'),
            _ => unreachable!(),
        }
        assert_eq!(
            gateway(&c, &Vault::default()).unwrap_err(),
            RuntimeError::InvalidProfile
        );
    }
}

#[test]
fn explicit_fixture_endpoint_policy_matches_gateway_and_signed_quote() {
    let mut c = config();
    c.price.endpoint = "http://127.0.0.1:32101/v1/images/generations".into();
    assert!(definition(&c).is_err());
    c.price.endpoint_policy = ImageEndpointPolicy::LoopbackFixture;
    let reviewed = definition(&c).unwrap();
    let vault = Vault(BTreeMap::from([(
        c.credential_key.clone(),
        b"unpaid-fixture-key".to_vec(),
    )]));
    let g = gateway(&c, &vault).unwrap();
    assert_eq!(
        g.endpoint_policy,
        zoen_models::EndpointPolicy::LoopbackFixture
    );
    ModelGateway::new(g).unwrap();
    c.price.endpoint_policy = ImageEndpointPolicy::Https;
    c.price.endpoint = "https://api.openai.com/v1/images/generations".into();
    assert_ne!(reviewed.price_digest, definition(&c).unwrap().price_digest);
}

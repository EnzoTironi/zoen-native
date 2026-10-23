//! Frozen v1 wire bytes stay unchanged when callers propagate serialization errors.
use super::*;

fn publication() -> SignedMutation {
    SignedMutation {
        owner_id: "human".into(),
        device_id: "phone".into(),
        at_ms: 23,
        mutation: Mutation {
            schema: SCHEMA,
            public_origin: "https://site.test".into(),
            site_id: "site_example".into(),
            request_id: "request_example".into(),
            expected_head: Some("prior".into()),
            change: Change::Unpublish,
        },
        signature: String::new(),
    }
}

#[test]
fn fallible_publication_preserves_v1_signing_bytes_intent_and_receipt() -> serde_json::Result<()> {
    let signed = publication();
    assert_eq!(signed.signing_bytes("relay")?, br#"["zoen-site-publication.v1","relay","human","phone",23,{"schema":1,"public_origin":"https://site.test","site_id":"site_example","request_id":"request_example","expected_head":"prior","change":{"kind":"unpublish"}}]"#);
    let digest = "457bb327279f277c08658bf4f71019cc63708a9dd060ea9862c8fc99f5f10c71";
    assert_eq!(signed.intent_digest("relay")?, digest);
    let receipt = Receipt::for_mutation(&signed, "relay")?;
    assert_eq!(receipt.head, digest);
    assert_eq!(receipt.previous_head.as_deref(), Some("prior"));
    assert!(!receipt.active);
    assert!(receipt.source.is_none());
    Ok(())
}

#[test]
fn fallible_inspection_and_site_id_preserve_their_v1_domains() -> serde_json::Result<()> {
    let signed = SignedInspection {
        owner_id: "human".into(),
        device_id: "phone".into(),
        at_ms: 23,
        site_id: "site_example".into(),
        public_origin: "https://site.test".into(),
        before: Some("prior".into()),
        signature: String::new(),
    };
    assert_eq!(signed.signing_bytes("relay")?, br#"["zoen-site-inspection.v1","relay","human","phone",23,"site_example","https://site.test","prior"]"#);
    assert_eq!(
        site_id("human", "space", "item", "https://site.test")?,
        "site_8f433618aeff3fdb49273c71f3cb1006"
    );
    Ok(())
}

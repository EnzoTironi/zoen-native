//! Flat request and strict-field contracts remain stable when commands are composed.
use super::*;

#[tokio::test]
async fn forwarding_command_composition_preserves_flat_review_confirm_and_strict_fields(
) -> Result<(), Box<dyn std::error::Error>> {
    let core = Client::open(":memory:".into(), "en".into())?;
    core.create_account(
        "Forward contract".into(),
        "forward_contract".into(),
        "http://127.0.0.1:1".into(),
        Arc::new(Vault::default()),
    )?;
    let source = core.create_group_with("Source".into(), vec![], crate::PrivacyDto::Closed)?;
    let target = core.create_group_with("Target".into(), vec![], crate::PrivacyDto::Closed)?;
    let first = core.send_message(source.clone(), "First".into())?;
    let second = core.send_message(source.clone(), "Second".into())?;
    let owner = core.account().ok_or("account")?;
    let scope = serde_json::json!({"expected_identity_id":owner.identity_id,"expected_device_id":owner.device_id,"source_space":source,"message_id":first.id,"target_space":target});
    let request = serde_json::json!({"op":"forward_review","scope":scope});
    let before = core.lock().store.outbox()?.len();
    for nested in [false, true] {
        let mut invalid = request.clone();
        if nested {
            invalid["scope"]["unrecognized"] = true.into();
        } else {
            invalid["unrecognized"] = true.into();
        }
        assert!(core.client_request(invalid.to_string()).await.is_err());
        assert_eq!(core.lock().store.outbox()?.len(), before);
    }
    let review: serde_json::Value =
        serde_json::from_str(&core.client_request(request.to_string()).await?)?;
    let confirmation = serde_json::json!({"op":"confirm_forward","scope":scope,"operation_id":review["operation_id"],"source_revision":review["source_revision"]});
    let accepted: serde_json::Value =
        serde_json::from_str(&core.client_request(confirmation.to_string()).await?)?;
    assert!(accepted["accepted_message_id"].is_string());
    assert_eq!(
        core.client_request(confirmation.to_string()).await?,
        core.client_request(confirmation.to_string()).await?
    );
    assert_eq!(core.lock().store.outbox()?.len(), before + 1);
    let source_scope = serde_json::json!({"expected_identity_id":owner.identity_id,"expected_device_id":owner.device_id,"source_space":source,"message_id":first.id});
    let status: serde_json::Value = serde_json::from_str(
        &core
            .client_request(
                serde_json::json!({"op":"forward_status","source":source_scope}).to_string(),
            )
            .await?,
    )?;
    assert_eq!(status, accepted);
    core.client_request(serde_json::json!({"op":"dismiss_forward","scope":scope,"operation_id":accepted["operation_id"],"accepted_message_id":accepted["accepted_message_id"]}).to_string()).await?;
    assert_eq!(core.lock().store.outbox()?.len(), before + 1);
    let batch = serde_json::json!({"expected_identity_id":owner.identity_id,"expected_device_id":owner.device_id,"source_space":source,"message_ids":[first.id,second.id]});
    let reviewed:serde_json::Value=serde_json::from_str(&core.client_request(serde_json::json!({"op":"forward_batch_review","scope":{"source":batch,"target_space":target}}).to_string()).await?)?;
    let accepted:serde_json::Value=serde_json::from_str(&core.client_request(serde_json::json!({"op":"confirm_forward_batch","source":batch,"batch_id":reviewed["batch_id"],"review_revision":reviewed["review_revision"]}).to_string()).await?)?;
    assert_eq!(accepted["messages"].as_array().ok_or("batch")?.len(), 2);
    assert!(accepted["messages"]
        .as_array()
        .ok_or("batch")?
        .iter()
        .all(|r| r["accepted_message_id"].is_string()));
    assert_eq!(core.lock().store.outbox()?.len(), before + 3);
    Ok(())
}

#[test]
fn forwarding_contracts_reject_nested_envelopes_duplicate_tags_and_foreign_fields() {
    for input in [
        r#"{"Forwarding":{"op":"forward_status","source":{}}}"#,
        r#"{"op":"forward_status","op":"snapshot","source":{}}"#,
        r#"{"op":"snapshot","source":{}}"#,
        r#"{"op":"unrecognized_forward","source":{}}"#,
    ] {
        assert!(serde_json::from_str::<Command>(input).is_err(), "{input}");
    }
}

#[test]
fn forwarding_options_commands_require_exact_boolean_flags_and_reject_foreign_fields() {
    let single = serde_json::json!({"op":"set_forward_options","scope":{"expected_identity_id":"alice","expected_device_id":"phone","source_space":"source","message_id":"message","target_space":"target"},"operation_id":"operation","source_revision":"revision","options":{"hide_author":true,"hide_caption":false}});
    let batch_source = serde_json::json!({"expected_identity_id":"alice","expected_device_id":"phone","source_space":"source","message_ids":["message"]});
    let batch = serde_json::json!({"op":"set_forward_batch_options","source":batch_source,"batch_id":"batch","review_revision":"revision","options":{"hide_author":true,"hide_caption":false}});
    for valid in [single, batch] {
        assert!(serde_json::from_value::<Command>(valid.clone()).is_ok());
        for options in [
            serde_json::json!({"hide_author":true}),
            serde_json::json!({"hide_author":"true","hide_caption":false}),
            serde_json::json!({"hide_author":true,"hide_caption":0}),
            serde_json::json!({"hide_author":true,"hide_caption":false,"foreign":true}),
            serde_json::Value::Null,
        ] {
            let mut invalid = valid.clone();
            invalid["options"] = options;
            assert!(serde_json::from_value::<Command>(invalid).is_err());
        }
        let mut foreign = valid;
        foreign["foreign"] = true.into();
        assert!(serde_json::from_value::<Command>(foreign).is_err());
    }
}

//! Fixed synthetic compatibility bytes; never a production key.
use super::*;

#[test]
fn configured_state_codec_legacy_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let key = state_key(&[7; 32]);
    let scope = Scope::enter(key);
    let value = serde_json::json!({"revision":7,"label":"state must survive"});
    let sealed = SealedCodec::to_vec(&value)?;
    println!("SYNTHETIC_STATE_KEY={}", hex::encode(key));
    println!("SYNTHETIC_SEALED={}", hex::encode(&sealed));
    assert_eq!(
        SealedCodec::from_slice::<serde_json::Value>(&sealed)?,
        value
    );
    drop(scope);
    assert!(matches!(SealedCodec::to_vec(&value), Err(SealError::NoKey)));
    Ok(())
}

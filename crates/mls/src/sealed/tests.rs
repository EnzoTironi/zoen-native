//! Fixed synthetic compatibility bytes; never a production key.
use super::*;

#[test]
fn configured_state_codec_legacy_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let key = state_key(&[7; 32])?;
    let scope = Scope::enter(Keys::derive(key)?);
    let value = serde_json::json!({"revision":7,"label":"state must survive"});
    let sealed = SealedCodec::to_vec(&value)?;
    assert_eq!(
        hex::encode(key),
        "bcece2d6f2544dda1e7694f9fcf4fb74973967f4f0561ea507bc92c5954b464c"
    );
    assert_eq!(hex::encode(&sealed), "e89473d34e339e78a036e72ba6b65980ce4791a4aeccde991178b719aa83f4383016281b0da2082e08244c6ae091e81f5a44a99a6473bc7bd6cd3bf77b5eed884392c539f1b78774b6cffedd7edae57095823b");
    assert_eq!(
        SealedCodec::from_slice::<serde_json::Value>(&sealed)?,
        value
    );
    drop(scope);
    assert!(matches!(SealedCodec::to_vec(&value), Err(SealError::NoKey)));
    Ok(())
}

#[test]
fn failed_expansion_never_replaces_active_device_scope() -> Result<(), Box<dyn std::error::Error>> {
    let scope = Scope::enter(Keys::derive(state_key(&[7; 32])?)?);
    let value = "existing canonical state";
    let before = SealedCodec::to_vec(&value)?;
    let hk = Hkdf::<Sha256>::new(Some(b"invalid-output-size"), &[9; 32]);
    assert!(matches!(
        expand_key::<8161>(&hk, b"too long"),
        Err(SealError::Derivation)
    ));
    assert_eq!(SealedCodec::to_vec(&value)?, before);
    drop(scope);
    assert!(matches!(SealedCodec::to_vec(&value), Err(SealError::NoKey)));
    Ok(())
}

#[test]
fn nested_device_scope_restores_exact_original_ciphertext() -> Result<(), Box<dyn std::error::Error>>
{
    let outer = Scope::enter(Keys::derive(state_key(&[7; 32])?)?);
    let before = SealedCodec::to_vec(&"original device")?;
    {
        let _inner = Scope::enter(Keys::derive(state_key(&[8; 32])?)?);
        assert!(matches!(
            SealedCodec::from_slice::<String>(&before),
            Err(SealError::Open)
        ));
        assert_ne!(SealedCodec::to_vec(&"original device")?, before);
    }
    assert_eq!(SealedCodec::to_vec(&"original device")?, before);
    assert_eq!(
        SealedCodec::from_slice::<String>(&before)?,
        "original device"
    );
    drop(outer);
    assert!(matches!(
        SealedCodec::from_slice::<String>(&before),
        Err(SealError::NoKey)
    ));
    Ok(())
}

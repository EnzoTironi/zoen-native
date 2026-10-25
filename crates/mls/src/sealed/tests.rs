//! Fixed synthetic compatibility bytes; never a production key.
use super::{expand_key, state_key, Keys, Scope, SealError, SealedCodec};
use hkdf::Hkdf;
use openmls_sqlite_storage::Codec;
use serde::{Deserialize, Serialize};
use sha2::Sha256;

const LEGACY_LABEL_FIRST: &str = "e89473d34e339e78a036e72ba6b65980ce4791a4aeccde991178b719aa83f4383016281b0da2082e08244c6ae091e81f5a44a99a6473bc7bd6cd3bf77b5eed884392c539f1b78774b6cffedd7edae57095823b";
const LEGACY_REVISION_FIRST: &str = "0c2510d567edd04b9b1952eb0d26ad771921707425a60040a84f6c8dcad06025833f57730ce6fd0a754953ef6b27285f59688b3e443c8b7dbcafc08147bff4d24328e1998cbbf290e8698dffa1b50972e0129d";

// Struct field order pins plaintext bytes independently of JSON Map feature unification.
#[derive(Debug, Serialize, Deserialize, PartialEq, Eq)]
struct LegacyState {
    label: String,
    revision: u32,
}

#[test]
fn configured_state_codec_legacy_fixture() -> Result<(), Box<dyn std::error::Error>> {
    let key = state_key(&[7; 32])?;
    let scope = Scope::enter(Keys::derive(key)?);
    let value = LegacyState {
        label: "state must survive".into(),
        revision: 7,
    };
    assert_eq!(
        serde_json::to_vec(&value)?,
        br#"{"label":"state must survive","revision":7}"#
    );
    assert_eq!(
        hex::encode(key),
        "bcece2d6f2544dda1e7694f9fcf4fb74973967f4f0561ea507bc92c5954b464c"
    );
    assert_eq!(
        hex::encode(SealedCodec::to_vec(&value)?),
        LEGACY_LABEL_FIRST
    );
    // Both genuine pre-migration JSON representations must still open. These are
    // literals from the old configured codec, not freshly generated roundtrips.
    for legacy in [LEGACY_LABEL_FIRST, LEGACY_REVISION_FIRST] {
        assert_eq!(
            SealedCodec::from_slice::<LegacyState>(&hex::decode(legacy)?)?,
            value
        );
    }
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

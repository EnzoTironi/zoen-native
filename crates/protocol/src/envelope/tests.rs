use super::*;
use event_log::Signer;
use types::{Privacy, SpaceKind};

#[test]
fn unreadable_plain_content_returns_error_without_rewriting_the_original(
) -> Result<(), Box<dyn std::error::Error>> {
    let author = Author::root(Signer::from_secret(&[21; 32]));
    let event = author.sign_event(
        "space",
        "event",
        7,
        None,
        EventBody::SpaceCreated {
            title: "Original".into(),
            kind: SpaceKind::Group,
            privacy: Privacy::Closed,
        },
    )?;
    let content = event.content.clone();
    let envelope = Envelope::plain(&event)?;
    assert_eq!(envelope.content(), content);
    assert_eq!(envelope.sig, event.sig);
    envelope.verify()?;
    let mut invalid = event.clone();
    invalid.seq = 42;
    invalid.content = vec![0xff];
    assert_eq!(
        Envelope::plain(&invalid),
        Err(LogError::BadContent { seq: 42 })
    );
    assert_eq!(event.content, content);
    Ok(())
}

#[test]
fn fallible_sealed_constructor_keeps_the_exact_author_signature_and_content(
) -> Result<(), Box<dyn std::error::Error>> {
    let author = Author::root(Signer::from_secret(&[22; 32]));
    let sealed = Sealed::new(SealedKind::Application, 1, b"ciphertext".to_vec());
    let (content, signature) = author.sign_sealed("space", "event", 7, None, sealed.clone());
    let envelope = Envelope::sealed(&author, "space", "event", 7, None, sealed)?;
    assert_eq!(envelope.content(), content);
    assert_eq!(envelope.sig, signature);
    envelope.verify()?;
    Ok(())
}

use super::*;
use event_log::{Author, Signer};

fn proposal(author: &Author, space: &str, id: &str, body: EventBody) -> Envelope {
    Envelope::plain(&author.sign_event(space, id, 1, None, body).unwrap()).unwrap()
}
fn genesis() -> EventBody {
    EventBody::SpaceCreated {
        title: String::new(),
        kind: SpaceKind::Direct,
        privacy: Privacy::EndToEnd,
    }
}
#[test]
fn saved_messages_same_owner_device_candidates_can_only_take_the_first_append() {
    let root = Signer::from_secret(&[41; 32]);
    let phone = Author::device(&root, Signer::from_secret(&[42; 32]));
    let laptop = Author::device(&root, Signer::from_secret(&[43; 32]));
    let space = types::saved_messages::space_id(&root.id()).unwrap();
    let first = proposal(&phone, &space, "phone", genesis());
    let second = proposal(&laptop, &space, "laptop", genesis());
    assert_ne!(first.content(), second.content());
    assert_eq!(
        admit(&first, &Facts::default()).unwrap(),
        Effect::Create {
            kind: SpaceKind::Direct,
            privacy: Privacy::EndToEnd
        }
    );
    let committed = Facts {
        head: Some((0, "canonical".into())),
        author_role: Some(Role::Owner),
        kind: Some(SpaceKind::Direct),
        privacy: Some(Privacy::EndToEnd),
        member_count: 1,
        ..Facts::default()
    };
    assert_eq!(
        admit(&second, &committed).unwrap_err().reason,
        "space already exists"
    );
}
#[test]
fn saved_messages_namespace_cannot_be_squatted_downgraded_renamed_or_shared() {
    let root = Signer::from_secret(&[44; 32]);
    let author = Author::device(&root, Signer::from_secret(&[45; 32]));
    let other = Author::device(
        &Signer::from_secret(&[46; 32]),
        Signer::from_secret(&[47; 32]),
    );
    let space = types::saved_messages::space_id(&root.id()).unwrap();
    assert!(admit(
        &proposal(&other, &space, "squat", genesis()),
        &Facts::default()
    )
    .is_err());
    for body in [
        EventBody::SpaceCreated {
            title: "Saved Messages".into(),
            kind: SpaceKind::Direct,
            privacy: Privacy::EndToEnd,
        },
        EventBody::SpaceCreated {
            title: String::new(),
            kind: SpaceKind::Direct,
            privacy: Privacy::Closed,
        },
        EventBody::MemberAdded {
            identity: other.identity.clone(),
            role: Role::Reader,
        },
        EventBody::MemberRemoved {
            identity: root.id(),
        },
    ] {
        assert!(admit(&proposal(&author, &space, "wrong", body), &Facts::default()).is_err());
    }
}

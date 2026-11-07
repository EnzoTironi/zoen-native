use super::*;
use crate::space_invites::{Action, Rename, Request, SpaceInvitePage, SpaceInviteRecord};
#[test]
fn management_actions_and_reply_round_trip_and_invalid_metadata_is_refused(
) -> Result<(), Box<dyn std::error::Error>> {
    let id = "ab".repeat(32);
    for action in [
        Action::List(String::new()),
        Action::List(id.clone()),
        Action::Revoke(id.clone()),
        Action::Rename(Rename {
            id: id.clone(),
            name: "Família".into(),
        }),
    ] {
        let frame = ClientFrame::Req {
            id: 12,
            op: Op::SpaceInvites(Request {
                space: "sp".into(),
                action: Some(action),
            }),
        };
        assert_eq!(
            ClientFrame::decode(&frame.encode()).map_err(|e| e.to_string())?,
            frame
        );
    }
    for action in [
        None,
        Some(Action::Revoke("secret-code".into())),
        Some(Action::List("not-a-hash".into())),
        Some(Action::Rename(Rename {
            id: id.clone(),
            name: "bad\nname".into(),
        })),
        Some(Action::Rename(Rename {
            id: id.clone(),
            name: "a".repeat(65),
        })),
    ] {
        let frame = ClientFrame::Req {
            id: 12,
            op: Op::SpaceInvites(Request {
                space: "sp".into(),
                action,
            }),
        };
        assert!(ClientFrame::decode(&frame.encode()).is_err());
    }
    let reply = ServerFrame::Res {
        id: 12,
        result: Ok(Reply::SpaceInvites(SpaceInvitePage {
            entries: vec![SpaceInviteRecord {
                id,
                name: "Família".into(),
                creator: "person".into(),
                role: "Member".into(),
                expires_at_ms: 12345,
                max_uses: 10,
                uses: 2,
                revoked: false,
            }],
            next: String::new(),
            indexing: false,
        })),
    };
    assert_eq!(
        ServerFrame::decode(&reply.encode()).map_err(|e| e.to_string())?,
        reply
    );
    Ok(())
}
#[test]
fn invitation_pages_do_not_accept_partial_index_or_invalid_counts_as_complete() {
    let mut page = SpaceInvitePage {
        entries: vec![SpaceInviteRecord {
            id: "ab".repeat(32),
            name: String::new(),
            creator: "person".into(),
            role: "Member".into(),
            expires_at_ms: 10,
            max_uses: 1,
            uses: 0,
            revoked: false,
        }],
        next: String::new(),
        indexing: false,
    };
    assert!(page.validate().is_ok());
    page.indexing = true;
    assert!(page.validate().is_err());
    page.indexing = false;
    page.entries[0].uses = 2;
    assert!(page.validate().is_err());
    page.entries[0].uses = 0;
    page.entries[0].revoked = true;
    assert!(page.validate().is_err());
    page.entries[0].expires_at_ms = 0;
    assert!(page.validate().is_ok());
    page.next = "ff".repeat(32);
    assert!(page.validate().is_err());
}

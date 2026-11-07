use super::*;
use crate::{managed_agents::*, AgentRequest};

fn request() -> Request {
    Request {
        operation: "11".repeat(32),
        service: Some(Service {
            id: "22".repeat(32),
            template_sha256: "33".repeat(32),
        }),
        name: "Assistant".into(),
        handle: "my_assistant".into(),
        recipient_public_key: "44".repeat(32),
    }
}

#[test]
fn managed_requests_and_ciphertext_receipts_roundtrip_without_grant_fields(
) -> Result<(), Box<dyn std::error::Error>> {
    let request = request();
    assert!(request.valid());
    for req in [
        AgentRequest::ManagedService,
        AgentRequest::DiscoverManaged {
            after: String::new(),
        },
        AgentRequest::DiscoverManaged {
            after: "01".repeat(32),
        },
        AgentRequest::RecoverManaged {
            operation: request.operation.clone(),
        },
        AgentRequest::CompleteManaged {
            request: request.clone(),
            agent: "66".repeat(32),
        },
        AgentRequest::PrepareManaged(request.clone()),
        AgentRequest::CancelManaged(request.clone()),
        AgentRequest::ManagedStatus {
            service: "22".repeat(32),
            operation: request.operation.clone(),
        },
    ] {
        let frame = ClientFrame::Req {
            id: 17,
            op: Op::Agent(req),
        };
        assert_eq!(
            ClientFrame::decode(&frame.encode()).map_err(|e| e.to_string())?,
            frame
        );
    }
    let receipt = Record {
        request: Some(request.clone()),
        owner: "55".repeat(32),
        state: "offered".into(),
        agent: "66".repeat(32),
        offer_digest: "77".repeat(32),
        sealed_offer: vec![81; 256],
        expires_ms: 1234,
        onboarding_completed: false,
    };
    for reply in [
        Reply::ManagedService(Availability {
            service: request.service.clone(),
        }),
        Reply::ManagedService(Availability { service: None }),
        Reply::ManagedAgents(Page {
            records: vec![receipt.clone()],
            next_operation: String::new(),
        }),
        Reply::ManagedAgent(receipt.clone()),
        Reply::ManagedAgent(Record {
            state: "cancel_requested".into(),
            ..receipt.clone()
        }),
        Reply::ManagedAgent(Record {
            state: "cancelled".into(),
            ..receipt.clone()
        }),
        Reply::ManagedAgent(Record {
            state: "cancelled".into(),
            agent: String::new(),
            offer_digest: String::new(),
            sealed_offer: vec![],
            expires_ms: 0,
            ..receipt
        }),
    ] {
        let frame = ServerFrame::Res {
            id: 17,
            result: Ok(reply),
        };
        assert_eq!(
            ServerFrame::decode(&frame.encode()).map_err(|e| e.to_string())?,
            frame
        );
    }
    Ok(())
}

#[test]
fn invalid_managed_metadata_and_receipt_states_fail_closed() {
    for field in ["handle", "service", "operation", "name", "recipient"] {
        let mut req = request();
        match field {
            "service" => req.service = None,
            "operation" => req.operation = "-".into(),
            "name" => req.name = " ".into(),
            "recipient" => req.recipient_public_key = "11".repeat(33),
            _ => req.handle = "bad handle".into(),
        }
        assert!(!req.valid(), "{field}");
        let frame = ClientFrame::Req {
            id: 17,
            op: Op::Agent(AgentRequest::PrepareManaged(req)),
        };
        assert!(ClientFrame::decode(&frame.encode()).is_err(), "{field}");
    }
    for state in ["ready", "approved", "registered"] {
        let receipt = Record {
            request: Some(request()),
            owner: "55".repeat(32),
            state: state.into(),
            agent: String::new(),
            offer_digest: String::new(),
            sealed_offer: Vec::new(),
            expires_ms: 0,
            onboarding_completed: false,
        };
        let frame = ServerFrame::Res {
            id: 17,
            result: Ok(Reply::ManagedAgent(receipt)),
        };
        assert!(ServerFrame::decode(&frame.encode()).is_err(), "{state}");
    }
}

#[test]
fn managed_discovery_rejects_ambiguous_pages_and_bad_cursors() {
    let r = Record {
        request: Some(request()),
        owner: "55".repeat(32),
        state: "queued".into(),
        agent: String::new(),
        offer_digest: String::new(),
        sealed_offer: vec![],
        expires_ms: 0,
        onboarding_completed: false,
    };
    for page in [
        Page {
            records: vec![r.clone(), r.clone()],
            next_operation: String::new(),
        },
        Page {
            records: vec![r.clone(); 33],
            next_operation: String::new(),
        },
        Page {
            records: vec![r],
            next_operation: "11".repeat(32),
        },
    ] {
        assert!(!page.valid());
        assert!(ServerFrame::decode(
            &ServerFrame::Res {
                id: 1,
                result: Ok(Reply::ManagedAgents(page))
            }
            .encode()
        )
        .is_err());
    }
    for req in [
        AgentRequest::DiscoverManaged {
            after: "bad".into(),
        },
        AgentRequest::RecoverManaged {
            operation: String::new(),
        },
        AgentRequest::CompleteManaged {
            request: request(),
            agent: "bad".into(),
        },
    ] {
        assert!(ClientFrame::decode(
            &ClientFrame::Req {
                id: 1,
                op: Op::Agent(req)
            }
            .encode()
        )
        .is_err());
    }
}

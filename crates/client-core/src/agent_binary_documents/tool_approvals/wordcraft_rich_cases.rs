//! Real canonical review/CAS/encryption, with the shared upstream rich DOCX model.
use super::*;
use crate::agent_binary_documents::tests::fixture_binary;
use std::sync::Arc;
use zoen_word_profile::model::{props::Align, Block, Document};
#[test]
fn reviewed_rich_word_operations_preserve_style_bytes_versions_and_exact_intent(
) -> Result<(), Box<dyn std::error::Error>> {
    let mut source = Document::from_text("Title\nUntouched body");
    source.core.title = "Owner document".into();
    let Block::Para(p) = Arc::make_mut(&mut source.body[1]) else {
        return Err("missing paragraph".into());
    };
    p.runs[0].props.italic = Some(true);
    let original = zoen_word_profile::encode(&source)?;
    for tool in ["wordcraft_format_range", "wordcraft_edit_range"] {
        let f = fixture_binary(types::Privacy::Closed, "wordcraft", &original)?;
        signed_trust(&f, TrustLevel::Act)?;
        let request = stage(&f, tool);
        assert!(f.agent.approved_tool_turn(&request.id, WORKER).is_err());
        assert_eq!(current(&f).version, 1);
        owner_decide(
            &f,
            &request.id,
            observed(&request),
            ToolRequestDecision::ApproveOnce,
        )?;
        let permit = f.agent.approved_tool_turn(&request.id, WORKER)?;
        let (_, action) = parsed(permit.proposal())?;
        let intent = command_intent(permit.proposal(), action.as_ref())?;
        let mut next = source.clone();
        let Block::Para(p) = Arc::make_mut(&mut next.body[0]) else {
            return Err("missing paragraph".into());
        };
        if tool == "wordcraft_format_range" {
            p.runs[0].props.bold = Some(true);
            p.props.align = Some(Align::Center);
        } else {
            p.text = "Reviseditle".into();
            p.runs[0].len = p.text.len();
        }
        let bytes = zoen_word_profile::encode(&next)?;
        let mutation = format!("wordcmd_{intent}");
        let before = counts(&f);
        assert!(f
            .agent
            .agent_binary_save(
                permit.turn(),
                &mutation,
                &"0".repeat(64),
                &STANDARD.encode(&bytes)
            )
            .is_err());
        assert_eq!(counts(&f), before);
        let accepted = f.agent.agent_binary_save(
            permit.turn(),
            &mutation,
            &intent,
            &STANDARD.encode(&bytes),
        )?;
        assert_eq!(
            (accepted.version, accepted.actor_kind.as_str()),
            (2, "agent")
        );
        assert_eq!(accepted.data_base64, STANDARD.encode(&bytes));
        assert_eq!(zoen_word_profile::parse(&bytes)?, next);
        assert_eq!(next.body[1], source.body[1]);
        assert_eq!(next.core, source.core);
        let repeated = f
            .agent
            .agent_binary_result(permit.turn(), &mutation, &intent)?
            .ok_or("missing durable receipt")?;
        assert_eq!(repeated.sha256, accepted.sha256);
        assert_eq!(repeated.version, 2);
        assert_eq!(current(&f).sha256, accepted.sha256);
    }
    Ok(())
}

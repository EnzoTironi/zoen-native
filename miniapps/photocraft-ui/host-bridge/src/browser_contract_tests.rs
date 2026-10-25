//! The actual Session producer keeps command intent separate from manual saves.
use super::*;
use crate::{
    Operation,
    managed_tests::{head, mutation, scope, seed},
};
use sha2::Digest;

#[test]
fn actual_session_proposals_keep_exact_command_and_manual_checkpoint_kinds() -> Result<()> {
    let source = seed()?;
    let mut session = Session::new();
    let mut bridge = BrowserBridge::default();
    bridge.open(
        &mut session,
        Open {
            scope: scope(),
            head: head("1", &source),
        },
        &source,
    )?;
    let checkpoint = bridge.snapshot(&mut session, mutation(&source, "manual"))?;
    assert_eq!(
        checkpoint.metadata["operation"],
        json!({"kind":"humanCheckpoint"})
    );
    bridge.accept(
        &mut session,
        &Accept {
            scope: scope(),
            mutation_id: "manual".into(),
            head: head("2", &checkpoint.bytes),
        },
    )?;
    let command = Command {
        mutation: Mutation {
            scope: scope(),
            base: head("2", &checkpoint.bytes),
            mutation_id: "rotate".into(),
        },
        action: Operation::RotateClockwise,
    };
    let proposal = bridge.execute(&mut session, command.clone())?;
    assert_eq!(
        proposal.metadata["operation"],
        json!({"kind":"command","action":"rotate_clockwise"})
    );
    assert_eq!(
        proposal.metadata["base"],
        json!(head("2", &checkpoint.bytes))
    );
    assert_eq!(proposal.metadata["scope"], json!(scope()));
    assert_eq!(
        bridge.execute(&mut session, command)?.metadata,
        proposal.metadata
    );
    let view = crate::inspect(
        &proposal.bytes,
        &sha2::Sha256::digest(&proposal.bytes).into(),
    )
    .map_err(|_| failure("proposal_inspection"))?;
    assert_eq!((view.width, view.height), (240, 320));
    // Accepted state remains the exact checkpoint until the scoped next-version ACK.
    assert_eq!(
        bridge
            .export(
                &session,
                &Export {
                    scope: scope(),
                    head: head("2", &checkpoint.bytes),
                    format: managed::ExportFormat::Pcraft,
                }
            )?
            .bytes,
        checkpoint.bytes
    );
    let receipt = Accept {
        scope: scope(),
        mutation_id: "rotate".into(),
        head: head("3", &proposal.bytes),
    };
    bridge.accept(&mut session, &receipt)?;
    assert_eq!(
        bridge
            .export(
                &session,
                &Export {
                    scope: scope(),
                    head: receipt.head,
                    format: managed::ExportFormat::Pcraft,
                }
            )?
            .bytes,
        proposal.bytes
    );
    Ok(())
}

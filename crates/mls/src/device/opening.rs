//! Ordered application/commit opening and confirmed local-commit merge.
use super::{Device, Provider};
use crate::group::{check_roster, identities, storage_error, unlisted, Opened};
use crate::{leaf_of, mls, Leaf, MlsError};
use openmls::prelude::{tls_codec::Deserialize as _, *};
use std::collections::BTreeSet;

impl<'c> Device<'c> {
    /// Processes one sealed Application or Commit from the log, in log order. `roster` is
    /// the log's member identities after this point; a commit leaving any other roster is
    /// refused and not applied.
    /// # Errors
    /// Refuses invalid message framing, authentication, unlisted/admitted sender mismatch or storage/merge failures. Stale and missing membership remain explicit outcomes.
    pub fn open(
        &self,
        space: &str,
        data: &[u8],
        roster: &BTreeSet<String>,
        admitted_sender: &Leaf,
    ) -> Result<Opened, MlsError> {
        self.with(|p| {
            let mut group = match Self::load(p, space) {
                Ok(group) => group,
                Err(MlsError::NoGroup) => return Ok(Opened::NotMember),
                Err(error) => return Err(error),
            };
            if !group.is_active() {
                return Ok(Opened::NotMember);
            }
            let Some((kind, epoch, processed)) = process(p, &mut group, data)? else {
                return Ok(Opened::Stale);
            };
            let sender = processed.sender().clone();
            match processed.into_content() {
                ProcessedMessageContent::ApplicationMessage(m) => {
                    let Sender::Member(index) = sender else {
                        return Err(MlsError::Mls(
                            "application message from a non-member".into(),
                        ));
                    };
                    let member = group
                        .members()
                        .find(|m| m.index == index)
                        .ok_or(MlsError::Credential)?;
                    Ok(Opened::Application {
                        plaintext: m.into_bytes(),
                        from: leaf_of(&member.credential, &member.signature_key)?,
                    })
                }
                ProcessedMessageContent::StagedCommitMessage(staged) => {
                    if sender == Sender::NewMemberCommit {
                        let path = staged.update_path_leaf_node().ok_or(MlsError::Credential)?;
                        let leaf = leaf_of(path.credential(), path.signature_key().as_slice())?;
                        unlisted(BTreeSet::from([leaf.identity.clone()]), roster)?;
                        if leaf.identity != admitted_sender.identity
                            || leaf.device != admitted_sender.device
                        {
                            return Err(MlsError::Credential);
                        }
                    }
                    let reconcile = match check_roster(&group, &staged, roster) {
                        Ok(()) => false,
                        Err(MlsError::Unlisted(extra))
                            if sender == Sender::NewMemberCommit
                                && extra.is_subset(&identities(&group)?) =>
                        {
                            true
                        }
                        Err(err) => return Err(err),
                    };
                    let tx = self
                        .conn
                        .is_autocommit()
                        .then(|| self.conn.unchecked_transaction())
                        .transpose()
                        .map_err(|e| MlsError::Storage(e.to_string()))?;
                    group
                        .merge_staged_commit(p, *staged)
                        .map_err(|error| match error {
                            MergeCommitError::StorageError(error) => storage_error(error),
                            error @ MergeCommitError::LibraryError(_) => mls(error),
                        })?;
                    self.set_reconciliation(space, reconcile)?;
                    let epoch = group.epoch().as_u64();
                    if !group.is_active() {
                        // Our leaf was taken out (a removal, or a stranded leaf replaced):
                        // the secrets go, and a later Welcome starts the group afresh.
                        group.delete(p.storage()).map_err(storage_error)?;
                    }
                    if let Some(tx) = tx {
                        tx.commit().map_err(|e| MlsError::Storage(e.to_string()))?;
                    }
                    Ok(if reconcile {
                        Opened::Reconcile { epoch }
                    } else {
                        Opened::Commit { epoch }
                    })
                }
                ProcessedMessageContent::OwnPendingCommit => self.merge_own(p, &mut group, roster),
                // Our own PrivateMessage: undecryptable by its author. A commit of ours at the
                // current epoch is the pending one (OpenMLS allows one at a time), so it won;
                // any other is a commit that lost the race.
                ProcessedMessageContent::OwnPrivateMessage if kind == ContentType::Commit => {
                    if epoch == group.epoch() && group.pending_commit().is_some() {
                        self.merge_own(p, &mut group, roster)
                    } else {
                        Ok(Opened::Stale)
                    }
                }
                ProcessedMessageContent::OwnPrivateMessage => Ok(Opened::Own),
                ProcessedMessageContent::ProposalMessage(_)
                | ProcessedMessageContent::ExternalJoinProposalMessage(_) => Err(MlsError::Mls(
                    "standalone proposals aren't used; commits carry them".into(),
                )),
            }
        })
    }

    fn merge_own(
        &self,
        p: &Provider<'c>,
        group: &mut MlsGroup,
        roster: &BTreeSet<String>,
    ) -> Result<Opened, MlsError> {
        let staged = group.pending_commit().ok_or(MlsError::NoGroup)?;
        if let Err(e) = check_roster(group, staged, roster) {
            // Everyone refuses it alike, so drop it and stay in this epoch.
            group
                .clear_pending_commit(p.storage())
                .map_err(storage_error)?;
            return Err(e);
        }
        let tx = self
            .conn
            .is_autocommit()
            .then(|| self.conn.unchecked_transaction())
            .transpose()
            .map_err(|e| MlsError::Storage(e.to_string()))?;
        group.merge_pending_commit(p).map_err(|error| match error {
            MergePendingCommitError::MergeCommitError(MergeCommitError::StorageError(error)) => {
                storage_error(error)
            }
            error => mls(error),
        })?;
        self.set_reconciliation(
            std::str::from_utf8(group.group_id().as_slice()).map_err(|_| MlsError::Credential)?,
            false,
        )?;
        if let Some(tx) = tx {
            tx.commit().map_err(|e| MlsError::Storage(e.to_string()))?;
        }
        Ok(Opened::Commit {
            epoch: group.epoch().as_u64(),
        })
    }
}

fn process(
    p: &Provider<'_>,
    group: &mut MlsGroup,
    data: &[u8],
) -> Result<Option<(ContentType, GroupEpoch, ProcessedMessage)>, MlsError> {
    let msg =
        MlsMessageIn::tls_deserialize_exact(data).map_err(|e| MlsError::Decode(e.to_string()))?;
    let msg = msg
        .try_into_protocol_message()
        .map_err(|e| MlsError::Decode(format!("{e:?}")))?;
    let (kind, epoch) = (msg.content_type(), msg.epoch());
    let processed = match group.process_message(p, msg) {
        Ok(m) => m,
        Err(ProcessMessageError::ValidationError(ValidationError::WrongEpoch)) => return Ok(None),
        Err(ProcessMessageError::StorageError(error)) => return Err(storage_error(error)),
        // OpenMLS also collapses failed local key reads into this staging error.
        Err(error @ ProcessMessageError::InvalidCommit(StageCommitError::MissingDecryptionKey)) => {
            return Err(storage_error(error));
        }
        Err(e) => return Err(mls(e)),
    };
    Ok(Some((kind, epoch, processed)))
}

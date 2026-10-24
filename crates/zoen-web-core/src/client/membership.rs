//! Browser storage adapter for the shared MLS membership planner. Claims are
//! journaled before consuming key packages, and recovery blobs gate publication.
use super::*;
use mls::{
    leaf_name,
    reconciliation::{self, Owed},
};
use protocol::{KeyPackageClaimClock, KeyPackageRecord};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const CLAIM_REQUEST: u64 = 0x8000_0030;
const DEVICES_REQUEST: u64 = 0x8000_0031;

#[derive(Clone, Default, Serialize, Deserialize)]
pub(super) struct Journal {
    creates: BTreeMap<String, Vec<String>>,
    claims: BTreeMap<String, Claim>,
    joins: BTreeMap<String, BTreeMap<String, String>>,
    stranded: BTreeMap<String, BTreeSet<String>>,
    welcomes: BTreeMap<String, BTreeSet<String>>,
    pub(super) revoked: BTreeSet<String>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Claim {
    operation: String,
    device: String,
    epoch: u64,
    intent: Owed,
    targets: Vec<String>,
    records: Option<Vec<KeyPackageRecord>>,
}
#[derive(Default)]
pub(super) struct Network {
    pub(super) clock: Option<KeyPackageClaimClock>,
    claiming: Option<(String, String, i64)>,
    retry: BTreeMap<String, i64>,
    turns: BTreeMap<String, i64>,
    devices_requested: bool,
    devices_due: i64,
    pub(super) stuck: BTreeSet<String>,
}
impl Network {
    pub(super) fn restart(&mut self) {
        let clock = self.clock;
        *self = Self {
            clock,
            ..Default::default()
        };
    }
}
#[derive(Deserialize)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
enum Command {
    StartDirect {
        identity_id: String,
    },
    CreateGroup {
        title: String,
        member_ids: Vec<String>,
    },
    CreateCommunity {
        title: String,
    },
    AddMember {
        space_id: String,
        identity_id: String,
    },
    AddAdmin {
        space_id: String,
        identity_id: String,
    },
    RemoveMember {
        space_id: String,
        identity_id: String,
    },
    LeaveSpace {
        space_id: String,
    },
    EncryptChat {
        space_id: String,
    },
    // Internal host wake after a confirmed device revocation.
    RefreshDevices {},
}

impl<S: Signer> Client<S> {
    pub(super) fn note_membership_event(&mut self, event: &Event) -> Result<(), String> {
        let space = &event.space;
        if matches!(event.body, EventBody::SpaceCreated { .. })
            && event.author == self.account.identity
        {
            if let Some(members) = self.saved.membership.creates.remove(space) {
                for identity in members {
                    self.enqueue_control(
                        space,
                        EventBody::MemberAdded {
                            identity,
                            role: Role::Member,
                        },
                        self.view_now_ms,
                    )?;
                }
            }
        }
        if matches!(event.body, EventBody::SpaceEncrypted)
            && event.device.as_deref() == Some(self.account.device.as_str())
            && self
                .device()?
                .group_state(space)
                .map_err(|error| error.to_string())?
                == GroupState::Missing
        {
            self.device()?
                .create_group(space)
                .map_err(|error| error.to_string())?;
        }
        if let EventBody::DeviceJoining { device } = &event.body {
            self.membership_net.devices_due = 0;
            if device != &self.account.device
                && self
                    .metadata(space)
                    .is_some_and(|(_, _, privacy)| privacy == Privacy::EndToEnd)
            {
                if let Ok(leaves) = self.device()?.leaves(space) {
                    let stale = leaves
                        .get(&(event.author.clone(), device.clone()))
                        .map(hex::encode)
                        .unwrap_or_default();
                    self.saved
                        .membership
                        .joins
                        .entry(space.clone())
                        .or_default()
                        .insert(leaf_name(&event.author, device), stale);
                }
            }
        }
        self.saved.membership.welcomes.remove(&event.client_id);
        Ok(())
    }

    pub(super) fn membership_waiting(&self, space: &str) -> bool {
        self.saved
            .outbox
            .values()
            .filter(|pending| pending.failure.is_none())
            .any(|pending| {
                pending.envelope().is_ok_and(|envelope| {
                    envelope.space() == space
                        && (matches!(
                            envelope.body(),
                            Some(
                                EventBody::MemberAdded { .. }
                                    | EventBody::MemberRemoved { .. }
                                    | EventBody::SpaceEncrypted
                            )
                        ) || envelope.sealed_kind() == Some(SealedKind::Welcome))
                })
            })
    }
    fn owed_membership(&self, space: &str) -> Result<Option<Owed>, String> {
        if self.membership_waiting(space) {
            return Ok(None);
        }
        let roster = self.roster(space);
        let Some(role) = roster.get(&self.account.identity) else {
            return Ok(None);
        };
        let Some(leaves) = self
            .device()?
            .committed_leaves(space)
            .map_err(|error| error.to_string())?
        else {
            return Ok(None);
        };
        let empty = BTreeSet::new();
        let joins = BTreeMap::new();
        Ok(reconciliation::plan(reconciliation::Input {
            me: &self.account.identity,
            admin: matches!(role, Role::Owner | Role::Admin),
            reconciling: false,
            listed: &roster.into_keys().collect(),
            leaves: &leaves,
            joins: self.saved.membership.joins.get(space).unwrap_or(&joins),
            stranded: self.saved.membership.stranded.get(space).unwrap_or(&empty),
            revoked: &self.saved.membership.revoked,
        }))
    }
    fn settle_membership(&mut self, space: &str) -> Result<(), String> {
        let Some(leaves) = self
            .device()?
            .committed_leaves(space)
            .map_err(|error| error.to_string())?
        else {
            return Ok(());
        };
        let roster = self.roster(space);
        if let Some(stranded) = self.saved.membership.stranded.get_mut(space) {
            let gone = stranded
                .iter()
                .filter(|name| {
                    name.split_once('/').is_none_or(|(id, device)| {
                        !leaves.contains_key(&(id.into(), device.into()))
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            for name in gone {
                stranded.remove(&name);
                self.saved
                    .membership
                    .joins
                    .entry(space.into())
                    .or_default()
                    .entry(name)
                    .or_default();
            }
        }
        if let Some(joins) = self.saved.membership.joins.get_mut(space) {
            joins.retain(|name, stale| {
                name.split_once('/').is_some_and(|(id, device)| {
                    roster.contains_key(id)
                        && leaves
                            .get(&(id.into(), device.into()))
                            .is_none_or(|key| hex::encode(key) == *stale)
                })
            });
        }
        Ok(())
    }
    fn recovery_recipients(&self, space: &str) -> Result<Option<Vec<(String, String)>>, String> {
        let Some(mine) = self.agreement_key() else {
            return Ok(None);
        };
        let mut recipients = Vec::new();
        let mut legacy = false;
        for identity in self.roster(space).into_keys() {
            if identity == self.account.identity {
                recipients.push((identity, mine.public_hex()));
            } else if let Some(public) = self.saved.profile.agreement.get(&identity) {
                recipients.push((identity, public.clone()));
            } else if self.profile_net.no_agreement.contains(&identity) {
                legacy = true;
            } else {
                return Err("waiting for member recovery keys".into());
            }
        }
        Ok((!legacy).then_some(recipients))
    }
    fn stage_membership(
        &mut self,
        space: &str,
        packages: &[Vec<u8>],
        remove: &BTreeSet<String>,
    ) -> Result<(), String> {
        let recipients = self.recovery_recipients(space)?;
        let before = self
            .provider
            .storage()
            .values
            .read()
            .map_err(|_| "MLS storage lock failed")?
            .clone();
        self.saved
            .outbox_sequence
            .checked_add(2)
            .ok_or("Outbox order overflow.")?;
        let result = (|| {
            let commit = self
                .device()?
                .commit(space, packages, remove)
                .map_err(|error| error.to_string())?;
            let mut sealed = Sealed::new(SealedKind::Commit, SUITE_ID, commit.commit);
            let mut upload = None;
            if let Some(recipients) = recipients {
                let summary = mls::recovery::verify_group_info(
                    space,
                    &commit.group_info,
                    Some(&self.account.device),
                )
                .map_err(|error| error.to_string())?;
                if !summary
                    .identities
                    .is_subset(&self.roster(space).into_keys().collect())
                {
                    return Err("Recovery context includes an unlisted member.".into());
                }
                let recipients = recipients
                    .iter()
                    .map(|(id, key)| (id.as_str(), key.as_str()))
                    .collect::<Vec<_>>();
                let bytes = event_log::recovery::seal(
                    space,
                    summary.epoch,
                    &self.account.device,
                    &commit.group_info,
                    &recipients,
                )
                .ok_or("Could not seal recovery context.")?;
                let blob = hex::encode(Sha256::digest(&bytes));
                sealed.recovery = event_log::recovery::RecoveryRef {
                    version: 1,
                    epoch: summary.epoch,
                    blob: blob.clone(),
                }
                .encode();
                upload = Some((blob, bytes));
            }
            let mut pending = self.handshake(space, sealed, self.view_now_ms)?;
            let commit_id = pending.envelope()?.client_id().to_string();
            let welcome = commit
                .welcome
                .map(|welcome| -> Result<_, String> {
                    let pending = self.handshake(
                        space,
                        Sealed::new(SealedKind::Welcome, SUITE_ID, welcome),
                        self.view_now_ms,
                    )?;
                    let id = pending.envelope()?.client_id().to_owned();
                    let newcomers = packages
                        .iter()
                        .filter_map(|package| mls::key_package_leaf(package).ok())
                        .map(|leaf| leaf_name(&leaf.identity, &leaf.device))
                        .collect();
                    Ok((id, pending, newcomers))
                })
                .transpose()?;
            // All fallible crypto/signing/encoding is complete. Mutate the durable
            // journal in one infallible block; do not copy every retained media blob.
            let order = self.saved.outbox_sequence;
            if let Some((blob, bytes)) = upload {
                pending.blob_uploads.push(blob.clone());
                self.saved.local_chunks.insert(blob, bytes);
            }
            pending.order = Some(order + 1);
            self.saved.outbox.insert(commit_id.clone(), pending);
            self.saved.outbox_sequence = order + 1;
            if let Some((id, mut welcome, newcomers)) = welcome {
                welcome.after_commit = Some(commit_id);
                welcome.order = Some(order + 2);
                self.saved.membership.welcomes.insert(id.clone(), newcomers);
                self.saved.outbox.insert(id, welcome);
                self.saved.outbox_sequence = order + 2;
            }
            self.saved.membership.claims.remove(space);
            Ok(())
        })();
        if result.is_err() {
            *self
                .provider
                .storage()
                .values
                .write()
                .map_err(|_| "MLS storage lock failed")? = before;
        }
        result
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
#[path = "membership_tests.rs"]
mod tests;

mod transport;

mod commands;

//! Bounded projection of the actual linked-device and MLS state. No writes.
use super::*;
use crate::owner_policy::{
    WorkerConnectionState, WorkerDirectoryState as D, WorkerGroupObservation, WorkerGroupState as G,
};

fn directory_set(raw: &str) -> Option<BTreeSet<String>> {
    if raw.len() > protocol::complete_reads::DEVICE_ROWS * 70 {
        return None;
    }
    let items: Vec<String> = serde_json::from_str(raw).ok()?;
    if items.len() > protocol::complete_reads::DEVICE_ROWS
        || items
            .iter()
            .any(|id| id.len() != 64 || !id.bytes().all(|b| b.is_ascii_hexdigit()))
    {
        return None;
    }
    let n = items.len();
    let unique: BTreeSet<_> = items.into_iter().collect();
    (n == unique.len()).then_some(unique)
}
impl Engine {
    pub(crate) fn linked_group_observation(&self, space: &str) -> R<WorkerGroupObservation> {
        let account = self.account().ok_or_else(|| CoreError::Forbidden {
            reason: "Current owner-control account unavailable.".into(),
        })?;
        let (dirty, claiming, retry_scheduled) = self.mls_work_observation(space);
        let (group, leaves) = match self.device() {
            Err(_) => (G::Credential, None),
            Ok(device) => match device.group_state(space) {
                Err(_) => (G::Storage, None),
                Ok(::mls::GroupState::Missing) => (G::Missing, None),
                Ok(state) => match device.leaves(space) {
                    Ok(leaves) => (
                        if state == ::mls::GroupState::Ready {
                            G::Ready
                        } else {
                            G::Pending
                        },
                        Some(leaves),
                    ),
                    Err(::mls::MlsError::NoGroup) if state == ::mls::GroupState::Pending => {
                        (G::Pending, None)
                    }
                    Err(_) => (G::Storage, None),
                },
            },
        };
        let mut result = WorkerGroupObservation {
            device_fingerprint: event_log::sha256_hex(account.device.as_bytes()),
            group,
            directory: D::Missing,
            space_synced: self.net.synced.contains(space),
            socket: WorkerConnectionState::Unobserved,
            socket_synced: None,
            roster_lists_self: self
                .mls_roster(space)
                .is_some_and(|r| r.contains_key(&account.identity)),
            own_leaf: leaves
                .as_ref()
                .map(|l| l.contains_key(&(account.identity.clone(), account.device.clone()))),
            self_active: None,
            active_devices: None,
            revoked_devices: None,
            other_active_devices: None,
            other_active_leaves: None,
            asked_other_devices: None,
            dirty,
            claiming,
            retry_scheduled,
        };
        let (active, revoked) = match (self.store.meta(META_ACTIVE), self.store.meta(META_REVOKED))
        {
            (Ok(Some(active)), Ok(Some(revoked))) => (active, revoked),
            (Ok(_), Ok(_)) => return Ok(result),
            _ => {
                result.directory = D::Storage;
                return Ok(result);
            }
        };
        let (Some(active), Some(revoked)) = (directory_set(&active), directory_set(&revoked))
        else {
            result.directory = D::Invalid;
            return Ok(result);
        };
        if !active.is_disjoint(&revoked) {
            result.directory = D::Invalid;
            return Ok(result);
        }
        let others: Vec<_> = active
            .iter()
            .filter(|d| **d != account.device && !revoked.contains(*d))
            .collect();
        let mut asked = 0;
        for device in &others {
            match self.store.meta(&format!("link.asked:{space}:{device}")) {
                Ok(Some(_)) => asked += 1,
                Ok(None) => {}
                Err(_) => {
                    result.directory = D::Storage;
                    return Ok(result);
                }
            }
        }
        let count = |n| {
            u32::try_from(n).map_err(|_| CoreError::Invalid {
                reason: "Linked device observation exceeds its profile.".into(),
            })
        };
        result.directory = D::Complete;
        result.self_active =
            Some(active.contains(&account.device) && !revoked.contains(&account.device));
        result.active_devices = Some(count(active.len())?);
        result.revoked_devices = Some(count(revoked.len())?);
        result.other_active_devices = Some(count(others.len())?);
        result.asked_other_devices = Some(asked);
        result.other_active_leaves = leaves
            .as_ref()
            .map(|l| {
                count(
                    others
                        .iter()
                        .filter(|d| l.contains_key(&(account.identity.clone(), (*d).to_string())))
                        .count(),
                )
            })
            .transpose()?;
        Ok(result)
    }
}

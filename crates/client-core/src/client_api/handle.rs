//! Handle changes are online directory requests. The local profile changes only
//! after the relay acknowledges the exact requested handle for this account.
use super::*;
use crate::{engine::Engine, sync::AccountMeta};
use protocol::{Op, Reply};
use std::{
    collections::HashSet,
    sync::{Mutex, OnceLock},
};
use types::{Identity, IdentityKind};

// An ephemeral in-flight fence, not an offline rename queue. Re-entrant calls are
// refused so two acknowledgements cannot overwrite each other out of order.
static CHANGING: OnceLock<Mutex<HashSet<usize>>> = OnceLock::new();
struct InFlight(usize);
impl InFlight {
    fn begin(core: &Client) -> Result<Self, CoreError> {
        let key = Arc::as_ptr(&core.inner) as usize;
        if !CHANGING
            .get_or_init(Mutex::default)
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .insert(key)
        {
            return Err(invalid("A username change is already in progress."));
        }
        Ok(Self(key))
    }
}
impl Drop for InFlight {
    fn drop(&mut self) {
        CHANGING
            .get()
            .unwrap()
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(&self.0);
    }
}
struct Change {
    account: AccountMeta,
    profile: Identity,
}
fn prepare(engine: &Engine, handle: &str) -> Result<Change, CoreError> {
    let handle = protocol::normalize_handle(handle)
        .ok_or_else(|| invalid("Use 3–24 letters, digits, '.' or '_', starting with a letter."))?;
    let account = engine
        .account()
        .cloned()
        .ok_or_else(|| invalid("Sign in before changing your username."))?;
    if !account.registered || !engine.is_unlocked() || engine.provider_setup_pending() {
        return Err(invalid("Finish signing in before changing your username."));
    }
    let mut profile = engine
        .my_profile()
        .ok_or_else(|| invalid("Account profile unavailable."))?;
    if profile.kind != IdentityKind::Person {
        return Err(invalid("This action is for your personal account."));
    }
    profile.handle = handle;
    profile.name.clear();
    profile.bio.clear();
    Ok(Change { account, profile })
}
fn acknowledged(
    engine: &mut Engine,
    change: &Change,
    reply: Reply,
) -> Result<AccountDto, CoreError> {
    let Reply::Registered(profile) = reply else {
        return Err(invalid("The relay did not confirm your username."));
    };
    if profile.id != change.account.identity
        || profile.handle != change.profile.handle
        || profile.kind != IdentityKind::Person
    {
        return Err(invalid(
            "The relay confirmed a different account or username.",
        ));
    }
    let account = engine
        .account()
        .cloned()
        .ok_or_else(|| invalid("The account changed while saving."))?;
    if account.identity != change.account.identity
        || account.device != change.account.device
        || account.relay_url != change.account.relay_url
    {
        return Err(invalid("The account changed while saving."));
    }
    let mut current = engine
        .my_profile()
        .ok_or_else(|| invalid("Account profile unavailable."))?;
    current.handle = profile.handle;
    // Read fresh private fields after the await. A parallel encrypted profile/photo
    // edit survives this directory-only acknowledgement, with its version untouched.
    engine.store.put_identity(&current, None)?;
    engine
        .identities
        .insert(current.id.clone(), current.clone());
    engine.net.wake();
    Ok(AccountDto {
        identity_id: account.identity,
        device_id: account.device,
        name: current.name,
        handle: current.handle,
        relay_url: account.relay_url,
        registered: account.registered,
        unlocked: engine.is_unlocked(),
        setup_pending: engine.provider_setup_pending(),
    })
}
impl Client {
    pub(super) async fn client_update_handle(
        &self,
        handle: String,
    ) -> Result<AccountDto, CoreError> {
        let _in_flight = InFlight::begin(self)?;
        let change = prepare(&self.lock(), &handle)?;
        let connection = self.connection();
        if connection.state != "online" || !connection.synced {
            return Err(invalid(
                "Connect to the relay before changing your username.",
            ));
        }
        let reply = self
            .request(Op::Register {
                profile: change.profile.clone(),
            })
            .await?;
        acknowledged(&mut self.lock(), &change, reply)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn engine() -> Engine {
        let mut engine = Engine::open(":memory:").unwrap();
        engine
            .create_account("Private name", "old_handle", "http://localhost")
            .unwrap();
        engine.set_registered(true).unwrap();
        engine
    }
    #[test]
    fn only_the_matching_directory_acknowledgement_changes_handle_and_private_profile_survives() {
        let mut engine = engine();
        engine
            .update_my_profile(
                "Private name",
                "Private biography",
                crate::profile::PhotoChange::Keep,
            )
            .unwrap();
        let original = engine
            .profile_view(&engine.account().unwrap().identity)
            .unwrap();
        let change = prepare(&engine, "new_handle").unwrap();
        assert!(change.profile.name.is_empty() && change.profile.bio.is_empty());
        assert!(acknowledged(&mut engine, &change, Reply::Done).is_err());
        assert_eq!(engine.my_profile().unwrap().handle, "old_handle");
        let mut wrong = change.profile.clone();
        wrong.handle = "not_the_requested".into();
        assert!(acknowledged(&mut engine, &change, Reply::Registered(wrong)).is_err());
        assert_eq!(engine.my_profile().unwrap().handle, "old_handle");
        engine
            .update_my_profile(
                "Updated private name",
                "Updated private bio",
                crate::profile::PhotoChange::Keep,
            )
            .unwrap();
        let pending_private = engine.profile_view(&change.account.identity).unwrap();
        let account = acknowledged(
            &mut engine,
            &change,
            Reply::Registered(change.profile.clone()),
        )
        .unwrap();
        assert_eq!(account.handle, "new_handle");
        let after = engine.profile_view(&change.account.identity).unwrap();
        assert_eq!(after.name, pending_private.name);
        assert_eq!(after.bio, pending_private.bio);
        assert_eq!(after.photo_sha256, pending_private.photo_sha256);
        assert_eq!(after.version, pending_private.version);
        assert!(after.version >= original.version);
    }
    #[test]
    fn a_replaced_device_cannot_receive_an_older_handle_acknowledgement() {
        let mut engine = engine();
        let change = prepare(&engine, "new_handle").unwrap();
        engine.net.account.as_mut().unwrap().device = event_log::Signer::generate().id();
        assert!(acknowledged(
            &mut engine,
            &change,
            Reply::Registered(change.profile.clone())
        )
        .is_err());
        assert_eq!(engine.my_profile().unwrap().handle, "old_handle");
    }
}

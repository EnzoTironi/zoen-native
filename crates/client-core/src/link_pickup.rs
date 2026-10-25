//! One owned provisional-link pickup. No Engine, vault or account state lives on
//! the worker. Its key lease and the install fence share the same revocation gate.
use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex},
    task::{Context, Poll},
    time::Duration,
};

use event_log::Signer;
use tokio::{
    runtime::Handle,
    sync::{oneshot, watch},
    task::JoinHandle,
    time::Instant,
};
use zeroize::{Zeroize, Zeroizing};

use crate::{i18n, link::Pending, CoreError};

pub(super) type PickupFuture = Pin<Box<dyn Future<Output = Result<Vec<u8>, String>> + Send>>;
pub(super) type PickupFactory = Box<dyn FnOnce() -> PickupFuture + Send>;

fn invalid(reason: &str) -> CoreError {
    CoreError::Invalid {
        reason: reason.into(),
    }
}

fn cancelled() -> CoreError {
    invalid("Link pickup cancelled.")
}

struct Material {
    pending: Pending,
    signer: Signer,
}

impl Material {
    fn new(mut pending: Pending) -> Result<Self, CoreError> {
        let make = || -> Result<Signer, CoreError> {
            let bytes = Zeroizing::new(
                hex::decode(&pending.device_secret)
                    .map_err(|_| invalid("Invalid link request."))?,
            );
            let seed: Zeroizing<[u8; 32]> = Zeroizing::new(
                bytes
                    .as_slice()
                    .try_into()
                    .map_err(|_| invalid("Invalid link request."))?,
            );
            Ok(Signer::from_secret(&seed))
        };
        match make() {
            Ok(signer) => Ok(Self { pending, signer }),
            Err(error) => {
                pending.device_secret.zeroize();
                pending.hpke_secret.zeroize();
                pending.secret.zeroize();
                Err(error)
            }
        }
    }
}

impl Drop for Material {
    fn drop(&mut self) {
        self.pending.device_secret.zeroize();
        self.pending.hpke_secret.zeroize();
        self.pending.secret.zeroize();
        // Signer contains dalek's zeroize-on-drop SigningKey (existing feature).
    }
}

struct Active {
    id: u64,
    deadline: Instant,
    material: Option<Material>,
    task: Option<JoinHandle<()>>,
    worker_live: bool,
    waiter_live: bool,
}

#[derive(Default)]
struct State {
    retired: bool,
    next: u64,
    active: Option<Active>,
}

struct Shared {
    state: Mutex<State>,
    changed: watch::Sender<()>,
}

/// Clones are execution handles only; they never copy a signing key or Client.
#[derive(Clone)]
pub(crate) struct PickupControl(Arc<Shared>);

impl Default for PickupControl {
    fn default() -> Self {
        Self(Arc::new(Shared {
            state: Mutex::new(State::default()),
            changed: watch::channel(()).0,
        }))
    }
}

impl PickupControl {
    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        match self.0.state.lock() {
            Ok(state) => state,
            Err(error) => {
                // A panicking install/sign callback cannot make its key gate
                // reusable. Preserve actual leases for drain, retire admission.
                let mut state = error.into_inner();
                state.retired = true;
                Self::revoke(&mut state);
                state
            }
        }
    }

    fn notify(&self) {
        self.0.changed.send_replace(());
    }

    fn check_idle(state: &State) -> Result<(), CoreError> {
        if state.retired {
            return Err(cancelled());
        }
        if state.active.is_some() {
            return Err(invalid("Link pickup is still active. Wait for it to stop."));
        }
        Ok(())
    }

    /// The synchronous QR/code writer cannot replace pending material beneath a
    /// pickup, including one whose cancellation or real Drop is still draining.
    pub(crate) fn with_request<T>(
        &self,
        make: impl FnOnce() -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let state = self.lock();
        Self::check_idle(&state)?;
        make()
    }

    pub(crate) fn begin(
        &self,
        load: impl FnOnce() -> Result<Pending, CoreError>,
        budget: Duration,
        lang: i18n::Lang,
    ) -> Result<PickupWaiter, CoreError> {
        self.begin_for(load, budget, lang, "identity".into())
    }

    pub(crate) fn begin_agent(
        &self,
        load: impl FnOnce() -> Result<Pending, CoreError>,
        offer_digest: &str,
        budget: Duration,
        lang: i18n::Lang,
    ) -> Result<PickupWaiter, CoreError> {
        if offer_digest.len() != 64
            || !offer_digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(invalid("Invalid Agent enrollment digest."));
        }
        self.begin_for(
            load,
            budget,
            lang,
            format!("agent-enrollment-owner-v1:{offer_digest}"),
        )
    }

    fn begin_for(
        &self,
        load: impl FnOnce() -> Result<Pending, CoreError>,
        budget: Duration,
        lang: i18n::Lang,
        purpose: String,
    ) -> Result<PickupWaiter, CoreError> {
        self.begin_with_purpose(
            load,
            budget,
            purpose,
            move |key, relay, box_id, deadline| {
                // The closure, not this call, constructs the concrete large future.
                // Its boxed return type prevents that future entering Worker's type.
                Box::new(move || -> PickupFuture {
                    Box::pin(i18n::scope(lang, async move {
                        crate::net::fetch_link_box(&relay, &key, &box_id, deadline).await
                    }))
                })
            },
        )
    }

    #[cfg(test)]
    fn begin_with_factory(
        &self,
        load: impl FnOnce() -> Result<Pending, CoreError>,
        budget: Duration,
        make: impl FnOnce(PickupKey, String, String, Instant) -> PickupFactory,
    ) -> Result<PickupWaiter, CoreError> {
        self.begin_with_purpose(load, budget, "identity".into(), make)
    }

    fn begin_with_purpose(
        &self,
        load: impl FnOnce() -> Result<Pending, CoreError>,
        budget: Duration,
        purpose: String,
        make: impl FnOnce(PickupKey, String, String, Instant) -> PickupFactory,
    ) -> Result<PickupWaiter, CoreError> {
        // The already-entered UniFFI Compat runtime owns its dedicated driver.
        // No new runtime, thread or blocking worker is created per pickup.
        let runtime = Handle::try_current().map_err(|_| invalid("Link runtime unavailable."))?;
        let mut state = self.lock();
        Self::check_idle(&state)?;
        let material = Material::new(load()?)?;
        let secret = Zeroizing::new(
            hex::decode(&material.pending.secret).map_err(|_| invalid("Invalid link request."))?,
        );
        let secret: Zeroizing<[u8; 32]> = Zeroizing::new(
            secret
                .as_slice()
                .try_into()
                .map_err(|_| invalid("Invalid link request."))?,
        );
        let box_id = crate::link::box_id(&secret, &purpose);
        let relay = material.pending.relay.clone();
        // Preserve the existing start after synchronous pending preparation.
        let deadline = Instant::now()
            .checked_add(budget)
            .ok_or_else(|| invalid("Invalid link deadline."))?;
        let id = state.next.checked_add(1).ok_or_else(|| {
            state.retired = true;
            cancelled()
        })?;
        state.next = id;
        let key = PickupKey {
            control: self.clone(),
            id,
        };
        let factory = make(key, relay, box_id, deadline);
        let (send, receive) = oneshot::channel();
        let cancellation = self.0.changed.subscribe();
        state.active = Some(Active {
            id,
            deadline,
            material: Some(material),
            task: None,
            worker_live: true,
            waiter_live: true,
        });
        let worker = Worker {
            factory: Some(factory),
            future: None,
            send: Some(send),
            completion: Some(WorkerCompletion {
                control: self.clone(),
                id,
            }),
        };
        // Construct both real owners before releasing admission: a panicking
        // spawn must also drop its reserved caller, not strand waiter_live.
        let waiter = PickupWaiter {
            control: self.clone(),
            id,
            receive: Some(receive),
            cancellation,
        };
        drop(state);
        // spawn never polls synchronously, but a closed runtime CAN destroy the
        // worker immediately. Its Drop must be able to reacquire this gate.
        let join = runtime.spawn(worker);
        {
            let mut state = self.lock();
            let retired = state.retired;
            let active = state
                .active
                .as_mut()
                .filter(|active| active.id == id)
                .expect("live waiter preserves its active pickup");
            let cancelled = retired || active.material.is_none();
            active.task = Some(join);
            // Revocation may have happened before there was a handle to abort.
            // Its key gate was already closed; retain and abort the real task.
            if cancelled {
                active.task.as_ref().unwrap().abort();
            }
        }
        Ok(waiter)
    }

    fn active(state: &State, id: u64) -> Result<&Active, CoreError> {
        let active = state.active.as_ref().filter(|a| a.id == id);
        let active = active.ok_or_else(cancelled)?;
        if state.retired || active.material.is_none() {
            return Err(cancelled());
        }
        if Instant::now() >= active.deadline {
            return Err(invalid("Link pickup deadline expired."));
        }
        Ok(active)
    }

    fn check(&self, id: u64) -> Result<(), CoreError> {
        Self::active(&self.lock(), id).map(|_| ())
    }

    fn revoke(state: &mut State) {
        if let Some(active) = state.active.as_mut() {
            // Clear key material before requesting asynchronous task abortion.
            drop(active.material.take());
            if let Some(task) = &active.task {
                task.abort();
            }
        }
    }

    /// Retryable cancellation. The occupied slot remains until both real owners
    /// retire; it is never released merely because `abort()` was called.
    pub(crate) fn cancel(&self) {
        Self::revoke(&mut self.lock());
        self.notify();
    }

    /// Compare under the same gate as install before cancelling a particular
    /// request. A stale cancellation cannot stop a newly prepared pickup.
    pub(crate) fn cancel_request(
        &self,
        matches: impl FnOnce() -> Result<bool, CoreError>,
    ) -> Result<bool, CoreError> {
        let mut state = self.lock();
        if !matches()? {
            return Ok(false);
        }
        Self::revoke(&mut state);
        drop(state);
        self.notify();
        Ok(true)
    }

    /// Terminal for this Client instance, called synchronously at owner teardown.
    pub(crate) fn retire(&self) {
        {
            let mut state = self.lock();
            state.retired = true;
            Self::revoke(&mut state);
        }
        self.notify();
    }

    pub(crate) async fn drain(&self) {
        let mut changed = self.0.changed.subscribe();
        loop {
            if self.lock().active.is_none() {
                return;
            }
            if changed.changed().await.is_err() {
                // Self owns a Sender. This cannot be a successful drain.
                std::future::pending::<()>().await;
            }
        }
    }

    fn release(&self, id: u64, worker: bool) {
        {
            let mut state = self.lock();
            if let Some(active) = state.active.as_mut().filter(|a| a.id == id) {
                if worker {
                    active.worker_live = false;
                } else {
                    active.waiter_live = false;
                    Self::revoke(&mut state);
                }
                if state
                    .active
                    .as_ref()
                    .is_some_and(|a| !a.worker_live && !a.waiter_live)
                {
                    state.active = None;
                }
            }
        }
        self.notify();
    }
}

/// No secret can escape the synchronous key/revocation gate.
#[derive(Clone)]
pub(crate) struct PickupKey {
    control: PickupControl,
    id: u64,
}

pub(crate) trait PickupSigning {
    fn check(&self) -> Result<(), String>;
    fn id(&self) -> Result<String, String>;
    fn sign(&self, message: &[u8]) -> Result<String, String>;
}

impl PickupSigning for PickupKey {
    fn check(&self) -> Result<(), String> {
        self.control.check(self.id).map_err(|e| e.to_string())
    }

    fn id(&self) -> Result<String, String> {
        let state = self.control.lock();
        Ok(PickupControl::active(&state, self.id)
            .map_err(|e| e.to_string())?
            .material
            .as_ref()
            .unwrap()
            .signer
            .id())
    }

    fn sign(&self, message: &[u8]) -> Result<String, String> {
        let state = self.control.lock();
        Ok(PickupControl::active(&state, self.id)
            .map_err(|e| e.to_string())?
            .material
            .as_ref()
            .unwrap()
            .signer
            .sign(message))
    }
}

// Existing raw transport/deadline tests remain unchanged. Production has only
// PickupKey's revocable implementation; it cannot use an unowned Signer.
#[cfg(test)]
impl PickupSigning for Signer {
    fn check(&self) -> Result<(), String> {
        Ok(())
    }
    fn id(&self) -> Result<String, String> {
        Ok(Signer::id(self))
    }
    fn sign(&self, message: &[u8]) -> Result<String, String> {
        Ok(Signer::sign(self, message))
    }
}

pub(crate) struct PickupWaiter {
    control: PickupControl,
    id: u64,
    receive: Option<oneshot::Receiver<Result<Vec<u8>, String>>>,
    cancellation: watch::Receiver<()>,
}

impl PickupWaiter {
    pub(crate) async fn wait(&mut self) -> Result<Vec<u8>, CoreError> {
        self.control.check(self.id)?;
        let mut receive = self
            .receive
            .take()
            .ok_or_else(|| invalid("Link pickup already consumed."))?;
        loop {
            let result = tokio::select! {
                biased;
                changed = self.cancellation.changed() => {
                    changed.map_err(|_| cancelled())?;
                    self.control.check(self.id)?;
                    continue;
                }
                value = &mut receive => value.map_err(|_| cancelled())?,
            };
            self.control.check(self.id)?;
            return result.map_err(|reason| invalid(&reason));
        }
    }

    /// This gate spans synchronous decrypt, validation and canonical writes.
    /// A teardown that wins first cannot be followed by a late install. An
    /// install that wins first finishes before revoke/erase can return.
    pub(crate) fn with_install<T>(
        &self,
        install: impl FnOnce(&Pending, &Signer) -> Result<T, CoreError>,
    ) -> Result<T, CoreError> {
        let state = self.control.lock();
        let active = PickupControl::active(&state, self.id)?;
        let material = active.material.as_ref().unwrap();
        install(&material.pending, &material.signer)
    }
}

impl Drop for PickupWaiter {
    fn drop(&mut self) {
        self.control.release(self.id, false);
    }
}

struct WorkerCompletion {
    control: PickupControl,
    id: u64,
}

impl Drop for WorkerCompletion {
    fn drop(&mut self) {
        self.control.release(self.id, true);
    }
}

/// Fixed-size wrapper: no concrete TLS/pickup future exists until first poll.
struct Worker {
    factory: Option<PickupFactory>,
    future: Option<PickupFuture>,
    send: Option<oneshot::Sender<Result<Vec<u8>, String>>>,
    completion: Option<WorkerCompletion>,
}

impl Future for Worker {
    type Output = ();

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<()> {
        if self.future.is_none() {
            self.future = Some(self.factory.take().expect("worker first poll")());
        }
        match self.future.as_mut().unwrap().as_mut().poll(cx) {
            Poll::Pending => Poll::Pending,
            Poll::Ready(result) => {
                // No result may become visible while its socket future lives.
                drop(self.future.take());
                if let Some(send) = self.send.take() {
                    let _ = send.send(result);
                }
                Poll::Ready(())
            }
        }
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        // Explicit ordering also holds on abort, panic and runtime shutdown.
        // Never release the completion lease before real future destruction.
        drop(self.future.take());
        drop(self.factory.take());
        drop(self.send.take());
        drop(self.completion.take());
    }
}

#[cfg(test)]
mod tests;

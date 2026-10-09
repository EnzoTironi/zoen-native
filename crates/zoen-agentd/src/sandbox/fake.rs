//! An in-memory provider for unit tests: no isolation, counts what happens.

use super::*;
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

type Handler = Box<dyn Fn(&ExecRequest) -> ExecOutput + Send + Sync>;

pub struct FakeProvider {
    pub acquired: AtomicUsize,
    pub released: AtomicUsize,
    pub suspended: AtomicUsize,
    files: Mutex<HashMap<(String, String), Bytes>>,
    live: Mutex<Vec<String>>,
    snaps: Mutex<HashMap<String, Lease>>,
    handler: Handler,
    can_suspend: bool,
}

impl FakeProvider {
    /// Every exec answers with its argv joined by spaces.
    pub fn new() -> Self {
        Self::with_handler(Box::new(|r| ExecOutput {
            exit_code: Some(0),
            stdout: Bytes::from(r.argv.join(" ")),
            ..Default::default()
        }))
    }

    pub fn with_handler(handler: Handler) -> Self {
        FakeProvider {
            acquired: AtomicUsize::new(0),
            released: AtomicUsize::new(0),
            suspended: AtomicUsize::new(0),
            files: Mutex::new(HashMap::new()),
            live: Mutex::new(vec![]),
            snaps: Mutex::new(HashMap::new()),
            handler,
            can_suspend: true,
        }
    }

    pub fn live(&self) -> usize {
        self.live.lock().unwrap().len()
    }
}

impl Default for FakeProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl SandboxProvider for FakeProvider {
    async fn acquire(&self, spec: &SandboxSpec, owner: &str) -> Result<Lease, SandboxError> {
        self.acquired.fetch_add(1, Ordering::SeqCst);
        let lease = Lease {
            id: new_lease_id(),
            owner: owner.into(),
            tool: spec.tool.clone(),
            tier: spec.tier,
        };
        self.live.lock().unwrap().push(lease.id.clone());
        Ok(lease)
    }

    async fn exec(&self, lease: &Lease, req: ExecRequest) -> Result<ExecOutput, SandboxError> {
        if !self.live.lock().unwrap().contains(&lease.id) {
            return Err(SandboxError::NoLease);
        }
        Ok((self.handler)(&req))
    }

    async fn put_file(&self, lease: &Lease, path: &str, bytes: Bytes) -> Result<(), SandboxError> {
        self.files
            .lock()
            .unwrap()
            .insert((lease.id.clone(), path.into()), bytes);
        Ok(())
    }

    async fn get_file(&self, lease: &Lease, path: &str) -> Result<Bytes, SandboxError> {
        self.files
            .lock()
            .unwrap()
            .get(&(lease.id.clone(), path.into()))
            .cloned()
            .ok_or_else(|| SandboxError::Backend("no such file".into()))
    }

    async fn suspend(&self, lease: &Lease) -> Result<SnapshotRef, SandboxError> {
        if !self.can_suspend {
            return Err(SandboxError::Unsupported("suspend"));
        }
        self.suspended.fetch_add(1, Ordering::SeqCst);
        self.live.lock().unwrap().retain(|l| l != &lease.id);
        self.snaps
            .lock()
            .unwrap()
            .insert(lease.id.clone(), lease.clone());
        Ok(SnapshotRef {
            id: format!("snap_{}", lease.id),
            lease: lease.id.clone(),
        })
    }

    async fn resume(&self, snap: &SnapshotRef) -> Result<Lease, SandboxError> {
        let lease = self
            .snaps
            .lock()
            .unwrap()
            .remove(&snap.lease)
            .ok_or(SandboxError::NoLease)?;
        self.live.lock().unwrap().push(lease.id.clone());
        Ok(lease)
    }

    async fn release(&self, lease: Lease) -> Result<(), SandboxError> {
        self.released.fetch_add(1, Ordering::SeqCst);
        self.live.lock().unwrap().retain(|l| l != &lease.id);
        self.files
            .lock()
            .unwrap()
            .retain(|(l, _), _| l != &lease.id);
        Ok(())
    }
}

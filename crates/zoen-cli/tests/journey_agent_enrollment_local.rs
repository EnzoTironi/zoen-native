//! Explicit attachment to the already-running trusted local relay. Never starts,
//! resets or administers services/DBs. All identities/homes are fresh synthetic ones.
//!
//! Quick (retains synthetic relay rows; revokes only this test's Agent):
//! ZOEN_ENROLLMENT_ALLOW_LOCAL_WRITES=1 ZOEN_ENROLLMENT_RELAY=http://127.0.0.1:8880 \
//! cargo test -p zoen-cli --test journey_agent_enrollment_local \
//!   local_relay_independent_agent_enrollment -- --ignored --exact --nocapture
//!
//! The separately ignored real-expiry variant additionally requires
//! ZOEN_ENROLLMENT_WAIT_FOR_REAL_EXPIRY=1 and approximately sixteen minutes.
//! No fake clock, registered flag, owner proof, vault journal or relay row is edited.
#![allow(dead_code)]

#[path = "../src/agent_host_lock.rs"]
mod agent_host_lock;
#[path = "../src/file_vault.rs"]
mod file_vault;

use client_core::{
    AgentEnrollmentDeliveryDto, AgentEnrollmentDeliveryOutcome, AgentEnrollmentHostRequest,
    AgentEnrollmentPreviewDto, Client, SecretVault,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::Write,
    path::PathBuf,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use zoen_agentd::{AgentKind, Definition, ModelTier};

const RELAY: &str = "http://127.0.0.1:8880";
const KEYS: [&str; 3] = ["zoen.identity.v1", "zoen.device.v1", "zoen.agreement.v1"];
type R<T> = Result<T, String>;
fn require(condition: bool, reason: &str) -> R<()> {
    if condition {
        Ok(())
    } else {
        Err(reason.into())
    }
}
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_millis() as u64
}

// The shared CLI adapter owns the SecretVault implementation, including atomic
// replacement. This test supplies only its own fresh synthetic homes.
struct Home {
    path: PathBuf,
    lock: Option<agent_host_lock::HostLock>,
    vault: Arc<file_vault::FileVault>,
}
impl Home {
    fn new(run: &str, role: &str) -> R<Self> {
        let path = std::env::temp_dir().join(format!("zoen-enrollment-{run}-{role}"));
        let mut builder = fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&path).map_err(|e| e.to_string())?;
        let lock = agent_host_lock::HostLock::acquire(&path)?;
        let vault = Arc::new(file_vault::FileVault {
            dir: path.join("vault"),
        });
        Ok(Self {
            path,
            lock: Some(lock),
            vault,
        })
    }
    fn open(&self) -> R<Arc<Client>> {
        Client::open(
            self.path.join("account.sqlite").to_string_lossy().into(),
            "en".into(),
        )
        .map_err(|e| e.to_string())
    }
    fn clear(&mut self) -> R<()> {
        drop(self.lock.take());
        fs::remove_dir_all(&self.path).map_err(|e| e.to_string())?;
        require(!self.path.exists(), "synthetic home cleanup incomplete")
    }
    fn keys(&self) -> R<Vec<Vec<u8>>> {
        KEYS.iter()
            .map(|key| {
                self.vault
                    .load((*key).into())
                    .ok_or_else(|| format!("missing synthetic {key} slot"))
            })
            .collect()
    }
}
impl Drop for Home {
    fn drop(&mut self) {
        drop(self.lock.take());
        let _ = fs::remove_dir_all(&self.path);
    }
}
struct Journey {
    owner: Option<Arc<Client>>,
    host: Option<Arc<Client>>,
    owner_home: Home,
    host_home: Home,
    handle: String,
    agent: Option<String>,
    events: Vec<Value>,
}
impl Drop for Journey {
    fn drop(&mut self) {
        for client in [self.host.take(), self.owner.take()].into_iter().flatten() {
            client.stop_sync();
            drop(client);
        }
    }
}
impl Journey {
    fn new() -> R<Self> {
        let run = types::new_id("local").to_ascii_lowercase();
        let owner_home = Home::new(&run, "owner")?;
        let host_home = Home::new(&run, "agent")?;
        let owner = owner_home.open()?;
        let host = host_home.open()?;
        Ok(Self {
            owner: Some(owner),
            host: Some(host),
            owner_home,
            host_home,
            handle: format!("qa_enroll_{}", &run[run.len() - 12..]),
            agent: None,
            events: Vec::new(),
        })
    }
    fn event(&mut self, stage: &str, observation: Value) {
        let value = json!({"stage":stage,"at_ms":now_ms(),"observation":observation});
        eprintln!("enrollment_evidence {value}");
        self.events.push(value);
    }
    fn owner(&self) -> Arc<Client> {
        self.owner.as_ref().unwrap().clone()
    }
    fn host(&self) -> Arc<Client> {
        self.host.as_ref().unwrap().clone()
    }
    fn reopen_host(&mut self) -> R<Arc<Client>> {
        let old = self.host.take().ok_or("host missing")?;
        old.stop_sync();
        drop(old);
        let fresh = self.host_home.open()?;
        self.host = Some(fresh.clone());
        Ok(fresh)
    }
    async fn online(client: &Client) -> R<()> {
        client.start_sync(None).map_err(|e| e.to_string())?;
        let connection = client.wait_until_idle(20_000).await;
        require(
            connection.state == "online"
                && connection.synced
                && connection.pending == 0
                && connection.error.is_none(),
            &format!("normal relay sync did not complete: {connection:?}"),
        )?;
        require(
            client.account().is_some_and(|a| a.registered),
            "relay did not acknowledge registration",
        )
    }
    async fn flow(&mut self, wait_expiry: bool) -> R<()> {
        let owner = self.owner();
        let owner_account = owner
            .create_account(
                "Synthetic enrollment owner".into(),
                format!("{}_o", self.handle),
                RELAY.into(),
                self.owner_home.vault.clone(),
            )
            .map_err(|e| e.to_string())?;
        self.event("owner_created_locally", json!({"identity":owner_account.identity_id,"handle":owner_account.handle,"registered":owner_account.registered}));
        require(
            !owner_account.registered,
            "local create unexpectedly asserted registration",
        )?;
        Self::online(&owner).await?;
        self.event(
            "owner_registered_by_relay",
            json!({"identity":owner_account.identity_id,"device":owner_account.device_id}),
        );

        let definition = Definition {
            version: 1,
            kind: AgentKind::User,
            persona: "Help only with reviewed tasks.".into(),
            rules: vec!["Ask before effects.".into()],
            state: json!({}),
            model_tier: ModelTier::Cheap,
        };
        let definition_bytes = definition.encode().map_err(|e| e.to_string())?;
        let definition_json =
            String::from_utf8(definition_bytes.clone()).map_err(|e| e.to_string())?;
        let host = self.host();
        let offered = host
            .prepare_agent_enrollment(
                AgentEnrollmentHostRequest {
                    expected_owner_id: owner_account.identity_id.clone(),
                    relay_url: RELAY.into(),
                    name: "Synthetic local helper".into(),
                    handle: self.handle.clone(),
                    definition_json,
                },
                self.host_home.vault.clone(),
            )
            .map_err(|e| e.to_string())?;
        let preview_wire = owner.client_request(json!({"op":"agent_enrollment_preview","code":offered.code,"expected_identity_id":owner_account.identity_id,"expected_device_id":owner_account.device_id}).to_string()).await.map_err(|e| e.to_string())?;
        // Unified JSON's existing integer contract is decimal strings. Decode
        // that one field back to the actual shared Rust DTO, not a parallel type.
        let mut preview_json: Value =
            serde_json::from_str(&preview_wire).map_err(|e| e.to_string())?;
        let expires: u64 = preview_json["expires_ms"]
            .as_str()
            .ok_or("timestamp wire type")?
            .parse()
            .map_err(|e: std::num::ParseIntError| e.to_string())?;
        preview_json["expires_ms"] = json!(expires);
        let preview: AgentEnrollmentPreviewDto =
            serde_json::from_value(preview_json).map_err(|e| e.to_string())?;
        self.agent = Some(preview.agent_id.clone());
        require(
            preview.offer_digest == offered.offer_digest
                && preview.pairing_check == offered.pairing_check
                && preview.expires_ms == offered.expires_ms,
            "native typed preview differs from trusted host offer",
        )?;
        require(
            preview.agent_id != owner_account.identity_id
                && preview.name == "Synthetic local helper"
                && preview.handle == self.handle,
            "independent Agent/profile binding differs",
        )?;
        self.event("owner_preview_matches_host", json!({"agent":preview.agent_id,"offer_digest":preview.offer_digest,"expires_ms":preview.expires_ms}));
        let delivery_wire = owner.client_request(json!({"op":"agent_enrollment_approve","code":offered.code,"observed_offer_digest":preview.offer_digest,"confirmed_pairing_check":preview.pairing_check,"expected_identity_id":owner_account.identity_id,"expected_device_id":owner_account.device_id}).to_string()).await.map_err(|e| e.to_string())?;
        let delivery: AgentEnrollmentDeliveryDto =
            serde_json::from_str(&delivery_wire).map_err(|e| e.to_string())?;
        require(
            delivery.outcome == AgentEnrollmentDeliveryOutcome::Sent
                && delivery.agent_id == preview.agent_id
                && delivery.offer_digest == preview.offer_digest,
            "real DeliverLink was not acknowledged for reviewed Agent",
        )?;
        self.event(
            "owner_deliver_link_ack",
            json!({"receipt_id":delivery.receipt_id,"outcome":delivery.outcome}),
        );
        let prepared = host
            .wait_agent_enrollment(self.host_home.vault.clone(), 15_000)
            .await
            .map_err(|e| e.to_string())?;
        require(
            prepared.agent_id == preview.agent_id
                && prepared.owner_id == owner_account.identity_id
                && !prepared.registered
                && prepared.definition_json.as_bytes() == definition_bytes,
            "real FetchLink/verified install does not match reviewed public consent",
        )?;
        self.event(
            "production_pickup_account_prepared",
            json!({"agent":prepared.agent_id,"registered":prepared.registered}),
        );
        let mut own_keys = self.owner_home.keys()?;
        let mut host_keys = self.host_home.keys()?;
        require(
            own_keys
                .iter()
                .zip(&host_keys)
                .all(|(owner, host)| owner != host),
            "a Person secret was copied to the independent host",
        )?;
        for key in &mut own_keys {
            key.fill(0);
        }
        let device_before = host.account().ok_or("Agent account absent")?.device_id;
        drop(host);
        let host = self.reopen_host()?;
        let recovered = host
            .resume_agent_enrollment(self.host_home.vault.clone())
            .map_err(|e| e.to_string())?
            .ok_or("Accepted setup missing on cold resume")?;
        require(
            recovered == prepared && host.account().is_some_and(|a| a.device_id == device_before),
            "cold pre-registration recovery replaced Agent/device",
        )?;
        let mut recovered_keys = self.host_home.keys()?;
        require(
            host_keys == recovered_keys,
            "cold recovery changed complete own key material",
        )?;
        for key in host_keys.iter_mut().chain(recovered_keys.iter_mut()) {
            key.fill(0);
        }
        Self::online(&host).await?;
        host.agent_define(
            prepared.agent_id.clone(),
            definition.version,
            "User".into(),
            definition_bytes.clone(),
        )
        .await
        .map_err(|e| e.to_string())?;
        let state = host.agent_state().await.map_err(|e| e.to_string())?;
        require(
            state.status == "active"
                && state.kind == "User"
                && state.definition_version == definition.version
                && state.definition == definition_bytes,
            "real relay definition/status differs from accepted exact Definition",
        )?;
        let found = owner
            .find_people(self.handle.clone())
            .await
            .map_err(|e| e.to_string())?;
        require(
            found.iter().any(|p| {
                p.id == prepared.agent_id
                    && p.handle == self.handle
                    && p.owner_id.as_deref() == Some(owner_account.identity_id.as_str())
                    && p.is_mine
            }),
            "owner directory did not confirm exact owned Agent",
        )?;
        self.event("agent_registered_definition_confirmed", json!({"agent":prepared.agent_id,"device":device_before,"kind":state.kind,"definition_version":state.definition_version,"status":state.status}));
        if wait_expiry {
            self.event(
                "waiting_for_real_offer_expiry",
                json!({"expires_ms":offered.expires_ms}),
            );
            while now_ms() <= offered.expires_ms {
                tokio::time::sleep(Duration::from_millis(
                    offered
                        .expires_ms
                        .saturating_add(1)
                        .saturating_sub(now_ms())
                        .min(30_000),
                ))
                .await;
            }
            require(
                now_ms() > offered.expires_ms,
                "real deadline has not elapsed",
            )?;
        }
        drop(host);
        let host = self.reopen_host()?;
        let recovered = host
            .resume_agent_enrollment(self.host_home.vault.clone())
            .map_err(|e| e.to_string())?
            .ok_or("registered Accepted setup missing")?;
        require(
            recovered.registered
                && recovered.agent_id == prepared.agent_id
                && host.account().is_some_and(|a| a.device_id == device_before),
            "registered recovery replaced principal or registration flag",
        )?;
        Self::online(&host).await?;
        require(
            host.agent_state().await.map_err(|e| e.to_string())? == state,
            "normal relay reconnect changed confirmed Agent state",
        )?;
        self.event(if wait_expiry {"registered_recovery_after_real_expiry"} else {"registered_cold_recovery"}, json!({"agent":recovered.agent_id,"registered":recovered.registered,"real_expiry_elapsed":wait_expiry}));
        Ok(())
    }
    async fn cleanup(&mut self) -> R<()> {
        let owner = self.owner();
        let result = if let Some(agent) = &self.agent {
            match tokio::time::timeout(
                Duration::from_secs(5),
                owner.agent_control(agent.clone(), "revoke".into()),
            )
            .await
            {
                Ok(Ok(())) => Ok(()),
                Ok(Err(error)) => Err(error.to_string()),
                Err(_) => Err("test Agent revoke timed out".into()),
            }
        } else {
            Ok(())
        };
        drop(owner);
        for client in [self.host.take(), self.owner.take()].into_iter().flatten() {
            client.stop_sync();
            drop(client);
        }
        let homes = self
            .host_home
            .clear()
            .and_then(|()| self.owner_home.clear());
        self.event("cleanup", json!({"own_agent_revoke":result,"private_homes_removed":homes,"retained_synthetic_owner_rows":true,"relay_database_edited_directly":false}));
        result.and(homes)
    }
    fn evidence(&self, outcome: &R<()>, cleanup: &R<()>, slow: bool) -> R<()> {
        if let Some(path) = std::env::var_os("ZOEN_ENROLLMENT_EVIDENCE") {
            let value = json!({"endpoint":RELAY,"events":self.events,"outcome":outcome,"cleanup":cleanup,"natural_expiry_test":slow,"claims":{"providers":false,"native_device":false,"tools":false,"distributed_fencing":false,"services_started_or_reset":false}});
            let mut file = fs::OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(path)
                .map_err(|e| e.to_string())?;
            file.write_all(serde_json::to_string_pretty(&value).unwrap().as_bytes())
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
async fn run(slow: bool) {
    assert_eq!(
        std::env::var("ZOEN_ENROLLMENT_ALLOW_LOCAL_WRITES").as_deref(),
        Ok("1"),
        "explicit local synthetic-write opt-in required"
    );
    assert_eq!(
        std::env::var("ZOEN_ENROLLMENT_RELAY").as_deref(),
        Ok(RELAY),
        "only the already-running trusted local8880 endpoint is allowed"
    );
    if slow {
        assert_eq!(
            std::env::var("ZOEN_ENROLLMENT_WAIT_FOR_REAL_EXPIRY").as_deref(),
            Ok("1")
        );
    }
    let mut journey = Journey::new().expect("private synthetic homes");
    let outcome = match tokio::time::timeout(
        Duration::from_secs(if slow { 1020 } else { 90 }),
        journey.flow(slow),
    )
    .await
    {
        Ok(outcome) => outcome,
        Err(_) => Err("bounded enrollment journey timed out".into()),
    };
    let cleanup = journey.cleanup().await;
    journey
        .evidence(&outcome, &cleanup, slow)
        .expect("public evidence receipt");
    assert!(
        outcome.is_ok(),
        "first actual local relay failure: {outcome:?}; cleanup: {cleanup:?}"
    );
    assert!(
        cleanup.is_ok(),
        "cleanup could not revoke its own synthetic Agent: {cleanup:?}"
    );
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit opt-in: writes only fresh synthetic identities to existing local relay8880"]
async fn local_relay_independent_agent_enrollment() {
    run(false).await
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "separate opt-in slow test: waits for the real15-minute signed offer expiry; no clock/state mutation"]
async fn local_relay_registered_agent_recovers_after_real_expiry() {
    run(true).await
}

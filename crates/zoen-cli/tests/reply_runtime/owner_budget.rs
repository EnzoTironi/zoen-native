//! The same owner review/confirmation Core contract as iOS, over actual MLS.
use super::*;
use client_core::owner_policy::OwnerPolicyScope;
use std::{
    process::{Child, Stdio},
    time::{Duration, Instant},
};
pub(super) struct Companion {
    child: Child,
    log: std::path::PathBuf,
}
impl Drop for Companion {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
impl Companion {
    pub(super) fn start(w: &World, path: &Path) -> Result<Self, Box<dyn std::error::Error>> {
        let log = w.dir.join("reply-owner-controls.log");
        let output = std::fs::File::create(&log)?;
        let child = w
            .cmd("citizen", &["agent-controls", "--for", "180"])
            .env("ZOEN_REPLY_HOST_BIN", env!("CARGO_BIN_EXE_zoen-reply-host"))
            .env("ZOEN_REPLY_HOST_CONFIG", path)
            .env_remove("ZOEN_IMAGE_HOST_BIN")
            .env_remove("ZOEN_IMAGE_HOST_CONFIG")
            .stdout(Stdio::from(output.try_clone()?))
            .stderr(Stdio::from(output))
            .spawn()?;
        Ok(Self { child, log })
    }
    fn check(&mut self) -> TestResult {
        if let Some(status) = self.child.try_wait()? {
            return Err(format!(
                "owner controls exited {status}: {}",
                std::fs::read_to_string(&self.log)?
            )
            .into());
        }
        Ok(())
    }
    fn timeout(&self) -> Box<dyn std::error::Error> {
        format!(
            "owner budget roundtrip timed out: {}",
            std::fs::read_to_string(&self.log).unwrap_or_default()
        )
        .into()
    }
}
pub(super) async fn set(
    w: &World,
    companion: &mut Companion,
    agent: &str,
    space: &str,
    enabled: bool,
) -> TestResult {
    let (owner, scope, source, context) = quote(w, companion, agent, space).await?;
    assert_eq!(context.current_enabled, Some(!enabled));
    let review = owner.prepare_owner_policy(
        scope.clone(),
        source,
        context.options_digest,
        "500".into(),
        "110".into(),
        enabled,
    )?;
    // UI presents this exact canonical review and submits its confirmation digest.
    let approved =
        owner.review_owner_policy(scope.clone(), review.canonical_policy_json.clone())?;
    assert_eq!(approved.confirmation_digest, review.confirmation_digest);
    let submitted = owner.submit_owner_policy(
        scope.clone(),
        approved.canonical_policy_json.clone(),
        approved.confirmation_digest.clone(),
    )?;
    let repeat = owner.submit_owner_policy(
        scope.clone(),
        approved.canonical_policy_json,
        approved.confirmation_digest,
    )?;
    assert_eq!(repeat.source, submitted.source);
    let deadline = Instant::now() + Duration::from_secs(30);
    loop {
        companion.check()?;
        let status = owner.owner_policy_status(scope.clone(), submitted.source.clone())?;
        if status.state == "installed" {
            break;
        }
        if status.state == "refused" {
            return Err("owner policy refused".into());
        }
        if Instant::now() >= deadline {
            return Err(companion.timeout());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    owner.stop_sync();
    Ok(())
}

async fn quote(
    w: &World,
    companion: &mut Companion,
    agent: &str,
    space: &str,
) -> Result<
    (
        Arc<Client>,
        OwnerPolicyScope,
        String,
        client_core::owner_policy::OwnerPolicyContext,
    ),
    Box<dyn std::error::Error>,
> {
    let owner = Client::open(
        w.dir
            .join("owner/zoen.sqlite")
            .to_string_lossy()
            .into_owned(),
        "en".into(),
    )?;
    assert!(owner.unlock(Arc::new(file_vault::FileVault {
        dir: w.dir.join("owner/vault")
    }))?);
    owner.start_sync(None)?;
    owner.wait_until_settled(20_000).await?;
    let account = owner.account().ok_or("owner account")?;
    let scope = OwnerPolicyScope {
        expected_identity_id: account.identity_id,
        expected_device_id: account.device_id,
        agent_id: agent.into(),
        space_id: space.into(),
    };
    let attempts = w.count("SELECT count(*) FROM runtime_attempts").await;
    let policies = w.count("SELECT count(*) FROM runtime_policies").await;
    let request = owner.request_account_policy_options(scope.clone())?;
    let deadline = Instant::now() + Duration::from_secs(30);
    let context = loop {
        companion.check()?;
        let status = owner.owner_policy_options(scope.clone(), request.source.clone())?;
        if let Some(context) = status.context {
            break context;
        }
        if Instant::now() >= deadline {
            return Err(companion.timeout());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    };
    assert_eq!(context.service_kind, "conversation");
    assert_eq!(context.reservation_units, "110");
    assert_eq!(
        context.profile_digest,
        config::Config::read(&w.dir.join("reply-host.json"))?
            .chat_price
            .digest()?
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_attempts").await,
        attempts
    );
    assert_eq!(
        w.count("SELECT count(*) FROM runtime_policies").await,
        policies
    );
    let actual = w
        .scalar("SELECT digest FROM runtime_policies ORDER BY version DESC LIMIT 1")
        .await;
    assert_eq!(
        context.current_policy_digest.as_deref(),
        Some(actual.as_str())
    );
    Ok((owner, scope, request.source, context))
}

pub(super) async fn observe_disabled(
    w: &World,
    companion: &mut Companion,
    agent: &str,
    space: &str,
) -> TestResult {
    let (owner, _, _, context) = quote(w, companion, agent, space).await?;
    assert_eq!(context.current_enabled, Some(false));
    assert_eq!(context.current_limit_units.as_deref(), Some("500"));
    owner.stop_sync();
    Ok(())
}

//! Resume only a prepared synthetic publication: no model or Gate invocation.
use super::*;
use std::{
    fs,
    path::PathBuf,
    process::{Child, Command, Stdio},
    time::Duration,
};
use zoen_runtime::{ReplyReceiptProgress, RuntimeAuthority};

struct Relay(Child);
impl Drop for Relay {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}
struct Placeholder(PathBuf);
impl Drop for Placeholder {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.0);
    }
}

#[test]
#[ignore = "explicit retained synthetic fixture, run and private result path required; no inference"]
fn recover_prepared_managed_reply_without_inference() -> TestResult {
    let fixture = PathBuf::from(std::env::var("ZOEN_RETAINED_REPLY_FIXTURE")?).canonicalize()?;
    if fixture.parent() != Some(std::env::temp_dir().canonicalize()?.as_path())
        || !fixture
            .file_name()
            .and_then(|s| s.to_str())
            .is_some_and(|s| s.starts_with("zoen-journey-zoen_t_managed_live_chat_"))
    {
        return Err("only the retained synthetic live fixture is supported".into());
    }
    let run = std::env::var("ZOEN_RETAINED_REPLY_RUN")?;
    let result_path = PathBuf::from(std::env::var("ZOEN_RETAINED_REPLY_RESULT")?);
    let config = config::Config::read(&fixture.join("managed-service.json"))?;
    if config.home.canonicalize()? != fixture.join("reply-runtime") {
        return Err("synthetic home differs".into());
    }
    let vault = Arc::new(file_vault::FileVault {
        dir: config.home.join("vault"),
    });
    if vault.load("chat".into()).is_some() {
        return Err("retire original chat credential first".into());
    }
    // The runtime constructor requires a gateway config. This value is not a
    // provider credential; this test calls only prepared-output APIs.
    if !vault.save(
        "chat".into(),
        b"recovery-only-no-provider-credential".to_vec(),
    ) {
        return Err("placeholder".into());
    }
    let _placeholder = Placeholder(vault.dir.join("chat"));
    let runtime = config.runtime(&vault)?;
    let url = reqwest::Url::parse(&config.relay_url)?;
    if url.scheme() != "http" || url.host_str() != Some("127.0.0.1") {
        return Err("local fixture relay required".into());
    }
    let port = url.port().ok_or("fixture port")?;
    let reservation = common::PortReservation::acquire(Some(port));
    if reservation.port() != port {
        return Err("original relay port is reserved by another test".into());
    }
    let log = fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(result_path.with_extension("relay.log"))?;
    let child = Command::new(common::relay_bin())
        .args([
            "--bind",
            &format!("127.0.0.1:{port}"),
            "--relay-name",
            &config.relay_key,
        ])
        .env("DATABASE_URL", &runtime.postgres_url)
        .env("ZOEN_FDB_CELL", &config.source_cell)
        .env("ZOEN_BLOB_DIR", fixture.join("blobs"))
        .env(
            "ZOEN_MANAGED_DEVICE_CONFIG",
            config.admission_config.as_ref().ok_or("registry")?,
        )
        .env_remove("ZOEN_NATS_URL")
        .stdout(Stdio::from(log.try_clone()?))
        .stderr(Stdio::from(log))
        .spawn()?;
    let mut relay = Relay(child);
    // SAFETY: this exact opt-in test owns the sole FDB network in this process;
    // every authority and database ends before its executor and network guard.
    let network = unsafe { foundationdb::boot() };
    let executor = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()?;
    let result=executor.block_on(async {
        for _ in 0..100 {
            if let Some(exit)=relay.0.try_wait()? {return Err(format!("owned relay exited: {exit}").into());}
            if tokio::net::TcpStream::connect(("127.0.0.1",port)).await.is_ok() {break;}
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let pool=sqlx::PgPool::connect(&runtime.postgres_url).await?;
        let before:(i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM runtime_attempts), (SELECT COALESCE(sum(amount_units),0)::text FROM runtime_dispositions WHERE kind='settle')").fetch_one(&pool).await?;
        let owns:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runtime_attempts WHERE binding->'context'->>'run_id'=$1)").bind(&run).fetch_one(&pool).await?;
        if !owns {return Err("run does not belong to fixture".into());}
        println!("retained recovery: opening service authority");
        let authority=RuntimeAuthority::open_native_service(runtime,vault.clone(),config.key_version).await?;
        let mut principals=Vec::new();
        for owner in fs::read_dir(&config.home)? {
            let owner=owner?; if !owner.file_type()?.is_dir() || owner.file_name()=="vault" {continue;}
            for operation in fs::read_dir(owner.path())? {
                let operation=operation?; if !operation.file_type()?.is_dir() {continue;}
                let leaf=operation.path().join("executor");
                if leaf.join("principal.json").is_file() {principals.push(leaf);}
            }
        }
        if principals.len()!=1 {return Err("exactly one synthetic executor required".into());}
        let leaf=&principals[0];
        let principal:Value=runtime_host_config::read_private(&leaf.join("principal.json"))?;
        let agent=principal["agent"].as_str().ok_or("agent")?;
        let device=principal["device"].as_str().ok_or("device")?;
        authority.attach_native_device(Arc::new(file_vault::FileVault{dir:leaf.join("vault")}),agent,device).await?;
        println!("retained recovery: opening continuity");
        let authority=authority.open_runtime_continuity(&config.continuity).await?;
        let target=config.host()?.target;
        println!("retained recovery: qualifying output route");
        let authority=authority.qualify_shared_reply_output(target.clone()).await?;
        println!("retained recovery: delivering original publication");
        let delivery=authority.deliver_reply_output(&run,target).await;
        let after:(i64,String)=sqlx::query_as("SELECT (SELECT count(*) FROM runtime_attempts), (SELECT COALESCE(sum(amount_units),0)::text FROM runtime_dispositions WHERE kind='settle')").fetch_one(&pool).await?;
        assert_eq!(after,before,"recovery must not create another attempt or cost");
        let outcome=json!({"delivery":format!("{delivery:?}"),"attempts":after.0,"settled_microdollars":after.1,"no_model_or_gate_invocation":true});
        write_json(&result_path,&outcome)?;
        assert!(matches!(delivery?,ReplyReceiptProgress::Accepted{..}));
        let space:String=sqlx::query_scalar("SELECT binding->'context'->>'space' FROM runtime_attempts WHERE binding->'context'->>'run_id'=$1").bind(&run).fetch_one(&pool).await?;
        let output=authority.prepare_reply_output(&run).await?;
        let owner=Client::open(fixture.join("owner/zoen.sqlite").to_string_lossy().into(),"en".into())?;
        if !owner.unlock(Arc::new(file_vault::FileVault{dir:fixture.join("owner/vault")}))? {return Err("synthetic owner unlock".into());}
        owner.start_sync(None)?;
        owner.wait_until_settled(20_000).await?;
        let phrase=owner.timeline(space.clone())?.into_iter().find_map(|entry|match entry.kind {
            client_core::EntryKind::Message{text,..} if text.starts_with("First use search_conversation_history")=>text.rsplit_once("nothing else: ").map(|(_,phrase)|phrase.to_owned()),
            _=>None,
        }).ok_or("synthetic search challenge")?;
        let checked=super::managed_support::assert_recovered_reply(&owner,&space,agent,&json!({"client_id":output.client_id,"message":output.message}),&phrase).await;
        owner.stop_sync();
        checked?;
        write_json(&result_path.with_extension("owner.json"),&json!({"cold_owner_verified":true,"client_id":output.client_id,"no_model_or_gate_invocation":true}))?;
        pool.close().await;
        Ok(())
    });
    drop(executor);
    drop(network);
    result
}

use super::*;
use std::{
    io::Write,
    process::{Command, Stdio},
};
pub fn write_json(path: &Path, value: &Value) -> TestResult {
    let mut options = std::fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(&serde_json::to_vec(value)?)?;
    Ok(())
}
pub fn account(w: &World, who: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let db = rusqlite::Connection::open(w.dir.join(who).join("zoen.sqlite"))?;
    let value: String = db.query_row("SELECT value FROM meta WHERE key='account'", [], |r| {
        r.get(0)
    })?;
    Ok(serde_json::from_str(&value)?)
}
pub fn call(
    path: &Path,
    action: &str,
    input: Option<Value>,
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zoen-reply-host"));
    if action == "control" {
        command.env_clear();
        for name in ["DYLD_LIBRARY_PATH", "LD_LIBRARY_PATH"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
    }
    command
        .args(["--config"])
        .arg(path)
        .arg(action)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let mut child = command.spawn()?;
    if let Some(input) = input {
        child
            .stdin
            .take()
            .ok_or("stdin")?
            .write_all(&serde_json::to_vec(&input)?)?;
    }
    let result = child.wait_with_output()?;
    if !result.status.success() {
        return Err(format!(
            "reply host {action}: {}",
            String::from_utf8_lossy(&result.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&result.stdout)?)
}
pub fn configuration(w: &World, agent: &str, endpoint: &str) -> Value {
    serde_json::json!({"namespace":format!("reply-device-{}",w.cell),"agent":agent,"device":"",
        "home":w.dir.join("reply-runtime"),"continuity":w.dir.join("independent-anchor"),
        "postgres_key":"pg","evidence_key":"evidence","fdb_cluster_file":std::env::var("FDB_CLUSTER_FILE").ok(),
        "source_cell":w.cell,"relay_url":w.relay_url(),"relay_key":w.relay_name(),"key_version":1,
        "chat_credential_key":"chat","chat_endpoint_policy":"loopback_fixture","gate":{"provider":"offline"},
        "chat_price":{"version":"chat-v1","endpoint":endpoint,"model":"reply-fixture","credential_ref":"chat","currency":"USD","scale":6,"input_units_per_million":1000000,"output_units_per_million":1000000,"max_billable_input_tokens":100,"max_output_tokens":10,"max_request_bytes":32768,"max_response_bytes":8192}})
}
pub async fn signed_policy(
    w: &World,
    who: &str,
    owner: &str,
    profile: String,
) -> Result<types::owner_budget::SignedOwnerPolicy, Box<dyn std::error::Error>> {
    signed_policy_limits(w, who, owner, profile, 500, 110).await
}
pub async fn signed_policy_limits(
    w: &World,
    who: &str,
    owner: &str,
    profile: String,
    limit_units: i64,
    max_attempt_units: i64,
) -> Result<types::owner_budget::SignedOwnerPolicy, Box<dyn std::error::Error>> {
    let author = policy_author(w, who)?;
    assert_eq!(author.identity, owner);
    let date = w
        .scalar("SELECT to_char(now() AT TIME ZONE 'UTC','YYYY-MM')")
        .await;
    let (year, month) = date.split_once('-').ok_or("date")?;
    let end=w.scalar("SELECT ((extract(epoch FROM (date_trunc('month',now())+interval '1 month'))*1000)::bigint)::text").await.parse()?;
    Ok(event_log::owner_budget::sign(
        &author,
        types::owner_budget::OwnerPeriodPolicy {
            owner: owner.into(),
            year: year.parse()?,
            month: month.parse()?,
            version: 1,
            previous_digest: None,
            currency: "USD".into(),
            scale: 6,
            limit_units,
            max_attempt_units,
            expires_at_ms: end,
            enabled: true,
            allowed_profiles: vec![profile],
        },
    )
    .ok_or("signed owner policy")?)
}

pub fn serve(path: &Path, commands: &[Value]) -> Result<Vec<Value>, Box<dyn std::error::Error>> {
    let mut child = Command::new(env!("CARGO_BIN_EXE_zoen-reply-host"))
        .arg("--config")
        .arg(path)
        .arg("serve")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().ok_or("serve stdin")?;
    for command in commands {
        input.write_all(&serde_json::to_vec(command)?)?;
        input.write_all(b"\n")?;
    }
    drop(input);
    let output = child.wait_with_output()?;
    if !output.status.success() {
        return Err(format!("reply serve: {}", String::from_utf8_lossy(&output.stderr)).into());
    }
    String::from_utf8(output.stdout)?
        .lines()
        .map(|line| serde_json::from_str(line).map_err(Into::into))
        .collect()
}

pub fn policy_author(
    w: &World,
    who: &str,
) -> Result<event_log::Author, Box<dyn std::error::Error>> {
    use event_log::{Author, Signer};
    let vault = file_vault::FileVault {
        dir: w.dir.join(who).join("vault"),
    };
    let key = |name: &str| -> Result<Signer, Box<dyn std::error::Error>> {
        Ok(Signer::from_secret(
            &vault
                .load(name.into())
                .ok_or("owner key")?
                .try_into()
                .map_err(|_| "owner key size")?,
        ))
    };
    Ok(Author::device(
        &key("zoen.identity.v1")?,
        key("zoen.device.v1")?,
    ))
}

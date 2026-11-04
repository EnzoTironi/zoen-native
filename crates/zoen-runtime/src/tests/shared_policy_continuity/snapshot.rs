//! Use maintained PostgreSQL dump/restore, preserving immutable history and
//! constraints. Only generated fixture databases and private temporary files.
use super::*;
use std::{
    fs::{File, OpenOptions},
    io::Read,
    path::PathBuf,
    process::{Command, Stdio},
};

pub(in crate::tests) struct Snapshot {
    directory: PathBuf,
    dump: PathBuf,
}
struct Restored {
    url: String,
    database: String,
}

fn postgres(program: &str) -> Command {
    if let Some(container) = std::env::var_os("ZOEN_TEST_PG_CONTAINER") {
        let mut command = Command::new("docker");
        command
            .args(["exec", "-i", "-e", "PGCONNECT_TIMEOUT=5"])
            .arg(container)
            .arg(program);
        command
    } else {
        let mut command = Command::new(program);
        command.env("PGCONNECT_TIMEOUT", "5");
        command
    }
}

pub(in crate::tests) fn check_postgres_tools() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("ZOEN_TEST_PG")?;
    let output = postgres("pg_dump")
        .args(["--schema-only", "--lock-wait-timeout=5000", "--dbname"])
        .arg(url)
        .stdout(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "PostgreSQL snapshot preflight failed ({}). Check the connection and use pg_dump at least as new as the server.",
            output.status
        )
        .into());
    }
    Ok(())
}

impl Snapshot {
    pub(in crate::tests) fn capture(w: &World) -> Result<Self, Box<dyn std::error::Error>> {
        let directory = std::env::temp_dir().join(types::new_id("policy-snapshot")?);
        let mut builder = std::fs::DirBuilder::new();
        #[cfg(unix)]
        {
            use std::os::unix::fs::DirBuilderExt;
            builder.mode(0o700);
        }
        builder.create(&directory)?;
        let snapshot = Self {
            dump: directory.join("database.dump"),
            directory,
        };
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let file = options.open(&snapshot.dump)?;
        let output = postgres("pg_dump")
            .args([
                "--format=custom",
                "--no-owner",
                "--no-privileges",
                "--lock-wait-timeout=5000",
                "--dbname",
            ])
            .arg(&w.db_url)
            .stdin(Stdio::null())
            .stdout(Stdio::from(file))
            .stderr(Stdio::piped())
            .output()?;
        if !output.status.success() {
            return Err("owned PostgreSQL snapshot failed".into());
        }
        let mut magic = [0; 5];
        File::open(&snapshot.dump)?.read_exact(&mut magic)?;
        assert_eq!(&magic, b"PGDMP");
        Ok(snapshot)
    }

    async fn restore(&self, w: &World) -> Result<Restored, Box<dyn std::error::Error>> {
        let id = Signer::generate()?.id();
        let suffix = id.get(..16).ok_or("fixture database ID")?;
        let database = format!("zoen_policy_restore_{suffix}");
        let (base, _) = w.admin.rsplit_once('/').ok_or("fixture PostgreSQL URL")?;
        let restored = Restored {
            url: format!("{base}/{database}"),
            database,
        };
        let mut connection = PgConnection::connect(&w.admin).await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "CREATE DATABASE {}",
            restored.database
        )))
        .execute(&mut connection)
        .await?;
        let output = postgres("pg_restore")
            .args([
                "--exit-on-error",
                "--no-owner",
                "--no-privileges",
                "--dbname",
            ])
            .arg(&restored.url)
            .stdin(Stdio::from(File::open(&self.dump)?))
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .output();
        match output {
            Ok(output) if output.status.success() => Ok(restored),
            _ => {
                restored.cleanup(w).await?;
                Err("owned PostgreSQL restore failed".into())
            }
        }
    }
}
impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}
impl Restored {
    async fn cleanup(self, w: &World) -> TestResult {
        let mut connection = PgConnection::connect(&w.admin).await?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            "DROP DATABASE {} WITH (FORCE)",
            self.database
        )))
        .execute(&mut connection)
        .await?;
        Ok(())
    }
}

pub(super) async fn restored_policy_refuses(
    w: &World,
    home: &Home,
    snapshot: &Snapshot,
    tip: &relay_admission::managed_images::Checkpoint,
) -> TestResult {
    let restored = snapshot.restore(w).await?;
    let pool = PgPool::connect(&restored.url).await?;
    let policies: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_policies")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        policies, 1,
        "restore must contain the actual older signed policy"
    );
    let denied = sqlx::query("DELETE FROM runtime_policies")
        .execute(&pool)
        .await;
    assert!(
        denied
            .as_ref()
            .err()
            .and_then(sqlx::Error::as_database_error)
            .and_then(|error| error.code())
            .is_some_and(|code| code == "23514"),
        "restore must retain immutable-history enforcement"
    );
    assert!(
        matches!(
            fresh_at(w, &restored.url)
                .await?
                .open_runtime_continuity(&home.0)
                .await,
            Err(RuntimeError::RestoreUnreconciled)
        ),
        "an actual old SQL backup cannot adopt the advanced anchor/FDB"
    );
    // Deliberately overlay only mutable detector metadata. A counter alone
    // would now match, but the old business-policy inventory must still refuse.
    sqlx::query("UPDATE runtime_image_checkpoint SET generation=$1,sequence=$2,digest=$3 WHERE singleton AND profile=3")
        .bind(i64::try_from(tip.generation)?).bind(i64::try_from(tip.sequence)?).bind(&tip.digest).execute(&pool).await?;
    assert!(
        matches!(
            fresh_at(w, &restored.url)
                .await?
                .open_runtime_continuity(&home.0)
                .await,
            Err(RuntimeError::RestoreUnreconciled)
        ),
        "matching current markers cannot hide the old policy"
    );
    pool.close().await;
    restored.cleanup(w).await?;
    let current = fresh(w).await?.open_runtime_continuity(&home.0).await?;
    assert_eq!(reader(&current, &home.0).await?.checkpoint()?, *tip);
    drop(current);
    println!("shared policy backup: real pg_dump/pg_restore to a new owned database preserves immutable triggers; old SQL and counter-only repairs both refuse, current original remains usable PASS");
    Ok(())
}

pub(in crate::tests) async fn restored_finance_refuses(
    w: &World,
    directory: &Path,
    snapshot: &Snapshot,
    tip: &relay_admission::managed_images::Checkpoint,
    expected_attempts: i64,
    expected_claims: i64,
    expected_witnesses: i64,
) -> TestResult {
    let restored = snapshot.restore(w).await?;
    let pool = PgPool::connect(&restored.url).await?;
    let old_attempts: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_attempts")
        .fetch_one(&pool)
        .await?;
    assert_eq!(old_attempts, expected_attempts);
    let claims: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_claims")
        .fetch_one(&pool)
        .await?;
    assert_eq!(claims, expected_claims);
    let witnesses: i64 = sqlx::query_scalar("SELECT count(*) FROM runtime_admission_witnesses")
        .fetch_one(&pool)
        .await?;
    assert_eq!(
        witnesses, expected_witnesses,
        "actual backup retains the expected original admission count"
    );
    let immutable: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='runtime_attempts'::regclass AND tgname='immutable_history' AND tgenabled='O')")
        .fetch_one(&pool).await?;
    assert!(
        immutable,
        "restore retains actual financial immutability triggers"
    );
    sqlx::query("UPDATE runtime_image_checkpoint SET generation=$1,sequence=$2,digest=$3 WHERE singleton AND profile=3")
        .bind(i64::try_from(tip.generation)?).bind(i64::try_from(tip.sequence)?).bind(&tip.digest).execute(&pool).await?;
    assert!(
        matches!(
            fresh_at(w, &restored.url)
                .await?
                .open_runtime_continuity(directory)
                .await,
            Err(RuntimeError::RestoreUnreconciled)
        ),
        "identical policies and advanced markers do not hide an actually older financial ledger"
    );
    pool.close().await;
    restored.cleanup(w).await?;
    Ok(())
}

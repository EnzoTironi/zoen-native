//! A response is derived from this host's actual stores, never caller facts.
use protocol::runtime_binding::{BlobBinding, Expectation, Proof, Request};

pub(crate) async fn expectation(
    st: &crate::Shared,
    identity: &str,
    device: &str,
    request: Request,
) -> Result<Expectation, &'static str> {
    if !request.valid() {
        return Err("invalid runtime binding");
    }
    // Registered agent and its active certified device, in this actual SQL
    // directory. These reads authorize only a proof, never provider/output work.
    let witness: Option<String> = sqlx::query_scalar(
        "SELECT b.witness FROM runtime_deployment_binding b \
         JOIN identities i ON i.id=$2 AND i.kind='Agent' AND i.owner IS NOT NULL \
         JOIN devices d ON d.identity=i.id AND d.device=$3 AND d.revoked_at IS NULL \
         WHERE b.namespace=$1 AND b.singleton",
    )
    .bind(&request.namespace)
    .bind(identity)
    .bind(device)
    .fetch_optional(&st.pool)
    .await
    .map_err(|_| "runtime binding unavailable")?;
    let witness = witness.ok_or("runtime binding refused")?;
    let managed = st.log.managed_device(device);
    match &managed {
        Some(configured)
            if configured.deployment().agent == identity
                && configured.deployment().namespace == request.namespace
                && configured.deployment().witness == witness
                && request.managed.is_some() => {}
        Some(_) => return Err("managed runtime binding required"),
        None if request.managed.is_some() => return Err("unconfigured managed device"),
        None => {}
    }
    let cell = st
        .log
        .runtime_cell(&request.namespace, &witness)
        .await
        .map_err(|_| "runtime binding unavailable")?
        .ok_or("runtime binding refused")?;
    if let Some(configured) = managed {
        if configured.deployment().source != cell {
            return Err("managed runtime binding refused");
        }
        current_managed(st, &configured, &request).await?;
    }
    Expectation::new(&witness, request, &cell, &st.relay_name, identity, device)
        .ok_or("runtime binding refused")
}

pub(crate) async fn prove(
    st: &crate::Shared,
    identity: &str,
    device: &str,
    request: Request,
) -> Result<Proof, &'static str> {
    Ok(expectation(st, identity, device, request).await?.proof())
}

pub(crate) async fn verify_blob(
    st: &crate::Shared,
    device: &str,
    binding: &BlobBinding,
) -> Result<Option<relay_admission::managed_images::GenerationGuard>, &'static str> {
    if st.log.managed_device(device).is_some() {
        effect_checkpoint(&binding.request)?;
    }
    let expected = expectation(st, &binding.identity, device, binding.request.clone()).await?;
    if !expected.verify(&binding.proof) {
        return Err("runtime binding refused");
    }
    let Some(configured) = st.log.managed_device(device) else {
        return Ok(None);
    };
    current_managed(st, &configured, &binding.request).await?;
    let requested = effect_checkpoint(&binding.request)?;
    // Unrelated documents may advance the tip. Keep this original historical
    // checkpoint's generation locked through storage without freezing the tip.
    configured.upload_guard(&requested).map(Some)
}

pub(crate) fn checkpoint(
    request: &Request,
) -> Result<relay_admission::managed_images::Checkpoint, &'static str> {
    let value = request
        .managed
        .as_ref()
        .ok_or("managed runtime binding required")?;
    if !value.valid() {
        return Err("invalid managed checkpoint");
    }
    Ok(relay_admission::managed_images::Checkpoint {
        generation: value.generation,
        sequence: value.sequence,
        digest: value.digest.clone(),
    })
}

/// A bootstrap binding is read-only; it cannot authorize a storage effect.
pub(crate) fn effect_checkpoint(
    request: &Request,
) -> Result<relay_admission::managed_images::Checkpoint, &'static str> {
    let checkpoint = checkpoint(request)?;
    if checkpoint.sequence == 0 {
        return Err("managed bootstrap binding is read-only");
    }
    Ok(checkpoint)
}

pub(crate) async fn current_managed(
    st: &crate::Shared,
    configured: &crate::managed_devices::ManagedDevice,
    request: &Request,
) -> Result<relay_admission::managed_images::Checkpoint, &'static str> {
    let deployment = configured.deployment();
    if request.namespace != deployment.namespace {
        return Err("managed namespace refused");
    }
    let requested = checkpoint(request)?;
    let row: Option<(i64,i64,String)> = sqlx::query_as(
        "SELECT generation,sequence,digest FROM runtime_image_checkpoint WHERE singleton AND namespace=$1 AND witness=$2 AND agent=$3 AND device=$4",
    ).bind(&deployment.namespace).bind(&deployment.witness).bind(&deployment.agent)
        .bind(&deployment.device).fetch_optional(&st.pool).await.map_err(|_| "managed stores unavailable")?;
    let (generation, sequence, digest) = row.ok_or("managed checkpoint missing")?;
    let sql = relay_admission::managed_images::Checkpoint {
        generation: u64::try_from(generation).map_err(|_| "invalid managed checkpoint")?,
        sequence: u64::try_from(sequence).map_err(|_| "invalid managed checkpoint")?,
        digest,
    };
    let fdb = st
        .log
        .managed_checkpoint(deployment)
        .await
        .map_err(|_| "managed stores unavailable")?
        .ok_or("managed checkpoint missing")?;
    validate_managed_checkpoints(configured, &requested, &sql, &fdb)?;
    Ok(sql)
}

fn validate_managed_checkpoints(
    configured: &crate::managed_devices::ManagedDevice,
    requested: &relay_admission::managed_images::Checkpoint,
    sql: &relay_admission::managed_images::Checkpoint,
    fdb: &relay_admission::managed_images::Checkpoint,
) -> Result<(), &'static str> {
    if sql != fdb || requested.generation != sql.generation || requested.sequence > sql.sequence {
        return Err("managed stores require reconciliation");
    }
    configured
        .anchor()
        .check_store_checkpoint(sql)
        .map_err(|_| "managed anchor refused")?;
    // A genesis checkpoint has no Prepared history. Only the exact independently
    // anchored current genesis may prove the read-only bootstrap binding.
    if requested.sequence == 0 {
        return if requested == sql && sql.generation == 1 {
            Ok(())
        } else {
            Err("managed bootstrap checkpoint refused")
        };
    }
    // Proving the transport binding also proves this original historical CP.
    // Later unrelated checkpoints may advance; they cannot invent this record.
    configured
        .anchor()
        .retained_checkpoint(requested)
        .map_err(|_| "managed historical checkpoint refused")?;
    Ok(())
}

#[cfg(test)]
mod managed_tests;

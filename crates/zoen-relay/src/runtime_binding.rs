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
    let expected = expectation(st, &binding.identity, device, binding.request.clone()).await?;
    if !expected.verify(&binding.proof) {
        return Err("runtime binding refused");
    }
    let Some(configured) = st.log.managed_device(device) else {
        return Ok(None);
    };
    let current = current_managed(st, &configured, &binding.request).await?;
    let requested = checkpoint(&binding.request)?;
    // Blob writes are bound to the exact current checkpoint. They cannot use an
    // old transport proof after another checkpoint advances the deployment.
    if requested != current {
        return Err("managed upload checkpoint changed");
    }
    configured.upload_guard(&current).map(Some)
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
    if sql != fdb || requested.generation != sql.generation || requested.sequence > sql.sequence {
        return Err("managed stores require reconciliation");
    }
    configured
        .anchor()
        .check_store_checkpoint(&sql)
        .map_err(|_| "managed anchor refused")?;
    Ok(sql)
}

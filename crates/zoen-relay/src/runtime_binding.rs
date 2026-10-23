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
    let cell = st
        .log
        .runtime_cell(&request.namespace, &witness)
        .await
        .map_err(|_| "runtime binding unavailable")?
        .ok_or("runtime binding refused")?;
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
) -> Result<(), &'static str> {
    let expected = expectation(st, &binding.identity, device, binding.request.clone()).await?;
    if !expected.verify(&binding.proof) {
        return Err("runtime binding refused");
    }
    Ok(())
}

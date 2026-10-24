//! Actual router/SQL/cryptographic HTTP admission; no fabricated FDB proof.
use super::*;
use object_store::ObjectStoreExt;
use relay_admission::managed_images::{AnchorWriter, Deployment};

struct Home(std::path::PathBuf);
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[tokio::test]
async fn registry_refuses_ordinary_signed_put_and_header_stripping_without_touching_storage(
) -> Result<(), Box<dyn std::error::Error>> {
    let root = Signer::generate()?;
    let author = Author::device(&root, Signer::generate()?);
    let device = author.device.as_deref().ok_or("missing fixture device")?;
    let home = Home(std::env::temp_dir().join(types::new_id("managed-http")?));
    let deployment = Deployment {
        namespace: "managed-http".into(),
        witness: "a".repeat(64),
        source: "test-cell".into(),
        agent: author.identity.clone(),
        device: device.into(),
    };
    let writer = AnchorWriter::create(&home.0, deployment.clone())?;
    let registry =
        zoen_relay::managed_devices::ManagedDevices::open([(deployment, home.0.clone())])?;
    let test = TestRelay::configured_log(
        Arc::new(object_store::memory::InMemory::new()),
        "",
        Arc::new(UnusedLog { registry }),
    )
    .await?;
    let profile = Identity {
        owner_proof: None,
        id: author.identity.clone(),
        kind: IdentityKind::Person,
        name: "Fixture".into(),
        handle: "managedhttp".into(),
        tint_hex: "#123456".into(),
        glyph: None,
        owner: None,
        bio: String::new(),
    };
    let mut tx = test.state.pool.begin().await?;
    db::register(
        &mut tx,
        &profile,
        &profile.handle,
        device,
        author
            .cert
            .as_deref()
            .ok_or("missing fixture certificate")?,
        true,
    )
    .await?;
    tx.commit().await?;
    let bytes = b"actual encrypted payload";
    let sha = hex::encode(Sha256::digest(bytes));
    // A valid ordinary signature must not become a managed-upload fallback.
    let response = test
        .signed(
            &author,
            Method::PUT,
            &format!("/v1/blobs/{sha}"),
            bytes,
            |ts| blob_put_message(&sha, ts, &test.state.relay_name),
        )
        .await;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response.text().await?, "managed runtime binding required");
    let response = test
        .client
        .put(format!("{}/v1/blobs/{sha}", test.url))
        .header("x-zoen-device", device)
        .header("x-zoen-ts", "1")
        .header("x-zoen-sig", "stripped bound signature")
        .body(bytes.to_vec())
        .send()
        .await?;
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert!(matches!(
        test.state
            .blobs
            .get(&object_store::path::Path::from(sha))
            .await,
        Err(object_store::Error::NotFound { .. })
    ));
    test.close().await;
    drop(writer);
    Ok(())
}

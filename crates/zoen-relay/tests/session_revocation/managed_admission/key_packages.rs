//! The real request must not turn missing SQL history into fresh managed stock.
use super::*;

fn packages(
    author: &Author,
    last_resort: bool,
) -> Result<Vec<Vec<u8>>, Box<dyn std::error::Error>> {
    let mut connection = rusqlite::Connection::open_in_memory()?;
    mls::migrate(&mut connection)?;
    let device = mls::Device::new(
        &connection,
        Signer::generate()?.secret(),
        &author.identity,
        author.key.secret(),
        author
            .cert
            .as_deref()
            .ok_or("missing fixture certificate")?,
    )?;
    Ok(device.key_packages(2, last_resort)?)
}

async fn counts(test: &TestRelay, device: &str) -> Result<(i64, i64), sqlx::Error> {
    sqlx::query_as("SELECT (SELECT count(*) FROM key_packages WHERE device=$1), (SELECT count(*) FROM key_package_publications WHERE device=$1)")
        .bind(device).fetch_one(&test.state.pool).await
}

async fn stock(test: &TestRelay, device: &str) -> Result<Vec<(String, bool)>, sqlx::Error> {
    sqlx::query_as("SELECT encode(sha256(data),'hex'),last_resort FROM key_packages WHERE device=$1 ORDER BY 1,2")
        .bind(device).fetch_all(&test.state.pool).await
}

async fn connect_stock(
    url: &str,
    author: &Author,
) -> Result<(Socket, Vec<u32>), Box<dyn std::error::Error>> {
    let (mut socket, ready) = authenticate(url, author).await;
    assert!(matches!(ready, ServerFrame::Ready { .. }));
    send(&mut socket, ClientFrame::Ping).await;
    let mut notices = Vec::new();
    loop {
        match recv(&mut socket).await.ok_or("fixture socket closed")? {
            ServerFrame::KeyPackagesLow { device, remaining }
                if Some(device.as_str()) == author.device.as_deref() =>
            {
                notices.push(remaining)
            }
            ServerFrame::Pong { .. } => return Ok((socket, notices)),
            _ => {}
        }
    }
}

#[tokio::test]
async fn managed_key_packages_cannot_downgrade_to_ordinary_socket_after_receipt_loss(
) -> Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let managed = Author::device(&root, Signer::generate()?);
    let ordinary = Author::device(&root, Signer::generate()?);
    let device = managed.device.as_deref().ok_or("missing managed device")?;
    let other_device = ordinary
        .device
        .as_deref()
        .ok_or("missing ordinary device")?;
    let mut first = connect(&test.ws_url, &managed).await;
    request(
        &mut first,
        1,
        Op::Register {
            profile: profile(&managed, "managedkeys"),
        },
    )
    .await?;
    request(&mut first, 2, link_op(&ordinary, &Signer::generate()?.id())).await?;
    let mut other = connect(&test.ws_url, &ordinary).await;
    let batch = packages(&managed, false)?;
    let last_resort = packages(&managed, true)?
        .into_iter()
        .next()
        .ok_or("missing last resort")?;
    // Seed a past publication through the same SQL primitive used by native
    // custody. This is fixture history, not a claim of qualified native restore.
    let validated = relay_admission::directory::key_packages::validate_publications(
        &managed.identity,
        device,
        &batch,
        None,
    )?;
    assert!(
        zoen_relay::db::put_key_packages(
            &test.state.pool,
            &managed.identity,
            device,
            &validated,
            None
        )
        .await?
    );
    assert_eq!(counts(&test, device).await?, (2, 2));
    // Deliberately remove only this fixture's SQL history. The independently
    // retained managed registration must still prevent ordinary republication.
    sqlx::query("DELETE FROM key_packages WHERE device=$1")
        .bind(device)
        .execute(&test.state.pool)
        .await?;
    sqlx::query("DELETE FROM key_package_publications WHERE device=$1")
        .bind(device)
        .execute(&test.state.pool)
        .await?;
    // Refill hints apply only after the device opted in with last-resort stock.
    assert!(
        zoen_relay::db::put_key_packages(
            &test.state.pool,
            &managed.identity,
            device,
            &[],
            Some(&last_resort)
        )
        .await?
    );
    request(
        &mut other,
        20,
        Op::PublishKeyPackages {
            packages: Vec::new(),
            last_resort: packages(&ordinary, true)?.into_iter().next(),
        },
    )
    .await?;
    let retained = stock(&test, device).await?;
    let home = Home(std::env::temp_dir().join(types::new_id("managed-mls-wire")?));
    let deployment = Deployment {
        namespace: "managed-mls-wire".into(),
        witness: "a".repeat(64),
        source: "fixture-cell".into(),
        agent: managed.identity.clone(),
        device: device.into(),
    };
    let writer = AnchorWriter::create(&home.0, deployment.clone())?;
    let registry =
        zoen_relay::managed_devices::ManagedDevices::open([(deployment, home.0.clone())])?;
    test.log
        .managed_registry
        .set(registry)
        .map_err(|_| "fixture registry already set")?;
    let checkpoint = writer.reader().checkpoint()?;
    drop(first);
    drop(other);
    let (mut first, hints) = connect_stock(&test.ws_url, &managed).await?;
    assert!(
        hints.is_empty(),
        "managed custody must not be asked to refill over the ordinary socket"
    );
    let (mut other, hints) = connect_stock(&test.ws_url, &ordinary).await?;
    assert_eq!(
        hints,
        vec![0],
        "the ordinary sibling still receives its refill notice"
    );
    for (id, op) in [
        (
            3,
            Op::PublishKeyPackages {
                packages: batch.clone(),
                last_resort: None,
            },
        ),
        (
            4,
            Op::PublishKeyPackages {
                packages: Vec::new(),
                last_resort: Some(last_resort),
            },
        ),
        (
            5,
            Op::PublishKeyPackages {
                packages: batch,
                last_resort: None,
            },
        ),
    ] {
        let response = request(&mut first, id, op).await;
        assert!(
            response.is_err(),
            "managed ordinary key-package publication was accepted: {response:?}"
        );
        assert_eq!(
            response.err().as_deref(),
            Some("managed key packages require native custody")
        );
        assert_eq!(counts(&test, device).await?, (1, 0));
        assert_eq!(stock(&test, device).await?, retained);
        assert_eq!(writer.reader().checkpoint()?, checkpoint);
    }
    request(
        &mut other,
        6,
        Op::PublishKeyPackages {
            packages: packages(&ordinary, false)?,
            last_resort: None,
        },
    )
    .await?;
    assert_eq!(counts(&test, other_device).await?, (3, 2));
    assert_eq!(counts(&test, device).await?, (1, 0));
    // Existing native custody can still publish through its trusted SQL path.
    // A real destructive claim must notify the ordinary sibling, without
    // asking managed custody to refill through the refused generic endpoint.
    assert!(
        zoen_relay::db::put_key_packages(
            &test.state.pool,
            &managed.identity,
            device,
            &validated,
            None
        )
        .await?
    );
    let claimed = request(
        &mut other,
        7,
        Op::ClaimKeyPackages {
            ids: vec![managed.identity.clone()],
            operation_id: None,
        },
    )
    .await?;
    assert!(
        matches!(claimed, Reply::KeyPackages(ref rows) if rows.iter().any(|row| row.device == device))
    );
    send(&mut first, ClientFrame::Ping).await;
    let mut low = Vec::new();
    loop {
        match recv(&mut first).await.ok_or("managed fixture closed")? {
            ServerFrame::KeyPackagesLow { device, .. } => low.push(device),
            ServerFrame::Pong { .. } => break,
            _ => {}
        }
    }
    assert_eq!(low, vec![other_device.to_string()]);
    assert_eq!(counts(&test, device).await?, (2, 2));
    drop(first);
    drop(other);
    drop(writer);
    test.close().await;
    Ok(())
}

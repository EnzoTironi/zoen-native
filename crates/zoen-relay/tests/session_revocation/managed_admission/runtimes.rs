//! Real authenticated sessions use the shared registry, not a caller-selected
//! anchor. The unrelated log sequencer remains the controlled TestRelay double.
use super::*;
use relay_admission::managed_images::RuntimeDeployment;
use zoen_relay::managed_devices::{ManagedDevices, ManagedPrincipal, ManagedRuntime};

#[tokio::test]
async fn multiple_registered_principals_cannot_downgrade_shared_runtime_authority(
) -> Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let first = Author::device(&Signer::generate()?, Signer::generate()?);
    let second = Author::device(&Signer::generate()?, Signer::generate()?);
    let ordinary = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut one = connect(&test.ws_url, &first).await;
    let mut two = connect(&test.ws_url, &second).await;
    let mut other = connect(&test.ws_url, &ordinary).await;
    for (author, socket, handle) in [
        (&first, &mut one, "runtimefirst"),
        (&second, &mut two, "runtimesecond"),
        (&ordinary, &mut other, "runtimeordinary"),
    ] {
        request(
            socket,
            1,
            Op::Register {
                profile: profile(author, handle),
            },
        )
        .await?;
    }
    let home = Home(std::env::temp_dir().join(types::new_id("shared-runtime-ws")?));
    let deployment = RuntimeDeployment {
        namespace: "shared-runtime-ws".into(),
        witness: "a".repeat(64),
        source: "fixture-cell".into(),
    };
    let writer = AnchorWriter::create_runtime(&home.0, deployment.clone())?;
    let original = writer.reader().checkpoint()?;
    let principals = [&first, &second]
        .into_iter()
        .map(|author| {
            Ok(ManagedPrincipal {
                agent: author.identity.clone(),
                device: author.device.clone().ok_or("missing principal device")?,
            })
        })
        .collect::<Result<Vec<_>, Box<dyn std::error::Error>>>()?;
    test.log
        .managed_registry
        .set(ManagedDevices::open_with_runtimes(
            [],
            [ManagedRuntime {
                deployment,
                anchor_directory: home.0.clone(),
                principals,
            }],
        )?)
        .map_err(|_| "test registry already configured")?;
    for (author, socket) in [(&first, &mut one), (&second, &mut two)] {
        assert_eq!(
            request(
                socket,
                2,
                Op::PublishKeyPackages {
                    packages: vec!["invalid publication must never reach validation".into()],
                    last_resort: None,
                }
            )
            .await
            .err()
            .as_deref(),
            Some("managed key packages require native custody")
        );
        send(
            socket,
            ClientFrame::Publish {
                env: envelope(author, "shared-omission")?,
            },
        )
        .await;
        loop {
            match recv(socket).await {
                Some(ServerFrame::Rejected {
                    client_id, reason, ..
                }) if client_id == "shared-omission" => {
                    assert_eq!(reason, "managed runtime binding required");
                    break;
                }
                Some(ServerFrame::Accepted { .. }) => {
                    return Err("shared principal downgraded to ordinary publication".into())
                }
                Some(_) => {}
                None => return Err("managed socket closed".into()),
            }
        }
    }
    let stock: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM key_packages")
        .fetch_one(&test.state.pool)
        .await?;
    assert_eq!(stock, 0);
    assert!(test
        .log
        .saved
        .lock()
        .map_err(|_| "fixture lock")?
        .is_empty());
    assert_eq!(writer.reader().checkpoint()?, original);
    test.log.release.add_permits(1);
    send(
        &mut other,
        ClientFrame::Publish {
            env: envelope(&ordinary, "ordinary-control")?,
        },
    )
    .await;
    loop {
        match recv(&mut other).await {
            Some(ServerFrame::Accepted { client_id, .. }) if client_id == "ordinary-control" => {
                break
            }
            Some(ServerFrame::Rejected { reason, .. }) => return Err(reason.into()),
            Some(_) => {}
            None => return Err("ordinary socket closed".into()),
        }
    }
    assert_eq!(test.log.saved.lock().map_err(|_| "fixture lock")?.len(), 1);
    one.close(None).await?;
    two.close(None).await?;
    other.close(None).await?;
    test.close().await;
    Ok(())
}

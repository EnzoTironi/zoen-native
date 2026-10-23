//! Real sockets prove omission cannot downgrade a configured managed device.
use super::*;
use relay_admission::managed_images::{AnchorWriter, Deployment};

struct Home(std::path::PathBuf);
impl Drop for Home {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn envelope(author: &Author, id: &str) -> Envelope {
    Envelope::plain(&author.sign_event(
        "managed-omission-test",
        id,
        1,
        None,
        types::EventBody::MessagePosted {
            message: id.into(),
            text: "Fixture".into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        },
    ))
}

#[tokio::test]
async fn managed_socket_cannot_omit_binding_or_use_ordinary_publish_while_other_device_keeps_working(
) -> Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await;
    let root = Signer::generate();
    let primary = Author::device(&root, Signer::generate());
    let ordinary = Author::device(&root, Signer::generate());
    let mut first = connect(&test.ws_url, &primary).await;
    request(
        &mut first,
        1,
        Op::Register {
            profile: profile(&primary, "managedsocket"),
        },
    )
    .await?;
    request(&mut first, 2, link_op(&ordinary, &Signer::generate().id())).await?;
    let mut other = connect(&test.ws_url, &ordinary).await;
    let home = Home(std::env::temp_dir().join(types::new_id("managed-ws")));
    let device = primary.device.as_deref().ok_or("missing device")?;
    let deployment = Deployment {
        namespace: "managed-ws".into(),
        witness: "a".repeat(64),
        source: "fixture-cell".into(),
        agent: primary.identity.clone(),
        device: device.into(),
    };
    let writer = AnchorWriter::create(&home.0, deployment.clone())?;
    let registry =
        zoen_relay::managed_devices::ManagedDevices::open([(deployment, home.0.clone())])?;
    test.log
        .managed_registry
        .set(registry)
        .map_err(|_| "test registry already configured")?;
    for id in ["omitted", "ordinary-after-refused-binding"] {
        if id != "omitted" {
            assert!(request(
                &mut first,
                3,
                Op::RuntimeBinding(protocol::runtime_binding::Request {
                    namespace: "managed-ws".into(),
                    nonce: vec![0; 32],
                    managed: None,
                })
            )
            .await
            .is_err());
        }
        send(
            &mut first,
            ClientFrame::Publish {
                env: envelope(&primary, id),
            },
        )
        .await;
        loop {
            match recv(&mut first).await {
                Some(ServerFrame::Rejected {
                    client_id, reason, ..
                }) if client_id == id => {
                    assert_eq!(reason, "managed runtime binding required");
                    break;
                }
                Some(ServerFrame::Accepted { .. }) => {
                    return Err("managed omission was accepted".into())
                }
                Some(_) => {}
                None => return Err("managed socket unexpectedly closed".into()),
            }
        }
    }
    assert!(test
        .log
        .saved
        .lock()
        .map_err(|_| "fixture saved lock")?
        .is_empty());
    test.log.release.add_permits(1);
    send(
        &mut other,
        ClientFrame::Publish {
            env: envelope(&ordinary, "ordinary-control"),
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
            None => return Err("ordinary control closed".into()),
        }
    }
    assert_eq!(
        test.log
            .saved
            .lock()
            .map_err(|_| "fixture saved lock")?
            .len(),
        1
    );
    first.close(None).await?;
    other.close(None).await?;
    test.close().await;
    drop(writer);
    Ok(())
}

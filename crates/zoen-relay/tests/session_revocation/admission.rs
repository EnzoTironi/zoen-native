use super::*;

#[tokio::test]
async fn a_protocol_three_client_is_refused_before_authentication_or_linking(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let primary = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut current = connect(&test.ws_url, &primary).await;
    request(
        &mut current,
        1,
        Op::Register {
            profile: profile(&primary, "ana"),
        },
    )
    .await
    .unwrap();
    let _ = current.close(None).await;

    // An enrolled sponsor with a valid certificate still needs the enrollment-aware
    // client. Refusal must be the first frame, before a challenge or link request.
    let (mut old, _) = tokio_tungstenite::connect_async(&test.ws_url)
        .await
        .unwrap();
    send(
        &mut old,
        ClientFrame::Hello {
            protocol: 3,
            capabilities: Vec::new(),
            identity: primary.identity.clone(),
            device: primary.device.clone().unwrap(),
            cert: primary.cert.clone().unwrap(),
        },
    )
    .await;
    match recv(&mut old).await {
        Some(ServerFrame::Error { code, message }) => {
            assert_eq!(code, ErrorCode::UpgradeRequired);
            assert!(message.contains("needs 4 or newer"), "{message}");
        }
        other => panic!("expected an upgrade error before authentication, got {other:?}"),
    }
    assert!(recv(&mut old).await.is_none());
    test.close().await;
    Ok(())
}

#[tokio::test]
async fn an_unlinked_root_holder_cannot_return_with_a_fresh_certificate(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let primary = Author::device(&root, Signer::generate()?);
    let lost = Author::device(&root, Signer::generate()?);
    let mut controller = connect(&test.ws_url, &primary).await;
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&primary, "ana"),
        },
    )
    .await
    .unwrap();
    request(
        &mut controller,
        2,
        link_op(&lost, &Signer::generate()?.id()),
    )
    .await
    .unwrap();
    let mut target = connect(&test.ws_url, &lost).await;
    request(
        &mut controller,
        3,
        Op::Unlink {
            device: lost.device.clone().unwrap(),
        },
    )
    .await
    .unwrap();
    // A linked device retains the root secret. Minting a valid new certificate and
    // proving the new key now succeeds cryptographically, but grants no enrollment.
    let fresh = Author::device(&root, Signer::generate()?);
    let (mut attempted, result) = authenticate(&test.ws_url, &fresh).await;
    assert!(
        matches!(
            result,
            ServerFrame::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ),
        "{result:?}"
    );
    assert!(recv(&mut attempted).await.is_none());
    assert_device_absent(&test, &fresh).await;
    while let Some(frame) = recv(&mut target).await {
        assert!(matches!(
            frame,
            ServerFrame::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ));
    }
    // Even an active sponsor cannot turn the old device's row back on.
    let box_id = Signer::generate()?.id();
    assert!(request(&mut controller, 4, link_op(&lost, &box_id))
        .await
        .is_err());
    assert!(
        zoen_relay::db::device_revoked(&test.state.pool, lost.device.as_ref().unwrap())
            .await
            .unwrap()
    );
    assert!(zoen_relay::db::read_link_box(&test.state.pool, &box_id)
        .await
        .unwrap()
        .is_none());
    // The surviving enrolled device can still change its own directory profile.
    request(
        &mut controller,
        5,
        Op::Register {
            profile: profile(&primary, "ana_changed"),
        },
    )
    .await
    .unwrap();
    let handle: String = sqlx::query_scalar("SELECT handle FROM identities WHERE id = $1")
        .bind(root.id())
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(handle, "ana_changed");
    let _ = controller.close(None).await;
    test.close().await;
    Ok(())
}

#[tokio::test]
async fn embedded_payloads_must_be_signed_by_the_authorized_session_device(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let primary = Author::device(&root, Signer::generate()?);
    let lost = Author::device(&root, Signer::generate()?);
    let mut controller = connect(&test.ws_url, &primary).await;
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&primary, "ana"),
        },
    )
    .await
    .unwrap();
    request(
        &mut controller,
        2,
        link_op(&lost, &Signer::generate()?.id()),
    )
    .await
    .unwrap();

    let profile_key = ProfileKey::generate()?;
    let fields = ProfileFields {
        name: "Ana".into(),
        bio: String::new(),
        photo: None,
    };
    for revoked in [false, true] {
        let id = if revoked { 10 } else { 3 };
        if revoked {
            request(
                &mut controller,
                id,
                Op::Unlink {
                    device: lost.device.clone().unwrap(),
                },
            )
            .await
            .unwrap();
        }
        assert_eq!(
            zoen_relay::db::device_revoked(&test.state.pool, lost.device.as_deref().unwrap())
                .await
                .unwrap(),
            revoked
        );

        // The active session can still publish its own signatures after revocation.
        let public = AgreementKey::generate()?.public_hex();
        let agreement = AgreementKeyRecord {
            identity: primary.identity.clone(),
            signed: sign_agreement(&primary, &public).unwrap(),
            public,
        };
        assert_eq!(
            request(
                &mut controller,
                id + 1,
                Op::PublishAgreementKey {
                    public: agreement.public.clone(),
                    signed: agreement.signed.clone(),
                },
            )
            .await,
            Ok(Reply::Done)
        );
        let version = if revoked { 3 } else { 1 };
        let ciphertext = seal_profile(&profile_key, &primary.identity, version, &fields)?;
        let upload = SealedProfile {
            identity: primary.identity.clone(),
            version,
            signed: sign_upload(&primary, version, &ciphertext).unwrap(),
            ciphertext,
        };
        assert_eq!(
            request(
                &mut controller,
                id + 2,
                Op::PutProfile {
                    profile: upload.clone(),
                },
            )
            .await,
            Ok(Reply::Done)
        );

        // Another device's valid certificate and signatures cannot borrow this session's
        // authority, whether that device is still active or has already been revoked.
        let public = AgreementKey::generate()?.public_hex();
        let signed = sign_agreement(&lost, &public).unwrap();
        assert!(verify_agreement(&primary.identity, &public, &signed));
        assert_eq!(
            request(
                &mut controller,
                id + 3,
                Op::PublishAgreementKey { public, signed },
            )
            .await,
            Err("agreement key is for another device".into())
        );
        let version = version + 1;
        let ciphertext = seal_profile(&profile_key, &primary.identity, version, &fields)?;
        let signed = sign_upload(&lost, version, &ciphertext).unwrap();
        assert!(verify_upload(
            &primary.identity,
            version,
            &ciphertext,
            &signed
        ));
        assert_eq!(
            request(
                &mut controller,
                id + 4,
                Op::PutProfile {
                    profile: SealedProfile {
                        identity: primary.identity.clone(),
                        version,
                        ciphertext,
                        signed,
                    },
                },
            )
            .await,
            Err("profile is for another device".into())
        );
        let ids = [primary.identity.clone()];
        assert_eq!(
            zoen_relay::db::agreement_keys(&test.state.pool, &ids)
                .await
                .unwrap(),
            vec![agreement]
        );
        assert_eq!(
            zoen_relay::db::sealed_profiles(&test.state.pool, &ids)
                .await
                .unwrap(),
            vec![upload]
        );
    }
    let _ = controller.close(None).await;
    test.close().await;
    Ok(())
}

#[tokio::test]
async fn a_preopened_unregistered_socket_cannot_enroll_after_registration(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let test = TestRelay::new().await?;
    let root = Signer::generate()?;
    let first = Author::device(&root, Signer::generate()?);
    let late = Author::device(&root, Signer::generate()?);
    let (mut controller, first_ready) = authenticate(&test.ws_url, &first).await;
    let (mut stale, late_ready) = authenticate(&test.ws_url, &late).await;
    assert!(matches!(
        first_ready,
        ServerFrame::Ready {
            registered: false,
            ..
        }
    ));
    assert!(matches!(
        late_ready,
        ServerFrame::Ready {
            registered: false,
            ..
        }
    ));
    request(
        &mut controller,
        1,
        Op::Register {
            profile: profile(&first, "ana"),
        },
    )
    .await
    .unwrap();
    send(
        &mut stale,
        ClientFrame::Req {
            id: 2,
            op: Op::Register {
                profile: profile(&late, "hijacked"),
            },
        },
    )
    .await;
    while let Some(frame) = recv(&mut stale).await {
        assert!(matches!(
            frame,
            ServerFrame::Error {
                code: ErrorCode::Unauthorized,
                ..
            }
        ));
    }
    // This is also refused at the SQL creation boundary if identity registration wins
    // after the unregistered request's admission check, before its INSERT executes.
    let mut tx = test.state.pool.begin().await.unwrap();
    assert!(zoen_relay::db::register(
        &mut tx,
        &profile(&late, "hijacked"),
        "hijacked",
        late.device.as_ref().unwrap(),
        late.cert.as_ref().unwrap(),
        true,
    )
    .await
    .is_err());
    tx.rollback().await.unwrap();
    let mut tx = test.state.pool.begin().await.unwrap();
    assert!(zoen_relay::db::register(
        &mut tx,
        &profile(&late, "hijacked"),
        "hijacked",
        late.device.as_ref().unwrap(),
        late.cert.as_ref().unwrap(),
        false,
    )
    .await
    .is_err());
    tx.rollback().await.unwrap();
    let handle: String = sqlx::query_scalar("SELECT handle FROM identities WHERE id = $1")
        .bind(root.id())
        .fetch_one(&test.state.pool)
        .await
        .unwrap();
    assert_eq!(handle, "ana");
    assert_device_absent(&test, &late).await;
    let _ = controller.close(None).await;
    test.close().await;
    Ok(())
}

//! Allocation completion belongs to the same live pair, device and Net.
use super::*;

async fn ready(pair: &Pair, listener: &TcpListener) -> TestResult<Socket> {
    pair.client.start_sync(None)?;
    let mut socket = authenticated(listener).await?;
    send(&mut socket, ServerFrame::SyncDone).await?;
    barrier(&mut socket, &pair.client).await?;
    assert!(pair.client.connection().synced);
    Ok(socket)
}

#[tokio::test]
async fn completed_old_allocation_cannot_escape_after_same_owner_net_replacement() -> TestResult {
    let (pair, listener) = Pair::new().await?;
    let before = pair.pending();
    let mut socket = ready(&pair, &listener).await?;
    let turn = pair.turn();
    tokio::pin!(turn);
    poll_pending(turn.as_mut()).await;
    let id = answer_until_turn(&mut socket, &pair).await?;
    send(
        &mut socket,
        ServerFrame::Res {
            id,
            result: Ok(Reply::CallTurn(credentials())),
        },
    )
    .await?;
    // Let Net fully receive the response, but hold the API continuation until
    // a different authenticated Net serves the exact same account/device/URL.
    barrier(&mut socket, &pair.client).await?;
    pair.client.stop_sync();
    let mut replacement = ready(&pair, &listener).await?;
    assert!(
        matches!(turn.await, Err(CoreError::Invalid { reason }) if reason == "Call connection changed")
    );
    assert_eq!(pair.pending(), before);

    let retry = pair.turn();
    let relay = async {
        let id = answer_until_turn(&mut replacement, &pair).await?;
        send(
            &mut replacement,
            ServerFrame::Res {
                id,
                result: Ok(Reply::CallTurn(credentials())),
            },
        )
        .await
    };
    let (result, served) = tokio::join!(retry, relay);
    served?;
    assert!(
        result.is_ok(),
        "a fresh explicitly requested allocation may recover"
    );
    assert_eq!(pair.pending(), before);
    Ok(())
}

#[tokio::test]
async fn blocked_pair_during_initial_catchup_never_reaches_turn_transport() -> TestResult {
    let (pair, listener) = Pair::new().await?;
    pair.client.start_sync(None)?;
    let mut socket = authenticated(&listener).await?;
    let turn = pair.turn();
    tokio::pin!(turn);
    poll_pending(turn.as_mut()).await;
    barrier(&mut socket, &pair.client).await?;
    pair.client.lock().block_person(&pair.peer)?;
    assert!(matches!(turn.await, Err(CoreError::Forbidden { .. })));
    send(&mut socket, ServerFrame::SyncDone).await?;
    barrier(&mut socket, &pair.client).await?;
    Ok(())
}

#[tokio::test]
async fn lost_allocation_response_reconnects_but_waits_for_fresh_catchup() -> TestResult {
    let (pair, listener) = Pair::new().await?;
    let before = pair.pending();
    let mut socket = ready(&pair, &listener).await?;
    let turn = pair.turn();
    tokio::pin!(turn);
    poll_pending(turn.as_mut()).await;
    let _unanswered_id = answer_until_turn(&mut socket, &pair).await?;
    drop(socket);
    assert!(
        turn.await.is_err(),
        "transport loss cannot manufacture an allocation"
    );
    // The same Net reconnects itself. No flush, user poke or global drain is
    // needed, and new authentication does not reuse the prior catch-up result.
    let mut socket = authenticated(&listener).await?;
    let retry = pair.turn();
    tokio::pin!(retry);
    poll_pending(retry.as_mut()).await;
    barrier(&mut socket, &pair.client).await?;
    assert!(!pair.client.connection().synced);
    send(&mut socket, ServerFrame::SyncDone).await?;
    let relay = async {
        let id = answer_until_turn(&mut socket, &pair).await?;
        send(
            &mut socket,
            ServerFrame::Res {
                id,
                result: Ok(Reply::CallTurn(credentials())),
            },
        )
        .await
    };
    let (result, served) = tokio::join!(retry, relay);
    served?;
    assert!(result.is_ok());
    assert_eq!(pair.pending(), before);
    Ok(())
}

enum Change {
    Blocked,
    Reader,
    Removed,
    RevokedDevice,
    InvalidRevocationCache,
}

fn membership(pair: &Pair, body: EventBody) -> TestResult<Sequenced> {
    let engine = pair.client.lock();
    let author = engine.net.author.as_ref().expect("fixture author");
    let mut log = engine.logs[&pair.space].clone();
    let event = author.sign_event(&pair.space, "canonical-pair-change", 10, log.head(), body)?;
    let event = log.sequence(event)?;
    Ok(Sequenced {
        seq: event.seq,
        prev: event.prev.clone(),
        hash: event.hash.clone(),
        env: Envelope::plain(event)?,
    })
}

#[tokio::test]
async fn held_reply_rechecks_canonical_membership_blocking_and_current_device_access() -> TestResult
{
    for change in [
        Change::Blocked,
        Change::Reader,
        Change::Removed,
        Change::RevokedDevice,
        Change::InvalidRevocationCache,
    ] {
        let (pair, listener) = Pair::new().await?;
        let mut socket = ready(&pair, &listener).await?;
        let turn = pair.turn();
        tokio::pin!(turn);
        poll_pending(turn.as_mut()).await;
        let id = answer_until_turn(&mut socket, &pair).await?;
        send(
            &mut socket,
            ServerFrame::Res {
                id,
                result: Ok(Reply::CallTurn(credentials())),
            },
        )
        .await?;
        // Canonical membership changes are genuine signed sequenced ingress;
        // local block/revocation policies use their actual durable owners.
        match change {
            Change::Blocked => pair.client.lock().block_person(&pair.peer)?,
            Change::Reader | Change::Removed => {
                let body = if matches!(change, Change::Reader) {
                    EventBody::MemberAdded {
                        identity: pair.peer.clone(),
                        role: Role::Reader,
                    }
                } else {
                    EventBody::MemberRemoved {
                        identity: pair.peer.clone(),
                    }
                };
                let ev = membership(&pair, body)?;
                send(&mut socket, ServerFrame::Event { ev }).await?;
            }
            Change::RevokedDevice => pair.client.lock().unlinked(&pair.device)?,
            Change::InvalidRevocationCache => pair
                .client
                .lock()
                .store
                .set_meta("devices.revoked", "not-json")?,
        }
        barrier(&mut socket, &pair.client).await?;
        let result = turn.await;
        assert!(
            matches!(
                result,
                Err(CoreError::Forbidden { .. } | CoreError::Storage { .. })
            ),
            "stale allocation result: {result:?}"
        );
    }
    Ok(())
}

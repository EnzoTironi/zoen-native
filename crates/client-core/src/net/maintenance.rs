//! A completed catch-up permits one existing ordered maintenance pass.

use super::{
    complete_reads, flush, free, send, tracing_like, ClaimClockSample, ClientFrame, CoreListener,
    Ctx, Duration, EphemeralKind, Exit, HashMap, HashSet, Op, ProfileTraffic, Sink, StreamExt,
    Waiting, PROFILE_TIMEOUT,
};

/// Borrows the active session; the caller still owns scheduling and watchdogs.
pub(super) struct MaintenanceContext<'a> {
    pub(super) ctx: &'a Ctx,
    pub(super) sink: &'a mut Sink,
    pub(super) sent: &'a mut HashMap<String, Option<u64>>,
    pub(super) waiting: &'a mut HashMap<u64, Waiting>,
    pub(super) next_id: &'a mut u64,
    pub(super) dirty: &'a mut HashSet<String>,
    pub(super) profiles_inflight: &'a mut Option<tokio::time::Instant>,
    pub(super) traffic: &'a mut ProfileTraffic,
    pub(super) opening: &'a mut Option<tokio::time::Instant>,
    pub(super) claim_clock: &'a Option<ClaimClockSample>,
    pub(super) claim_refusal_reported: &'a mut bool,
    pub(super) media_kick: &'a tokio::sync::Notify,
    pub(super) read_owner: Option<&'a (String, String)>,
    pub(super) deferred_reads: &'a mut complete_reads::Deferred,
}

pub(super) async fn maintain(context: MaintenanceContext<'_>) -> Result<(), Exit> {
    let MaintenanceContext {
        ctx,
        sink,
        sent,
        waiting,
        next_id,
        dirty,
        profiles_inflight,
        traffic,
        opening,
        claim_clock,
        claim_refusal_reported,
        media_kick,
        read_owner,
        deferred_reads,
    } = context;
    macro_rules! engine {
        ($work:expr) => {
            match ctx.engine($work) {
                Some(value) => value,
                None => return Err(Exit::Stop),
            }
        };
    }
    if profiles_inflight.is_none() {
        let unknown = engine!(super::super::engine::Engine::take_unknown);
        if !unknown.is_empty() {
            let id = *next_id;
            *next_id += 1;
            if let Err(e) = send(
                sink,
                &ClientFrame::Req {
                    id,
                    op: Op::Profiles {
                        ids: unknown.clone(),
                    },
                },
            )
            .await
            {
                return Err(Exit::Retry(e));
            }
            waiting.insert(id, Waiting::Profiles(unknown));
            *profiles_inflight = Some(tokio::time::Instant::now());
        }
    }
    let shared = engine!(super::super::engine::Engine::profile_share_pass);
    match shared {
        Ok(true) => {
            if let Err(e) = flush(ctx, sink, sent).await {
                return Err(Exit::Retry(e));
            }
        }
        Ok(false) => {}
        Err(e) => tracing_like(&format!("profile shares: {e}")),
    }
    let mut reqs: Vec<(Op, Waiting)> = Vec::new();
    if free(&mut traffic.agreement) {
        let publish = engine!(|engine| engine.agreement_to_publish());
        if let Some((public, signed)) = publish {
            traffic.agreement = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
            reqs.push((
                Op::PublishAgreementKey {
                    public: public.clone(),
                    signed,
                },
                Waiting::AgreementPublished(public),
            ));
        }
    }
    if free(&mut traffic.upload) {
        let upload = engine!(super::super::engine::Engine::profile_to_upload);
        if let Some(profile) = upload {
            traffic.upload = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
            let version = profile.version;
            reqs.push((
                Op::PutProfile { profile },
                Waiting::ProfileUploaded(version),
            ));
        }
    }
    if free(&mut traffic.keys) {
        let ids = engine!(|engine| engine.take_need_agreement());
        if !ids.is_empty() {
            traffic.keys = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
            reqs.push((
                Op::AgreementKeys { ids: ids.clone() },
                Waiting::AgreementKeys(ids),
            ));
        }
    }
    if free(&mut traffic.profiles) {
        let ids = engine!(super::super::engine::Engine::take_need_profiles);
        if !ids.is_empty() {
            traffic.profiles = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
            *opening = Some(tokio::time::Instant::now());
            reqs.push((
                Op::GetProfiles { ids: ids.clone() },
                Waiting::SealedProfiles(ids),
            ));
        }
    }
    if free(&mut traffic.key_packages) {
        let publish = engine!(super::super::engine::Engine::mls_key_packages_to_publish);
        if let Some((packages, last_resort)) = publish {
            traffic.key_packages = Some(tokio::time::Instant::now() + PROFILE_TIMEOUT);
            reqs.push((
                Op::PublishKeyPackages {
                    packages,
                    last_resort,
                },
                Waiting::KeyPackagesPublished,
            ));
        }
    }
    for (to, sealed) in engine!(super::super::engine::Engine::take_device_outbox) {
        tracing_like(&format!(
            "device message to {} ({} bytes)",
            &to[..12.min(to.len())],
            sealed.len()
        ));
        reqs.push((Op::SendDevice { to, sealed }, Waiting::Ignored));
    }
    if engine!(super::super::engine::Engine::take_ask_devices) {
        reqs.push((Op::Devices, Waiting::Devices));
    }
    match engine!(super::super::engine::Engine::mls_recovery_pass) {
        Ok(true) => media_kick.notify_one(),
        Ok(false) => {}
        Err(e) => tracing_like(&format!("group recovery: {e}")),
    }
    let claim = engine!(
        |engine| engine.mls_to_claim(claim_clock.as_ref().and_then(ClaimClockSample::fresh))
    );
    match claim {
        Ok(Some((space, ids, operation))) => reqs.push((
            Op::ClaimKeyPackages {
                ids,
                operation_id: Some(operation.clone()),
            },
            Waiting::Claimed { space, operation },
        )),
        Ok(None) => {}
        Err(error) if !*claim_refusal_reported => {
            let message = error.to_string();
            tracing_like(&message);
            ctx.set(|s| s.error = Some(message.clone()));
            if let Some(listener) = &ctx.listener {
                listener.on_error(message);
            }
            *claim_refusal_reported = true;
        }
        Err(_) => {}
    }
    let checkpoints = engine!(super::super::engine::Engine::mls_checkpoints);
    match checkpoints {
        Ok(true) => {
            if let Err(e) = flush(ctx, sink, sent).await {
                return Err(Exit::Retry(e));
            }
        }
        Ok(false) => {}
        Err(e) => tracing_like(&format!("checkpoints: {e}")),
    }
    let sealed = engine!(super::super::engine::Engine::mls_seal_outbox);
    match sealed {
        Ok(true) => {
            if let Err(e) = flush(ctx, sink, sent).await {
                return Err(Exit::Retry(e));
            }
        }
        Ok(false) => {}
        Err(e) => tracing_like(&format!("sealing: {e}")),
    }
    let mut failed = None;
    for (op, w) in reqs {
        let (op, w) = complete_reads::start(op, w, read_owner);
        if let Err(e) = complete_reads::submit(op, w, sink, waiting, next_id, deferred_reads).await
        {
            failed = Some(e);
            break;
        }
    }
    if let Some(e) = failed {
        return Err(Exit::Retry(e));
    }
    // Hold UI updates briefly while we learn who new authors are, so the
    // app doesn't flash "Unknown" (or an @handle) for someone about to get a name.
    let hold = |t: &Option<tokio::time::Instant>| {
        t.is_some_and(|t| t.elapsed() < Duration::from_millis(1500))
    };
    let naming = hold(profiles_inflight) || hold(opening);
    if !dirty.is_empty() && !naming {
        let spaces: Vec<String> = dirty.drain().filter(|s| !s.is_empty()).collect();
        // ✓✓ for the senders: one aggregated watermark per Space whose head moved.
        let marks: Vec<(String, u64)> = engine!(|engine| spaces
            .iter()
            .filter_map(|s| engine.delivered_to_announce(s).map(|q| (s.clone(), q)))
            .collect());
        for (space, seq) in marks {
            if let Err(e) = send(
                sink,
                &ClientFrame::Ephemeral {
                    space: space.clone(),
                    kind: EphemeralKind::Delivered { seq },
                },
            )
            .await
            {
                failed = Some(e);
                break;
            }
            engine!(|engine| engine.delivered_announced(&space, seq));
        }
        if let Some(e) = failed {
            return Err(Exit::Retry(e));
        }
        if let Some(l) = &ctx.listener {
            l.on_change(spaces);
        }
        ctx.notify_connection();
    }
    Ok(())
}

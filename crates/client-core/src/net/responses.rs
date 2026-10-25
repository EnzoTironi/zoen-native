//! Correlated responses settle only the current request on this socket.

use super::{
    complete_reads, flush, settle, tracing_like, Ctx, Exit, HashMap, HashSet, ProfileTraffic,
    Reply, Sink, Waiting,
};

pub(super) struct ReplyContext<'a> {
    pub(super) ctx: &'a Ctx,
    pub(super) sink: &'a mut Sink,
    pub(super) sent: &'a mut HashMap<String, Option<u64>>,
    pub(super) waiting: &'a mut HashMap<u64, Waiting>,
    pub(super) profiles_inflight: &'a mut Option<tokio::time::Instant>,
    pub(super) traffic: &'a mut ProfileTraffic,
    pub(super) opening: &'a mut Option<tokio::time::Instant>,
    pub(super) dirty: &'a mut HashSet<String>,
    pub(super) media_kick: &'a tokio::sync::Notify,
}

pub(super) async fn dispatch_reply(
    id: u64,
    result: Result<Reply, String>,
    context: ReplyContext<'_>,
) -> Result<(), Exit> {
    let ReplyContext {
        ctx,
        sink,
        sent,
        waiting,
        profiles_inflight,
        traffic,
        opening,
        dirty,
        media_kick,
    } = context;
    macro_rules! engine {
        ($work:expr) => {
            match ctx.engine($work) {
                Some(value) => value,
                None => return Err(Exit::Stop),
            }
        };
    }
    let (waiter, result) = match waiting.remove(&id) {
        Some(Waiting::Paged {
            assembly,
            done,
            deadline,
        }) => {
            match complete_reads::page(id, result, assembly, done, deadline, sink, waiting).await? {
                Some((waiter, result)) => (Some(waiter), result),
                None => return Ok(()),
            }
        }
        waiter => (waiter, result),
    };
    match waiter {
        Some(Waiting::External(tx)) => {
            let _ = tx.send(result);
        }
        Some(Waiting::Devices) => {
            if let Ok(Reply::Devices(list)) = result {
                engine!(|engine| engine.devices_arrived(list))
                    .map_err(|e| Exit::Retry(e.to_string()))?;
            }
            if let Err(e) = flush(ctx, sink, sent).await {
                return Err(Exit::Retry(e));
            }
        }
        Some(Waiting::Ignored) => {
            if let Err(e) = result {
                tracing_like(&format!("device message: {e}"));
            }
        }
        Some(Waiting::Profiles(asked)) => {
            *profiles_inflight = None;
            engine!(|eng| {
                if let Ok(Reply::Profiles(list)) = result {
                    if !list.is_empty() && eng.put_profiles(list).is_ok() {
                        dirty.insert(String::new());
                    }
                }
                eng.profiles_answered(&asked);
            });
        }
        Some(Waiting::AgreementPublished(public)) => {
            let ok = matches!(result, Ok(Reply::Done));
            if ok {
                engine!(|engine| engine.agreement_published(&public));
            } else {
                tracing_like(&format!("agreement key refused: {result:?}"));
            }
            settle(&mut traffic.agreement, ok);
        }
        Some(Waiting::ProfileUploaded(version)) => {
            let ok = matches!(result, Ok(Reply::Done));
            let answer = result.map(|_| ());
            engine!(|engine| engine.profile_upload_answered(version, answer));
            settle(&mut traffic.upload, ok);
        }
        Some(Waiting::AgreementKeys(asked)) => {
            let ok = if let Ok(Reply::AgreementKeys(list)) = result {
                engine!(|engine| engine.agreement_keys_arrived(list, &asked))
            } else {
                false
            };
            settle(&mut traffic.keys, ok);
        }
        Some(Waiting::SealedProfiles(asked)) => {
            *opening = None;
            let request_error = result.is_err();
            let ok = if let Ok(Reply::SealedProfiles(list)) = result {
                let received = list.len();
                let (changed, complete, remaining, own_pending) = engine!(|engine| {
                    let changed = engine.sealed_profiles_arrived(list, &asked);
                    let complete = engine.profiles_answer_complete(&asked);
                    let pending = &engine.net.profiles.need_profiles;
                    let own_pending = engine.me.as_ref().is_some_and(|me| pending.contains(me));
                    (changed, complete, pending.len(), own_pending)
                });
                tracing_like(&format!(
                    "sealed profiles response asked={} received={received} complete={complete} remaining={remaining} own_pending={own_pending}", asked.len()
                ));
                if !changed.is_empty() {
                    dirty.insert(String::new());
                    media_kick.notify_one();
                    if let Some(l) = &ctx.listener {
                        for id in changed {
                            l.on_profile_changed(id);
                        }
                    }
                }
                complete
            } else {
                tracing_like(&format!(
                    "sealed profiles unusable reply asked={} request_error={request_error}",
                    asked.len()
                ));
                false
            };
            settle(&mut traffic.profiles, ok);
        }
        Some(Waiting::KeyPackagesPublished) => {
            let ok = matches!(result, Ok(Reply::Done));
            engine!(|engine| engine.mls_key_packages_published(result.map(|_| ())));
            settle(&mut traffic.key_packages, ok);
        }
        Some(Waiting::Claimed { space, operation }) => {
            let packages = match result {
                Ok(Reply::KeyPackages(list)) => Ok(list),
                Ok(other) => Err(format!("unexpected {other:?}")),
                Err(e) => Err(e),
            };
            engine!(|engine| engine.mls_claimed(&space, &operation, packages));
            if let Err(e) = flush(ctx, sink, sent).await {
                return Err(Exit::Retry(e));
            }
        }
        Some(Waiting::Paged { .. }) => return Err(Exit::Retry("nested complete read".into())),
        None => {}
    }
    Ok(())
}

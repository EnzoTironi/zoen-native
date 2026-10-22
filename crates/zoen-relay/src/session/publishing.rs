use super::*;

impl Session {
    /// What the metrics learn from a sequenced envelope: its kind and who it went to,
    /// never its content (sealed envelopes only say whether they're application data).
    pub(super) fn count_sequenced(
        &self,
        env: &protocol::Envelope,
        ev: &protocol::Sequenced,
        audience: &[String],
        joined: Option<&str>,
        started: std::time::Instant,
    ) {
        let a = &self.st.analytics;
        a.count("publish_ok", 1);
        a.latency("send_ms", started.elapsed());
        let is_message = match ev.env.body() {
            Some(EventBody::MessagePosted { .. }) => true,
            Some(EventBody::SpaceCreated { kind, .. }) => {
                a.count(
                    &format!("spaces_created_{}", format!("{kind:?}").to_lowercase()),
                    1,
                );
                false
            }
            Some(EventBody::RequestResolved { approved, .. }) => {
                a.count(
                    if approved {
                        "approvals_approved"
                    } else {
                        "approvals_denied"
                    },
                    1,
                );
                false
            }
            _ => env.sealed_kind() == Some(protocol::SealedKind::Application),
        };
        if is_message {
            a.message_sent(crate::analytics::SentMessage {
                author: &self.identity,
                space: env.space(),
                audience,
            });
        }
        if let (Some(_), Some(who)) = (&env.invite, joined) {
            a.invite_accepted(who);
        }
    }

    #[tracing::instrument(
        name = "publish",
        skip_all,
        fields(
            identity = %pseudo(&self.identity),
            space = %pseudo(env.space()),
            partition = i64::from(crate::ownership::partition_of(env.space())),
            outcome = Empty,
            reason = Empty,
            seq = Empty,
            audience = Empty,
            delivered_here = Empty,
        )
    )]
    pub(super) async fn publish(&mut self, env: protocol::Envelope) {
        let started = std::time::Instant::now();
        let span = tracing::Span::current();
        let analytics = &self.st.analytics;
        let reject = |outcome: &'static str, reason: &str, permanent: bool| {
            span.record("outcome", outcome);
            analytics.count("publish_rejected", 1);
            ServerFrame::Rejected {
                space: env.space().to_string(),
                client_id: env.client_id().to_string(),
                reason: reason.to_string(),
                permanent,
            }
        };
        if let Err(wait) = self.publish_allowed() {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self
                .send(reject("rate_limited", &limits::slow_down(wait), false))
                .await;
        }
        if env.stored_len() > MAX_ENVELOPE {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self.send(reject("too_large", "too large", true)).await;
        }
        if !env.valid_invite() {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self
                .send(reject("invalid_invite", "invalid invite code", true))
                .await;
        }
        if env.author() != self.identity || env.device().is_some_and(|d| d != self.device) {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self
                .send(reject(
                    "wrong_author",
                    "you can only publish as yourself, from this device",
                    true,
                ))
                .await;
        }
        if let Err(e) = env.verify() {
            Metrics::inc(&self.st.metrics.events_rejected);
            return self
                .send(reject("bad_signature", &format!("signature: {e}"), true))
                .await;
        }
        let target_known = match env.body() {
            Some(EventBody::MemberAdded { identity, .. }) => {
                match db::is_registered(&self.st.pool, &identity).await {
                    Ok(known) => known,
                    Err(_) => {
                        Metrics::inc(&self.st.metrics.events_rejected);
                        self.send(reject(
                            "directory_unavailable",
                            "directory unavailable",
                            false,
                        ))
                        .await;
                        return;
                    }
                }
            }
            _ => true,
        };
        match self.st.log.append(&env, target_known).await {
            Ok(Sequencing::Duplicate {
                ev,
                audience,
                joined,
            }) => {
                Metrics::inc(&self.st.metrics.events_duplicate);
                span.record("outcome", "duplicate");
                span.record("seq", ev.seq as i64);
                self.send(ServerFrame::Accepted {
                    space: env.space().to_string(),
                    client_id: env.client_id().to_string(),
                    seq: ev.seq,
                })
                .await;
                let space = ev.env.space().to_string();
                let n = if fork_histories_hold(&space) {
                    0
                } else {
                    self.send(ServerFrame::Event { ev: ev.clone() }).await;
                    self.st
                        .fanout
                        .send(&audience, &ServerFrame::Event { ev }, self.hub_id)
                };
                span.record("audience", audience.len() as i64);
                span.record("delivered_here", n as i64);
                if let Some(identity) = joined.filter(|id| id != &self.identity) {
                    self.st.fanout.send(
                        std::slice::from_ref(&identity),
                        &ServerFrame::Joined { space },
                        None,
                    );
                }
            }
            Ok(Sequencing::New {
                ev,
                audience,
                joined,
            }) => {
                Metrics::inc(&self.st.metrics.events_sequenced);
                span.record("outcome", "sequenced");
                self.count_sequenced(&env, &ev, &audience, joined.as_deref(), started);
                span.record("seq", ev.seq as i64);
                self.send(ServerFrame::Accepted {
                    space: env.space().to_string(),
                    client_id: env.client_id().to_string(),
                    seq: ev.seq,
                })
                .await;
                let space = ev.env.space().to_string();
                let is_create = matches!(ev.env.body(), Some(EventBody::SpaceCreated { .. }));
                let n = if fork_histories_hold(&space) {
                    0
                } else {
                    self.st
                        .fanout
                        .send(&audience, &ServerFrame::Event { ev }, None)
                };
                self.wake_away(&audience);
                span.record("audience", audience.len() as i64);
                span.record("delivered_here", n as i64);
                if let Some(j) = joined {
                    if j != self.identity {
                        self.st.fanout.send(
                            std::slice::from_ref(&j),
                            &ServerFrame::Joined {
                                space: space.clone(),
                            },
                            None,
                        );
                    }
                }
                if is_create {
                    tracing::info!(space = %pseudo(&space), "space created");
                }
            }
            Err(r) => {
                Metrics::inc(&self.st.metrics.events_rejected);
                span.record("reason", r.reason.as_str());
                tracing::info!(space = %pseudo(env.space()), reason = %r.reason, "rejected");
                self.send(reject("rejected", &r.reason, r.permanent)).await;
            }
        }
    }
}

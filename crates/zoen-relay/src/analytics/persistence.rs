use super::*;

impl Analytics {
    pub(super) async fn write(&self, pool: &PgPool, b: &Pending) -> anyhow::Result<()> {
        let ids: Vec<String> = b
            .accounts
            .keys()
            .map(|(_, id)| id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();
        // What the directory already knows, read in one query: kind, handle (for the test
        // flag only, never stored) and signup time.
        let rows: Vec<(String, String, Option<String>, i64)> = sqlx::query_as(
            "SELECT id, kind, handle, (extract(epoch FROM created_at) * 1000)::bigint
             FROM identities WHERE id = ANY($1)",
        )
        .bind(&ids)
        .fetch_all(pool)
        .await?;
        let info: HashMap<String, (i16, bool, i64)> = rows
            .into_iter()
            .map(|(id, kind, handle, created)| {
                let test = handle.as_deref().is_some_and(|h| self.is_test_handle(h));
                (id, (i16::from(kind == "Agent"), test, created))
            })
            .collect();

        let mut tx = pool.begin().await?;
        let (sources, exposures) = (&b.sources, &b.exposures);

        // Accounts young enough to matter for activation and cohorts.
        let horizon = now_ms() - i64::from(RETENTION_DAYS) * 86_400_000;
        let mut acc_rows: HashMap<String, (bool, Option<i64>)> = HashMap::new();
        for ((_, id), a) in &b.accounts {
            let e = acc_rows.entry(id.clone()).or_default();
            e.0 |= a.invited;
            e.1 = match (e.1, a.first_message_ms) {
                (Some(x), Some(y)) => Some(x.min(y)),
                (x, y) => x.or(y),
            };
        }
        let (mut a_pid, mut a_signup, mut a_kind, mut a_test, mut a_inv, mut a_first) =
            (vec![], vec![], vec![], vec![], vec![], vec![]);
        for (id, (invited, first)) in &acc_rows {
            let Some((kind, test, created)) = info.get(id) else {
                continue;
            };
            if *created < horizon {
                continue;
            }
            a_pid.push(self.pseudonym(b"acct\0", id));
            a_signup.push(*created);
            a_kind.push(*kind);
            a_test.push(*test);
            a_inv.push(*invited);
            a_first.push(*first);
        }
        if !a_pid.is_empty() {
            sqlx::query(
                "INSERT INTO metrics_accounts (pid, signup_at, kind, test, invited, first_message_at)
                 SELECT p, to_timestamp(s / 1000.0), k, t, i, to_timestamp(f / 1000.0)
                 FROM unnest($1::bytea[], $2::bigint[], $3::smallint[], $4::bool[], $5::bool[], $6::bigint[])
                   AS x(p, s, k, t, i, f)
                 ON CONFLICT (pid) DO UPDATE SET
                   invited = metrics_accounts.invited OR excluded.invited,
                   first_message_at = LEAST(metrics_accounts.first_message_at, excluded.first_message_at)",
            )
            .bind(&a_pid)
            .bind(&a_signup)
            .bind(&a_kind)
            .bind(&a_test)
            .bind(&a_inv)
            .bind(&a_first)
            .execute(&mut *tx)
            .await?;
        }

        let (mut day, mut pid, mut kind, mut test, mut cohort) =
            (vec![], vec![], vec![], vec![], vec![]);
        let (mut sent, mut syncs, mut sessions, mut peers) = (vec![], vec![], vec![], vec![]);
        for ((d, id), a) in &b.accounts {
            let Some((k, t, created)) = info.get(id) else {
                continue; // not registered (yet): nothing to count
            };
            day.push(*d);
            pid.push(self.pseudonym(b"acct\0", id));
            kind.push(*k);
            test.push(*t);
            cohort.push((*created >= horizon).then(|| day_of(*created)));
            sent.push(a.sent);
            syncs.push(a.syncs);
            sessions.push(a.sessions);
            peers.push(a.peers as i64);
        }
        if !day.is_empty() {
            sqlx::query(
                "INSERT INTO metrics_activity (day, pid, kind, test, cohort, sent, syncs, sessions, peers)
                 SELECT DATE '1970-01-01' + d, p, k, t, DATE '1970-01-01' + c, s, y, n, b
                 FROM unnest($1::int[], $2::bytea[], $3::smallint[], $4::bool[], $5::int[],
                             $6::int[], $7::int[], $8::int[], $9::bigint[]) AS x(d, p, k, t, c, s, y, n, b)
                 ON CONFLICT (day, pid) DO UPDATE SET
                   sent = metrics_activity.sent + excluded.sent,
                   syncs = metrics_activity.syncs + excluded.syncs,
                   sessions = metrics_activity.sessions + excluded.sessions,
                   peers = metrics_activity.peers | excluded.peers",
            )
            .bind(&day)
            .bind(&pid)
            .bind(&kind)
            .bind(&test)
            .bind(&cohort)
            .bind(&sent)
            .bind(&syncs)
            .bind(&sessions)
            .bind(&peers)
            .execute(&mut *tx)
            .await?;
        }

        if !sources.is_empty() {
            let (mut p, mut k, mut c) = (vec![], vec![], vec![]);
            for (id, (kind, campaign)) in sources {
                p.push(self.pseudonym(b"acct\0", id));
                k.push(kind.clone());
                c.push(campaign.clone());
            }
            sqlx::query(
                "UPDATE metrics_accounts a SET source = x.k, campaign = x.c
                 FROM unnest($1::bytea[], $2::text[], $3::text[]) AS x(p, k, c)
                 WHERE a.pid = x.p AND a.source IS NULL",
            )
            .bind(&p)
            .bind(&k)
            .bind(&c)
            .execute(&mut *tx)
            .await?;
        }

        if !exposures.is_empty() {
            let (mut p, mut f, mut v, mut d) = (vec![], vec![], vec![], vec![]);
            for ((id, flag), (variant, day)) in exposures {
                p.push(self.pseudonym(b"acct\0", id));
                f.push(flag.clone());
                v.push(variant.clone());
                d.push(*day);
            }
            sqlx::query(
                "INSERT INTO metrics_exposures (pid, flag, variant, first_day)
                 SELECT p, f, v, DATE '1970-01-01' + d
                 FROM unnest($1::bytea[], $2::text[], $3::text[], $4::int[]) AS x(p, f, v, d)
                 ON CONFLICT (pid, flag) DO NOTHING",
            )
            .bind(&p)
            .bind(&f)
            .bind(&v)
            .bind(&d)
            .execute(&mut *tx)
            .await?;
        }

        if !b.spaces.is_empty() {
            let (mut d, mut s, mut m, mut n) = (vec![], vec![], vec![], vec![]);
            for ((day, space), a) in &b.spaces {
                d.push(*day);
                s.push(self.pseudonym(b"space\0", space));
                m.push(a.members);
                n.push(a.messages);
            }
            sqlx::query(
                "INSERT INTO metrics_spaces (day, sid, members, messages)
                 SELECT DATE '1970-01-01' + d, s, m, n
                 FROM unnest($1::int[], $2::bytea[], $3::int[], $4::int[]) AS x(d, s, m, n)
                 ON CONFLICT (day, sid) DO UPDATE SET
                   members = GREATEST(metrics_spaces.members, excluded.members),
                   messages = metrics_spaces.messages + excluded.messages",
            )
            .bind(&d)
            .bind(&s)
            .bind(&m)
            .bind(&n)
            .execute(&mut *tx)
            .await?;
        }

        if !b.counters.is_empty() {
            let (mut d, mut name, mut v) = (vec![], vec![], vec![]);
            for ((day, k), n) in &b.counters {
                d.push(*day);
                name.push(k.clone());
                v.push(*n);
            }
            sqlx::query(
                "INSERT INTO metrics_counters (day, name, value)
                 SELECT DATE '1970-01-01' + d, n, v
                 FROM unnest($1::int[], $2::text[], $3::bigint[]) AS x(d, n, v)
                 ON CONFLICT (day, name) DO UPDATE SET value = metrics_counters.value + excluded.value",
            )
            .bind(&d)
            .bind(&name)
            .bind(&v)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

use super::*;

impl Cell {
    pub(super) fn space_key<T: foundationdb::tuple::TuplePack>(
        &self,
        space: &str,
        rest: T,
    ) -> Vec<u8> {
        self.root.subspace(&("s", space)).pack(&rest)
    }

    pub(super) fn space_range(&self, space: &str, part: &str) -> Subspace {
        self.root.subspace(&("s", space, part))
    }

    pub(super) async fn all(
        trx: &Transaction,
        range: &Subspace,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, FdbBindingError> {
        Self::scan(trx, range.range(), usize::MAX, false).await
    }

    /// Up to `limit` pairs of `[begin, end)`, following FoundationDB's partial batches.
    /// `snapshot` reads add no conflict range: for reads no decision depends on.
    pub(super) async fn scan(
        trx: &Transaction,
        (begin, end): (Vec<u8>, Vec<u8>),
        limit: usize,
        snapshot: bool,
    ) -> Result<Vec<(Vec<u8>, Vec<u8>)>, FdbBindingError> {
        let mut out = Vec::new();
        let mut opt = RangeOption::from((begin, end));
        opt.mode = StreamingMode::WantAll;
        let mut iteration = 1;
        while out.len() < limit {
            opt.limit = (limit != usize::MAX).then(|| limit - out.len());
            let page: FdbValues = trx.get_range(&opt, iteration, snapshot).await?;
            let more = page.more();
            for kv in page.iter() {
                out.push((kv.key().to_vec(), kv.value().to_vec()));
            }
            match (more, out.last()) {
                (true, Some((last, _))) => {
                    opt.begin = foundationdb::KeySelector::first_greater_than(last.clone())
                }
                _ => break,
            }
            iteration += 1;
        }
        Ok(out)
    }

    pub(super) async fn role_in(
        &self,
        trx: &Transaction,
        space: &str,
        who: &str,
    ) -> Result<Option<Role>, FdbBindingError> {
        Ok(trx
            .get(&self.space_key(space, ("m", who)), false)
            .await?
            .and_then(|v| unpack::<String>(&v).ok())
            .and_then(|s| parse(&s)))
    }

    pub(super) async fn members_in(
        &self,
        trx: &Transaction,
        space: &str,
        snapshot: bool,
    ) -> Result<Vec<(String, Role)>, FdbBindingError> {
        let range = self.space_range(space, "m");
        Ok(Self::scan(trx, range.range(), usize::MAX, snapshot)
            .await?
            .into_iter()
            .filter_map(|(k, v)| {
                let (who,): (String,) = range.unpack(&k).ok()?;
                Some((who, parse(&unpack::<String>(&v).ok()?)?))
            })
            .collect())
    }

    pub(super) async fn entry(
        &self,
        trx: &Transaction,
        space: &str,
        seq: u64,
        snapshot: bool,
    ) -> Result<Option<Sequenced>, FdbBindingError> {
        match trx
            .get(&self.space_key(space, ("log", seq as i64)), snapshot)
            .await?
        {
            Some(v) => Ok(Some(
                Sequenced::decode(&v).map_err(|e| custom(e.to_string()))?,
            )),
            None => Ok(None),
        }
    }

    /// The Space as of `head`, read with snapshot reads: the caller's head read is the fence.
    pub(super) async fn load(
        &self,
        trx: &Transaction,
        space: &str,
        head: Option<(u64, String)>,
    ) -> Result<SpaceState, FdbBindingError> {
        let mut state = SpaceState::default();
        let Some((seq, hash)) = head else {
            return Ok(state);
        };
        if let Some(meta) = trx.get(&self.space_key(space, "meta"), true).await? {
            let (kind, privacy, creator): (String, String, String) =
                unpack(&meta).map_err(|e| custom(e.to_string()))?;
            state.kind = parse(&kind);
            state.privacy = parse(&privacy);
            state.creator = creator;
        }
        if let Some(v) = trx.get(&self.space_key(space, "mls"), true).await? {
            let (next, device): (i64, String) = unpack(&v).map_err(|e| custom(e.to_string()))?;
            state.mls = Some((next as u64, device));
        }
        state.members = self
            .members_in(trx, space, true)
            .await?
            .into_iter()
            .collect();
        let holds = self.space_range(space, "ck");
        for (k, v) in Self::scan(trx, holds.range(), usize::MAX, true).await? {
            let (who, device): (String, String) =
                holds.unpack(&k).map_err(|e| custom(e.to_string()))?;
            let (at, when) = match unpack::<(i64, i64)>(&v) {
                Ok(hold) => hold,
                Err(_) => {
                    let at: i64 = unpack(&v).map_err(|e| custom(e.to_string()))?;
                    // Older relays did not record a time. Start their ceiling now,
                    // under the head fence, rather than expiring unread history.
                    let hold = (at, now_ms());
                    trx.set(&k, &pack(&hold));
                    hold
                }
            };
            state.holds.insert((who, device), (at as u64, when));
        }
        if let Some(v) = trx.get(&self.space_key(space, "pruned"), true).await? {
            let at: i64 = unpack(&v).map_err(|e| custom(e.to_string()))?;
            state.pruned = at as u64;
        }
        state.advance(seq, &hash);
        Ok(state)
    }

    /// Prunes sealed entries below the floor every current member device has reached, a
    /// batch at a time. Stubs keep the header and the chain hash, so `seen` and checkpoint
    /// checks and dedupe answers still work, and a later joiner can still link the chain.
    pub(super) async fn prune(
        &self,
        trx: &Transaction,
        space: &str,
        state: &mut SpaceState,
    ) -> Result<(), FdbBindingError> {
        let Some(floor) = state.prune_floor(now_ms(), prune_ceiling_ms()) else {
            return Ok(());
        };
        if floor <= state.pruned {
            return Ok(());
        }
        let begin = self.space_key(space, ("log", state.pruned as i64));
        let end = self.space_key(space, ("log", floor as i64));
        let rows = Self::scan(trx, (begin, end), PRUNE_BATCH, true).await?;
        let last = rows
            .last()
            .map(|(key, _)| {
                let (seq,): (i64,) = self
                    .space_range(space, "log")
                    .unpack(key)
                    .map_err(|error| custom(error.to_string()))?;
                Ok::<u64, FdbBindingError>(seq as u64)
            })
            .transpose()?;
        let next =
            mls_projection::pruned_cursor(state.pruned, floor, rows.len(), last).map_err(custom)?;
        for (key, bytes) in rows {
            if let Some(stub) = mls_projection::pruned_bytes(&bytes).map_err(custom)? {
                trx.set(&key, &stub);
            }
        }
        trx.set(&self.space_key(space, "pruned"), &pack(&(next as i64)));
        state.pruned = next;
        Ok(())
    }

    pub(super) fn put_hold(
        &self,
        trx: &Transaction,
        space: &str,
        state: &mut SpaceState,
        who: &str,
        device: &str,
        hold: (u64, i64),
    ) {
        let (at, when) = hold;
        trx.set(
            &self.space_key(space, ("ck", who, device)),
            &pack(&(at as i64, when)),
        );
        state
            .holds
            .insert((who.to_string(), device.to_string()), (at, when));
    }

    pub(super) fn drop_holds(
        &self,
        trx: &Transaction,
        space: &str,
        state: &mut SpaceState,
        who: &str,
    ) {
        let range = self.root.subspace(&("s", space, "ck", who));
        trx.clear_subspace_range(&range);
        state.holds.retain(|(w, _), _| w != who);
    }

    pub(super) fn put_mls(
        &self,
        trx: &Transaction,
        space: &str,
        state: &mut SpaceState,
        next: u64,
        device: String,
    ) {
        trx.set(
            &self.space_key(space, "mls"),
            &pack(&(next as i64, device.as_str())),
        );
        state.mls = Some((next, device));
    }

    pub(super) fn put_member(&self, trx: &Transaction, space: &str, who: &str, role: Role) {
        trx.set(&self.space_key(space, ("m", who)), &pack(&word(&role)));
        trx.set(&self.root.pack(&("i", who, space)), b"");
    }
}

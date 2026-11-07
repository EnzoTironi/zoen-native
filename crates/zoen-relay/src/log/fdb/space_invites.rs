//! Invitation controls share the redemption key and membership conflict ranges.
//! `siv` is a metadata index; the `inv` record remains the sole use/expiry authority.
use super::*;
use protocol::space_invites::{Action, Request, SpaceInvitePage, SpaceInviteRecord};

impl FdbLog {
    #[allow(clippy::too_many_arguments)]
    pub async fn create_invite_fenced(
        &self,
        who: &str,
        space: &str,
        role: Role,
        max_uses: u32,
        ttl_secs: u64,
        code: &str,
        fence: Fence,
    ) -> Result<InviteCreated, String> {
        let (max, ttl_ms) = relay_admission::invite::invite_limits(max_uses, ttl_secs);
        let expires = now_ms() + ttl_ms;
        let key = self.cell.root.pack(&("inv", code_hash(code)));
        let fence = &fence;
        let r = self
            .cell
            .db
            .run(|trx, _| {
                let key = key.clone();
                async move {
                    ownership::bound_transaction(&trx)?;
                    let version = trx.get_read_version().await?;
                    let lease = ownership::lease_in(&trx, &self.cell.root, fence.partition).await?;
                    if fence.owner != self.owner.node
                        || !ownership::accepts(lease.as_ref(), fence, space, version)
                    {
                        return Ok(Err("stale partition owner"));
                    }
                    if !protocol::valid_invite_code(code) {
                        return Ok(Err("invalid invite code"));
                    }
                    let mine = self.cell.role_in(&trx, space, who).await?;
                    if let Err(reason) = relay_admission::invite::authorize_inviter(mine, role) {
                        return Ok(Err(reason));
                    }
                    let kind = match trx.get(&self.cell.space_key(space, "meta"), false).await? {
                        Some(m) => {
                            unpack::<(String, String, String)>(&m)
                                .map_err(|e| custom(e.to_string()))?
                                .0
                        }
                        None => return Ok(Err("unknown space")),
                    };
                    if let Some(kind) = parse::<SpaceKind>(&kind) {
                        if let Err(reason) = relay_admission::invite::authorize_invite_kind(kind) {
                            return Ok(Err(reason));
                        }
                    }
                    if let Some(existing) = trx.get(&key, false).await? {
                        let inv: Invite = unpack(&existing).map_err(|e| custom(e.to_string()))?;
                        if inv.0 != space || inv.1 != word(&role) || inv.2 != who || inv.4 != max {
                            return Ok(Err("invite code collision"));
                        }
                    } else {
                        trx.set(&key, &pack(&(space, word(&role), who, expires, max, 0i64)));
                        trx.set(
                            &self.cell.root.pack(&("siv", space, code_hash(code))),
                            &pack(&""),
                        );
                    }
                    let stored = trx
                        .get(&key, true)
                        .await?
                        .ok_or_else(|| custom("invite missing after mint"))?;
                    let inv: Invite = unpack(&stored).map_err(|e| custom(e.to_string()))?;
                    Ok(Ok(inv.3))
                }
            })
            .await
            .map_err(|e| e.to_string())?;
        let expires = r.map_err(str::to_string)?;
        Ok(InviteCreated {
            code: code.to_string(),
            expires_at_ms: expires,
        })
    }

    pub(super) async fn manage_space_invites(
        &self,
        who: &str,
        request: &Request,
    ) -> Result<SpaceInvitePage, String> {
        request.validate().map_err(str::to_string)?;
        self.cell
            .db
            .run(|trx, _| async move {
                ownership::bound_transaction(&trx)?;
                let mine = self.cell.role_in(&trx, &request.space, who).await?;
                if relay_admission::invite::authorize_inviter(mine, Role::Member).is_err() {
                    return Ok(Err(
                        "invite administration requires current admin membership",
                    ));
                }
                let Some(meta) = trx
                    .get(&self.cell.space_key(&request.space, "meta"), false)
                    .await?
                else {
                    return Ok(Err("unknown space"));
                };
                let (kind, _, _): (String, String, String) =
                    unpack(&meta).map_err(|e| custom(e.to_string()))?;
                if parse::<SpaceKind>(&kind).is_none_or(|kind| kind == SpaceKind::Direct) {
                    return Ok(Err("direct chats don't take invites"));
                }
                let root = &self.cell.root;
                let range = root.subspace(&("siv", &request.space));
                let page = match &request.action {
                    Some(Action::List(after)) => {
                        if !backfill(&trx, root).await? {
                            return Ok(Ok(SpaceInvitePage {
                                indexing: true,
                                ..Default::default()
                            }));
                        }
                        let (mut begin, end) = range.range();
                        if !after.is_empty() {
                            begin = range.pack(&(after,));
                            begin.push(0);
                        }
                        let rows = Cell::scan(&trx, (begin, end), 51, false).await?;
                        let more = rows.len() > 50;
                        let mut entries = Vec::new();
                        for (key, label) in rows.into_iter().take(50) {
                            let (id,): (String,) =
                                range.unpack(&key).map_err(|e| custom(e.to_string()))?;
                            let name: String = unpack(&label).map_err(|e| custom(e.to_string()))?;
                            if let Some(invite) = trx.get(&root.pack(&("inv", &id)), false).await? {
                                let inv: Invite =
                                    unpack(&invite).map_err(|e| custom(e.to_string()))?;
                                if inv.0 != request.space {
                                    return Err(custom("invite index mismatch"));
                                }
                                entries.push(record(id, name, inv)?);
                            } else {
                                return Err(custom("invite index missing authority"));
                            }
                        }
                        let next = if more {
                            entries.last().map(|e| e.id.clone()).unwrap_or_default()
                        } else {
                            String::new()
                        };
                        SpaceInvitePage {
                            entries,
                            next,
                            indexing: false,
                        }
                    }
                    Some(
                        Action::Revoke(id)
                        | Action::Rename(protocol::space_invites::Rename { id, .. }),
                    ) => {
                        let key = root.pack(&("inv", id));
                        let Some(bytes) = trx.get(&key, false).await? else {
                            return Ok(Err("unknown invite"));
                        };
                        let mut inv: Invite = unpack(&bytes).map_err(|e| custom(e.to_string()))?;
                        if inv.0 != request.space {
                            return Ok(Err("unknown invite"));
                        }
                        // Admins may manage Member/Reader links; only Owner can manage links granting Admin.
                        let Some(role) = parse::<Role>(&inv.1) else {
                            return Err(custom("invalid invite role"));
                        };
                        if let Err(reason) = relay_admission::invite::authorize_inviter(mine, role)
                        {
                            return Ok(Err(reason));
                        }
                        let label_key = range.pack(&(id,));
                        let mut name: String = match trx.get(&label_key, false).await? {
                            Some(value) => unpack(&value).map_err(|e| custom(e.to_string()))?,
                            None => String::new(),
                        };
                        match &request.action {
                            Some(Action::Revoke(_)) => {
                                inv.3 = 0;
                                trx.set(&key, &pack(&inv));
                            }
                            Some(Action::Rename(rename)) => name = rename.name.trim().to_string(),
                            _ => return Err(custom("invalid invite action")),
                        }
                        trx.set(&label_key, &pack(&name));
                        SpaceInvitePage {
                            entries: vec![record(id.clone(), name, inv)?],
                            ..Default::default()
                        }
                    }
                    None => return Ok(Err("missing invite action")),
                };
                Ok(Ok(page))
            })
            .await
            .map_err(|e| e.to_string())?
            .map_err(str::to_string)
    }
}
fn record(id: String, name: String, inv: Invite) -> Result<SpaceInviteRecord, FdbBindingError> {
    Ok(SpaceInviteRecord {
        id,
        name,
        creator: inv.2,
        role: inv.1,
        expires_at_ms: inv.3,
        max_uses: u32::try_from(inv.4).map_err(|e| custom(e.to_string()))?,
        uses: u32::try_from(inv.5).map_err(|e| custom(e.to_string()))?,
        revoked: inv.3 == 0,
    })
}
/// One bounded, resumable global migration. Concurrent mint writes its own index;
/// existing labels are preserved. No incomplete list is presented as complete.
async fn backfill(trx: &Transaction, root: &Subspace) -> Result<bool, FdbBindingError> {
    let checkpoint = root.pack(&("siv_migration_v1",));
    let state = trx.get(&checkpoint, false).await?;
    if state.as_ref().is_some_and(|v| v.as_ref() == b"done") {
        return Ok(true);
    }
    let invites = root.subspace(&("inv",));
    let (mut begin, end) = invites.range();
    if let Some(cursor) = state {
        begin = cursor.to_vec();
        begin.push(0);
    }
    let rows = Cell::scan(trx, (begin, end), 257, false).await?;
    let more = rows.len() > 256;
    let mut last = None;
    for (key, value) in rows.into_iter().take(256) {
        let (id,): (String,) = invites.unpack(&key).map_err(|e| custom(e.to_string()))?;
        let inv: Invite = unpack(&value).map_err(|e| custom(e.to_string()))?;
        let label = root.pack(&("siv", &inv.0, id));
        if trx.get(&label, false).await?.is_none() {
            trx.set(&label, &pack(&""));
        }
        last = Some(key);
    }
    if more {
        let key = last.ok_or_else(|| custom("invite migration cursor missing"))?;
        trx.set(&checkpoint, &key);
    } else {
        trx.set(&checkpoint, b"done");
    }
    Ok(!more)
}

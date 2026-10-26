//! Redemption preflight is actor-aware but read-only. Public invite previews
//! intentionally continue hiding spent links, including from prior redeemers.
use super::*;

impl FdbLog {
    pub(super) async fn agent_invite_redemption_preview(
        &self,
        code: &str,
        who: &str,
    ) -> Result<InviteInfo, String> {
        let root = &self.cell.root;
        let key = root.pack(&("ainv", code_hash(code)));
        let mine = redeemers(root, code).pack(&who);
        self.cell
            .db
            .run(|trx, _| {
                let (key, mine) = (key.clone(), mine.clone());
                async move {
                    let Some(value) = trx.get(&key, false).await? else {
                        return Ok(Err("invite expired or already used"));
                    };
                    let invite: AgentInvite =
                        unpack(&value).map_err(|error| custom(error.to_string()))?;
                    if invite.2 <= now_ms()
                        || (invite.4 >= invite.3 && trx.get(&mine, false).await?.is_none())
                    {
                        return Ok(Err("invite expired or already used"));
                    }
                    // No use is spent here: SQL lifecycle admission must still
                    // succeed before the existing atomic redeem transaction.
                    Ok(Ok(agent_info(invite)))
                }
            })
            .await
            .map_err(|error| error.to_string())?
            .map_err(str::to_string)
    }
}

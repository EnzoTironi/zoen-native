//! Owner-only bootstrap using the existing first-append transaction and log.
use super::*;
use protocol::saved_messages::{Genesis, Receipt};
use types::Role;

impl Session {
    async fn saved_genesis(&self, space: &str) -> Result<Option<protocol::Sequenced>, String> {
        let events = self
            .st
            .log
            .read(space, 0, 1)
            .await
            .map_err(|_| "saved messages unavailable")?;
        let Some(event) = events.into_iter().next() else {
            return Ok(None);
        };
        let members = self
            .st
            .log
            .members(space)
            .await
            .map_err(|_| "saved messages unavailable")?;
        if event.seq != 0
            || event.env.verify().is_err()
            || !event
                .env
                .body()
                .as_ref()
                .is_some_and(|body| types::saved_messages::genesis(space, &self.identity, body))
            || event.env.author() != self.identity
            || members != vec![(self.identity.clone(), Role::Owner)]
        {
            return Err("invalid saved messages history".into());
        }
        Ok(Some(event))
    }

    pub(super) async fn ensure_saved_messages(
        &mut self,
        proposal: Genesis,
    ) -> Result<Reply, String> {
        if !self.saved_messages {
            return Err("saved messages was not negotiated".into());
        }
        let env = proposal
            .envelope()
            .ok_or("invalid saved messages genesis")?;
        if env.author() != self.identity
            || env.device() != Some(self.device.as_str())
            || env.cert.as_deref() != Some(self.cert.as_str())
            || env.verify().is_err()
        {
            return Err("invalid saved messages owner".into());
        }
        let space = env.space().to_owned();
        if let Some(event) = self.saved_genesis(&space).await? {
            return Ok(Reply::SavedMessages(Receipt::from_sequenced(&event)));
        }
        self.publish_allowed().map_err(limits::slow_down)?;
        // FDB's ordinary non-snapshot head read is the CAS. An independent
        // device's concurrent candidate can only lose; no second MLS genesis.
        let outcome = self.append_effect(&env, true).await;
        let original = match outcome {
            Ok(managed::Appended::Sequenced(
                Sequencing::New { ev, .. } | Sequencing::Duplicate { ev, .. },
            ))
            | Ok(managed::Appended::Observed(ev)) => ev,
            Err(reason) => match self.saved_genesis(&space).await? {
                Some(event) => event,
                None => return Err(reason.reason),
            },
        };
        // This reply resolves "ensure the owner's room", never claims that a
        // losing candidate was published. Return the exact original signed bytes.
        let canonical = self
            .saved_genesis(&space)
            .await?
            .ok_or("saved messages not committed")?;
        if canonical != original {
            return Err("saved messages genesis changed".into());
        }
        self.st.fanout.send(
            std::slice::from_ref(&self.identity),
            &ServerFrame::Event {
                ev: canonical.clone(),
            },
            None,
        );
        Ok(Reply::SavedMessages(Receipt::from_sequenced(&canonical)))
    }
}

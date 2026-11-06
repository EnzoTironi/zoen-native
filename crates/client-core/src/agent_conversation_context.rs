//! Native source and bounded context for the existing single-Client agent host.
use crate::{
    engine::{Engine, Entry, EntryBody, R},
    Client, CoreError,
};

const HISTORY: usize = 16;

/// Read-only model input. Construction is private and carries no tool permission.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AgentConversationMessage {
    id: String,
    author: String,
    text: String,
    revision: String,
}

impl AgentConversationMessage {
    pub fn id(&self) -> &str {
        &self.id
    }
    pub fn author(&self) -> &str {
        &self.author
    }
    pub fn revision(&self) -> &str {
        &self.revision
    }
    pub fn text(&self) -> &str {
        &self.text
    }
}

/// Not deserializable or FFI-exported. The same native Client rechecks this
/// observation after asynchronous inference; it is not a financial or IO permit.
pub struct AgentConversationContext {
    actor: String,
    space: String,
    binding: (String, String, String, String),
    source: AgentConversationMessage,
    history: Vec<AgentConversationMessage>,
    scoped: bool,
    thread_root: Option<String>,
}

impl AgentConversationContext {
    pub fn thread_root(&self) -> Option<&str> {
        self.thread_root.as_deref()
    }

    pub fn source(&self) -> &AgentConversationMessage {
        &self.source
    }
    pub fn history(&self) -> &[AgentConversationMessage] {
        &self.history
    }
    pub fn scoped(&self) -> bool {
        self.scoped
    }
}

fn denied() -> CoreError {
    CoreError::Forbidden {
        reason: "The agent conversation changed during this turn.".into(),
    }
}

fn message(entry: &Entry) -> Option<AgentConversationMessage> {
    let EntryBody::Message { text, marks, .. } = &entry.body else {
        return None;
    };
    if marks.deleted || text.trim().is_empty() {
        return None;
    }
    Some(AgentConversationMessage {
        id: entry.id().into(),
        author: entry.author.clone(),
        text: text.clone(),
        revision: marks.text_revision.as_deref().unwrap_or(entry.id()).into(),
    })
}

fn thread_root(entry: &Entry) -> Option<&str> {
    match &entry.body {
        EntryBody::Message {
            reply: Some(reply), ..
        } if reply.thread => Some(&reply.to),
        _ => None,
    }
}

impl Client {
    /// Read the actual source and preceding text under one native lock. Scoped
    /// artifact turns deliberately receive no unrelated conversation history.
    /// # Errors
    /// Refuses uncertified actors, missing/deleted sources and lost membership.
    pub fn agent_conversation_context(
        &self,
        space: &str,
        source: &str,
        serving: &str,
    ) -> R<AgentConversationContext> {
        let e = self.lock();
        let binding = e.agent_source_binding(space, source, serving, false)?;
        let scoped = e.artifact_request(space, source)?.is_some();
        let entries = &e.space_state(space)?.entries;
        let index = entries
            .iter()
            .position(|entry| entry.id() == source)
            .ok_or_else(denied)?;
        let source = message(&entries[index]).ok_or_else(denied)?;
        let thread_root = thread_root(&entries[index]).map(str::to_owned);
        let history = if scoped {
            Vec::new()
        } else {
            let mut history: Vec<_> = entries[..index]
                .iter()
                .rev()
                .filter(|entry| e.visible_message(entry))
                .filter(|entry| match thread_root.as_deref() {
                    Some(root) => entry.id() == root || self::thread_root(entry) == Some(root),
                    None => self::thread_root(entry).is_none(),
                })
                .filter_map(message)
                .take(HISTORY)
                .collect();
            history.reverse();
            history
        };
        Ok(AgentConversationContext {
            actor: e.me_id()?,
            space: space.into(),
            binding,
            source,
            history,
            scoped,
            thread_root,
        })
    }

    /// Recheck the same actor, source revision and membership. History is the
    /// admitted snapshot; later messages or history edits do not restart inference.
    /// A source edit invalidates the turn even if it restores the same text.
    /// This grants no tool authority.
    /// # Errors
    /// Refuses changes to any of the original native observations.
    pub fn recheck_agent_conversation_context(&self, context: &AgentConversationContext) -> R<()> {
        self.lock().recheck_agent_conversation_context(context)
    }

    /// Publish a plain reply under the same lock as the current source check.
    /// # Errors
    /// Refuses stale context or normal native reply/permission failures.
    pub fn agent_conversation_reply(
        &self,
        context: &AgentConversationContext,
        text: &str,
    ) -> R<crate::TimelineEntry> {
        let mut e = self.lock();
        e.recheck_agent_conversation_context(context)?;
        // Scoped artifacts must retain their existing source/head-specific port.
        if context.scoped {
            return Err(denied());
        }
        if let Some(prior) = e.space_state(&context.space)?.entries.iter().find(|entry| {
            entry.author == context.actor
                && matches!(&entry.body, EntryBody::Message { reply: Some(reply), .. }
                    if reply.source_id() == context.source.id())
        }) {
            let id = prior.id().to_owned();
            if !matches!(&prior.body, EntryBody::Message { text: prior_text, .. } if prior_text == text)
            {
                return Err(denied());
            }
            return e
                .timeline(&context.space)?
                .into_iter()
                .find(|entry| entry.id == id)
                .ok_or_else(denied);
        }
        e.send_reply(
            &context.space,
            text,
            context.source.id(),
            context.thread_root.is_some(),
        )
    }
}

impl Engine {
    fn recheck_agent_conversation_context(&self, context: &AgentConversationContext) -> R<()> {
        let e = self;
        if e.me_id()? != context.actor
            || e.agent_source_binding(
                &context.space,
                context.source.id(),
                context.source.author(),
                false,
            )? != context.binding
            || e.artifact_request(&context.space, context.source.id())?
                .is_some()
                != context.scoped
        {
            return Err(denied());
        }
        let entries = &e.space_state(&context.space)?.entries;
        if entries
            .iter()
            .find(|entry| entry.id() == context.source.id())
            .and_then(message)
            .as_ref()
            != Some(&context.source)
        {
            return Err(denied());
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

//! Explicit screenshot-test fixture. Deterministic keys are confined to demo seeding.
use std::cell::Cell;
thread_local! { static COUNTER: Cell<Option<u64>> = const { Cell::new(None) }; }
pub(crate) const FIXED_MS: i64 = 1_768_478_400_000;
pub(crate) fn enabled() -> bool {
    std::env::var("ZOEN_PARITY_FIXTURE").as_deref() == Ok("native-parity-v1")
}
pub(crate) fn fixed_ms() -> Option<i64> {
    enabled().then_some(FIXED_MS)
}
pub(crate) struct SeedGuard;
impl SeedGuard {
    pub(crate) fn enter() -> Self {
        COUNTER.with(|c| c.set(enabled().then_some(128)));
        Self
    }
}
impl Drop for SeedGuard {
    fn drop(&mut self) {
        COUNTER.with(|c| c.set(None));
    }
}
fn next() -> Option<u64> {
    COUNTER.with(|c| {
        c.get().map(|n| {
            c.set(Some(n + 1));
            n
        })
    })
}
pub(crate) fn id(prefix: &str) -> String {
    next()
        .map(|n| {
            let mut value = ((FIXED_MS as u128) << 80) | n as u128;
            let alphabet = b"0123456789abcdefghjkmnpqrstvwxyz";
            let mut encoded = [b'0'; 26];
            for c in encoded.iter_mut().rev() {
                *c = alphabet[(value & 31) as usize];
                value >>= 5;
            }
            format!(
                "{prefix}_{}",
                String::from_utf8(encoded.to_vec()).expect("ASCII ULID")
            )
        })
        .unwrap_or_else(|| roda_types::new_id(prefix))
}
pub(crate) fn client_id(at_ms: i64) -> Option<String> {
    next().map(|n| {
        let mut value = ((at_ms.max(0) as u128) << 80) | n as u128;
        let alphabet = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
        let mut encoded = [b'0'; 26];
        for c in encoded.iter_mut().rev() {
            *c = alphabet[(value & 31) as usize];
            value >>= 5;
        }
        String::from_utf8(encoded.to_vec()).expect("ASCII ULID")
    })
}
pub(crate) fn signer() -> Option<roda_log::Signer> {
    next().map(|n| {
        let mut secret = [128u8; 32];
        secret[..8].copy_from_slice(&n.to_le_bytes());
        roda_log::Signer::from_secret(&secret)
    })
}
#[cfg(test)]
mod tests {
    use serde_json::{json, Value};
    fn persona(p: &crate::dto::Persona) -> Value {
        json!({"id":p.id,"name":p.name,"handle":p.handle,"initials":p.initials,
            "tintHex":p.tint_hex,"glyph":p.glyph,"bio":p.bio,"isMe":p.is_me,
            "kind":format!("{:?}",p.kind).to_lowercase()})
    }
    /// Regenerate the web's read-only native fixture directly from the native seed.
    /// Run alone with ZOEN_PARITY_FIXTURE=native-parity-v1 and ZOEN_PARITY_EXPORT=<path>.
    #[test]
    #[ignore = "explicit fixture export; requires output path and fixture environment"]
    fn export_native_fixture() {
        assert!(super::enabled());
        let core = crate::RodaEngine::open(":memory:".into(), "en_US".into()).unwrap();
        core.seed_demo_if_empty().unwrap();
        let spaces: Vec<Value> = core.spaces().iter().map(|s| {
            let entries: Vec<Value> = core.timeline(s.id.clone()).unwrap().iter().map(|e| {
                let (kind, text, card) = match &e.kind {
                    crate::dto::EntryKind::Message { text, card } => ("message", text.clone(), card.as_ref().map(|c| json!({
                        "itemId":c.item_id,"title":c.title,"summary":c.summary,"kindLabel":c.kind_label,
                        "kindId":c.kind_id,"version":c.version,"totalCents":c.total_cents,
                        "budgetCents":c.budget_cents,"lineCount":c.line_count,"doneCount":c.done_count}))),
                    crate::dto::EntryKind::System {text} => ("system",text.clone(),None),
                    crate::dto::EntryKind::ItemEdited {note,..} => ("itemEdited",note.clone(),None),
                    crate::dto::EntryKind::Request {title,..} => ("request",title.clone(),None),
                    crate::dto::EntryKind::Background {..} => ("background",String::new(),None),
                };
                json!({"id":e.id,"seq":e.seq,"author":persona(&e.author),"atMs":e.at_ms,
                    "kind":kind,"text":text,"card":card,"delivery":format!("{:?}",e.delivery).to_lowercase(),
                    "threadReplies":e.thread_replies,"edited":e.edited,"deleted":e.deleted})
            }).collect();
            json!({"id":s.id,"title":s.title,"kind":format!("{:?}",s.kind).to_lowercase(),
                "members":s.members.iter().map(persona).collect::<Vec<_>>(),
                "counterpart":s.counterpart.as_ref().map(persona),"lastPreview":s.last_preview,
                "lastAuthor":s.last_author.as_ref().map(persona),"lastAtMs":s.last_at_ms,
                "unread":s.unread,"pinned":s.pinned,"muted":s.muted,"entries":entries})
        }).collect();
        let fixture = json!({"fixture_id":"native-parity-v1","fixed_time":"2026-01-15T12:00:00Z",
            "locale":"en_US","timezone":"UTC","seed":128,"me":persona(&core.me().unwrap()),
            "personas":core.personas().iter().map(persona).collect::<Vec<_>>(),"spaces":spaces});
        std::fs::write(
            std::env::var("ZOEN_PARITY_EXPORT").unwrap(),
            serde_json::to_string_pretty(&fixture).unwrap(),
        )
        .unwrap();
    }
    use super::*;
    #[test]
    fn scoped_sequence_repeats_and_exits() {
        // Test the scope without mutating process environment used by other tests.
        COUNTER.with(|c| c.set(Some(128)));
        let a = id("sp");
        let key = signer().unwrap().id();
        COUNTER.with(|c| c.set(Some(128)));
        assert_eq!(a, id("sp"));
        assert_eq!(key, signer().unwrap().id());
        drop(SeedGuard);
        assert!(signer().is_none());
        assert_eq!(a.len(), 29); // prefix, underscore, canonical 26-character ULID
    }
}

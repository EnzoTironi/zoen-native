//! Explicit screenshot-test fixture. Deterministic keys are confined to demo seeding.
use std::cell::Cell;
thread_local! {
    static COUNTER: Cell<Option<u64>> = const { Cell::new(None) };
    static REQUEST_COUNTER: Cell<Option<u64>> = const { Cell::new(None) };
}
// Separate namespace preserves every existing fixture ID and signed card snapshot.
pub(crate) fn request_id() -> String {
    REQUEST_COUNTER.with(|c| match c.get() {
        Some(n) => {
            c.set(Some(n + 1));
            let mut value = ((FIXED_MS as u128) << 80) | n as u128;
            let alphabet = b"0123456789abcdefghjkmnpqrstvwxyz";
            let mut encoded = [b'0'; 26];
            for digit in encoded.iter_mut().rev() {
                *digit = alphabet[(value & 31) as usize];
                value >>= 5;
            }
            format!(
                "rq_{}",
                String::from_utf8(encoded.to_vec()).expect("ASCII ULID")
            )
        }
        None => roda_types::new_id("rq"),
    })
}
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
        REQUEST_COUNTER.with(|c| c.set(enabled().then_some(128)));
        Self
    }
}
impl Drop for SeedGuard {
    fn drop(&mut self) {
        COUNTER.with(|c| c.set(None));
        REQUEST_COUNTER.with(|c| c.set(None));
    }
}
fn next() -> Option<u64> {
    COUNTER.with(|c| {
        c.get().inspect(|n| {
            c.set(Some(n + 1));
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
            "isMine":p.is_mine,"ownerId":p.owner_id,"ownerName":p.owner_name,"ownerTintHex":p.owner_tint_hex,
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
        let independently_seeded =
            crate::RodaEngine::open(":memory:".into(), "en_US".into()).unwrap();
        independently_seeded.seed_demo_if_empty().unwrap();
        assert!(!core.requests().is_empty());
        assert_eq!(
            core.requests(),
            independently_seeded.requests(),
            "full genuine request DTOs must repeat across independent seeds"
        );
        let spaces: Vec<Value> = core.spaces().iter().map(|s| {
            let entries: Vec<Value> = core.timeline(s.id.clone()).unwrap().iter().map(|e| {
                let (kind, text, card) = match &e.kind {
                    crate::dto::EntryKind::Message { text, card } => ("message", text.clone(), card.as_ref().map(|c| json!({
                        "itemId":c.item_id,"title":c.title,"summary":c.summary,"kindLabel":c.kind_label,
                        "kindId":c.kind_id,"version":c.version,"totalCents":c.total_cents,
                        "budgetCents":c.budget_cents,"lineCount":c.line_count,"doneCount":c.done_count,
                        "snapshot":core.item(c.item_id.clone()).ok().and_then(|item| item.app)
                            .and_then(|app| serde_json::from_str::<Value>(&app.snapshot_json).ok())}))),
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
        // Fresh fixture has empty persisted homePins/homeUnpinned. Match AppModel.homeApps:
        // live (not released) apps, oldest first, retaining only native widget snapshots.
        let home_apps: Vec<Value> = core
            .items()
            .into_iter()
            .rev()
            .filter_map(|item| {
                let app = item.app?;
                let view: Value = serde_json::from_str(&app.view_json).unwrap();
                if view
                    .get("released")
                    .and_then(Value::as_bool)
                    .unwrap_or(false)
                {
                    return None;
                }
                if app.snapshot_json.is_empty() {
                    return None;
                }
                let snapshot: Value =
                    serde_json::from_str(&app.snapshot_json).expect("native snapshot JSON");
                assert_eq!(
                    snapshot["deepLink"],
                    format!("zoen://app/{}", snapshot["id"].as_str().unwrap())
                );
                assert!(snapshot["title"].as_str().is_some_and(|s| !s.is_empty()));
                assert!([
                    "stat",
                    "progress",
                    "countdown",
                    "list",
                    "caption",
                    "ticket",
                    "photo"
                ]
                .contains(&snapshot["template"].as_str().unwrap()));
                Some(snapshot)
            })
            .collect();
        assert!(
            !home_apps.is_empty(),
            "native seed must expose actual Home widgets"
        );
        let requests: Vec<Value> = core.requests().iter().map(|r| json!({
            "id":r.id,"agent":persona(&r.agent),"spaceId":r.space_id,"spaceTitle":r.space_title,
            "title":r.title,"detail":r.detail,"audience":r.audience,"actionLabel":r.action_label,
            "costCents":r.cost_cents,"reason":r.reason,"status":format!("{:?}",r.status).to_lowercase(),
            "openedMs":r.opened_ms,"resolvedMs":r.resolved_ms,"itemId":r.item_id,"lineId":r.line_id,
            "actionKey":r.action_key,"canAlwaysApprove":r.can_always_approve,"byStanding":r.by_standing
        })).collect();
        let agents: Vec<Value> = core.agents().iter().map(|a| json!({
            "persona":persona(&a.persona),"budgetLimitCents":a.budget_limit_cents,
            "budgetSpentCents":a.budget_spent_cents,"nearLimit":a.near_limit,
            "spaces":a.spaces.iter().map(|s| json!({"spaceId":s.space_id,"spaceTitle":s.space_title,
                "level":format!("{:?}",s.level).to_lowercase()})).collect::<Vec<_>>(),
            "pendingRequests":a.pending_requests,"approvalsStreak":a.approvals_streak,
            "runsOn":a.runs_on,"ownerNotesPending":a.owner_notes_pending
        })).collect();
        let mentions: Vec<Value> = core
            .mentions()
            .iter()
            .map(|m| {
                // Resolve the exact projected entry already exported above: no second renderer or fake message.
                let entry = spaces.iter().find(|s| s["id"] == m.space_id).unwrap()["entries"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|e| e["id"] == m.entry.id)
                    .unwrap()
                    .clone();
                json!({"spaceId":m.space_id,"spaceTitle":m.space_title,"entry":entry})
            })
            .collect();
        // Full public item UI projection, independent of timeline card summaries.
        // Never export storage paths, signing material or file payloads.
        let native_items = core.items();
        let items: Vec<Value> = native_items.iter().map(|i| json!({
            "id":i.id,"spaceId":i.space_id,"spaceTitle":i.space_title,
            "kindLabel":i.kind_label,"kindId":i.kind_id,"title":i.title,"origin":i.origin,
            "createdBy":persona(&i.created_by),"text":i.text,"version":i.version,"path":i.path,
            "plan":i.plan.as_ref().map(|p| json!({"title":p.title,"summary":p.summary,
                "budgetCents":p.budget_cents,"totalCents":p.total_cents,
                "sections":p.sections.iter().map(|s| json!({"title":s.title,
                    "lines":s.lines.iter().map(|l| json!({"id":l.id,"text":l.text,
                        "costCents":l.cost_cents,"done":l.done})).collect::<Vec<_>>()
                })).collect::<Vec<_>>() })),
            "versions":i.versions.iter().map(|v| json!({"number":v.number,"author":persona(&v.author),
                "atMs":v.at_ms,"note":v.note,"isUndo":v.is_undo,"totalCents":v.total_cents})).collect::<Vec<_>>(),
            "linkedRequests":i.linked_requests.iter().map(|r| requests.iter().find(|v| v["id"] == r.id)
                .expect("linked request is in public requests").clone()).collect::<Vec<_>>(),
            "app":i.app.as_ref().map(|a| json!({"appId":a.app_id,"resourceUri":a.resource_uri,
                "name":a.name,"viewJson":a.view_json,"headline":a.headline,
                "metrics":a.metrics.iter().map(|m| json!({"label":m.label,"value":m.value})).collect::<Vec<_>>(),
                "lastAction":a.last_action,"trust":format!("{:?}",a.trust).to_lowercase(),"snapshotJson":a.snapshot_json})),
            "file":i.file.as_ref().map(|f| json!({"name":f.name,"path":f.path,"mime":f.mime,
                "bytes":f.bytes,"sha256":f.sha256,"ready":f.ready,"chunks":f.chunks,
                "chunksHere":f.chunks_here,"hasThumbnail":f.has_thumbnail}))
        })).collect();
        let actual_tasks: Vec<_> = native_items
            .iter()
            .filter(|i| {
                i.kind_id == "task"
                    || i.plan
                        .as_ref()
                        .is_some_and(|p| p.sections.iter().any(|s| s.lines.iter().any(|l| !l.done)))
            })
            .map(|i| i.id.as_str())
            .collect();
        let exported_tasks: Vec<_> = items
            .iter()
            .filter(|i| {
                i["kindId"] == "task"
                    || i["plan"]["sections"].as_array().is_some_and(|ss| {
                        ss.iter().any(|s| {
                            s["lines"]
                                .as_array()
                                .unwrap()
                                .iter()
                                .any(|l| l["done"] == false)
                        })
                    })
            })
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(actual_tasks, exported_tasks);
        let actual_owned: Vec<_> = core
            .agents()
            .into_iter()
            .filter(|a| a.persona.is_mine)
            .map(|a| a.persona.id)
            .collect();
        let exported_owned: Vec<_> = agents
            .iter()
            .filter(|a| a["persona"]["isMine"] == true)
            .map(|a| a["persona"]["id"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(actual_owned, exported_owned);
        let stats = core.stats();
        let grants: Vec<Value> = core.app_device_grants().iter().map(|g| json!({
            "grantId":g.grant_id,"itemId":g.item_id,"itemTitle":g.item_title,"appName":g.app_name,
            "spaceTitle":g.space_title,"capability":g.capability,"purpose":g.purpose,
            "always":g.always,"atMs":g.at_ms
        })).collect();
        let account = core.account().map(|a| {
            json!({"identityId":a.identity_id,"deviceId":a.device_id,
            "name":a.name,"handle":a.handle,"registered":a.registered,"unlocked":a.unlocked})
        });
        let connection = core.connection();
        let fixture = json!({"fixture_id":"native-parity-v1","fixed_time":"2026-01-15T12:00:00Z",
            "locale":"en_US","timezone":"UTC","seed":128,"me":persona(&core.me().unwrap()),
            "personas":core.personas().iter().map(persona).collect::<Vec<_>>(),"spaces":spaces,
            "homeApps":home_apps,"requests":requests,"agents":agents,"mentions":mentions,"items":items,
            "stats":{"spaces":stats.spaces,"events":stats.events,"items":stats.items,
                "identities":stats.identities,"allLogsValid":stats.all_logs_valid},
            "appDeviceGrants":grants,"account":account,
            "connection":{"state":connection.state,"synced":connection.synced,
                "pending":connection.pending,"error":connection.error}});
        std::fs::write(
            std::env::var("ZOEN_PARITY_EXPORT").unwrap(),
            serde_json::to_string_pretty(&fixture).unwrap(),
        )
        .unwrap();
    }
    use super::*;
    #[test]
    fn request_ids_are_random_outside_seed_scope() {
        REQUEST_COUNTER.with(|c| c.set(None));
        assert_ne!(request_id(), request_id());
        REQUEST_COUNTER.with(|c| c.set(Some(128)));
        let first = request_id();
        REQUEST_COUNTER.with(|c| c.set(Some(128)));
        assert_eq!(first, request_id());
        drop(SeedGuard);
        assert_ne!(first, request_id());
    }
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

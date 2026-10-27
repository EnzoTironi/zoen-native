use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use protocol::{Envelope, Sequenced};
use types::EventBody;

use super::{Applied, Pending};

/// The release binary must not contain this string.
pub const SWITCH: &str = "zoen-fork-histories-v1";
/// How many canonical events a debug sync folds into one view.
pub const READ_LIMIT: usize = 10_000;
const CAUSAL_REJECT: &str = "signed on a history this relay doesn't have";
const FILE_NAME: &str = "fork_histories";

static HELD: LazyLock<Mutex<HashMap<String, Vec<Envelope>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

#[cfg(test)]
thread_local! {
    static SWITCH_OVERRIDE: std::cell::RefCell<Option<String>> =
        const { std::cell::RefCell::new(None) };
}

#[derive(Default)]
struct Switch {
    spaces: HashSet<String>,
    reach: HashSet<String>,
}

pub fn armed(space: &str) -> bool {
    load().spaces.contains(space)
}

pub fn view(
    space: &str,
    reader: &str,
    from: u64,
    canonical: Vec<Sequenced>,
) -> Result<Vec<Sequenced>, serde_json::Error> {
    if !armed(space) {
        return Ok(canonical.into_iter().filter(|ev| ev.seq >= from).collect());
    }
    let reach = load().reach.contains(space);
    let mut chain = cross(reader, &canonical)?;
    append_held(space, reader, &mut chain)?;
    chain = apply_far(chain, reach)?;
    Ok(chain.into_iter().filter(|ev| ev.seq >= from).collect())
}

pub(crate) fn observe(space: &str, pending: &[Pending], applied: &Applied) {
    if !armed(space) {
        return;
    }
    let mut add = Vec::new();
    for (item, result) in pending.iter().zip(&applied.results) {
        match result {
            Err(reject) if reject.reason == CAUSAL_REJECT => add.push(item.env.clone()),
            _ => {}
        }
    }
    if add.is_empty() {
        return;
    }
    let mut held = HELD.lock().expect("fork histories");
    let slot = held.entry(space.to_string()).or_default();
    for env in add {
        if slot.iter().any(|kept| kept.client_id() == env.client_id()) {
            continue;
        }
        slot.push(env);
    }
}

fn load() -> Switch {
    #[cfg(test)]
    if let Some(text) = SWITCH_OVERRIDE.with(|slot| slot.borrow().clone()) {
        return parse(&text);
    }
    let Some(root) = std::env::var_os("ZOEN_DEV_DIR") else {
        return Switch::default();
    };
    read_dir(Path::new(&root))
}

fn read_dir(root: &Path) -> Switch {
    let path = root.join(FILE_NAME);
    match std::fs::read_to_string(path) {
        Ok(text) => parse(&text),
        Err(_) => Switch::default(),
    }
}

fn parse(text: &str) -> Switch {
    let mut out = Switch::default();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line == SWITCH {
            continue;
        }
        if let Some(rest) = line.strip_prefix("reach ") {
            let space = rest.trim();
            if !space.is_empty() {
                out.spaces.insert(space.to_string());
                out.reach.insert(space.to_string());
            }
            continue;
        }
        out.spaces.insert(line.to_string());
    }
    out
}

fn cross(reader: &str, events: &[Sequenced]) -> Result<Vec<Sequenced>, serde_json::Error> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < events.len() {
        let seen = events[i].env.seen();
        let mut end = i + 1;
        if seen.is_some() {
            while end < events.len() && events[end].env.seen() == seen {
                end += 1;
            }
        }
        let group = &events[i..end];
        let reader_wrote = group.iter().any(|ev| ev.env.author() == reader);
        let someone_else = group.iter().any(|ev| ev.env.author() != reader);
        let drop_own = group.len() > 1 && reader_wrote && someone_else;
        for ev in group {
            if drop_own && ev.env.author() == reader {
                continue;
            }
            push_linked(&mut out, ev)?;
        }
        i = end;
    }
    Ok(out)
}

fn append_held(
    space: &str,
    reader: &str,
    chain: &mut Vec<Sequenced>,
) -> Result<(), serde_json::Error> {
    let held = HELD
        .lock()
        .expect("fork histories")
        .get(space)
        .cloned()
        .unwrap_or_default();
    for env in held {
        if env.author() == reader {
            continue;
        }
        if chain.iter().any(|ev| ev.env.client_id() == env.client_id()) {
            continue;
        }
        push_linked(
            chain,
            &Sequenced {
                seq: 0,
                prev: String::new(),
                hash: String::new(),
                env,
            },
        )?;
    }
    Ok(())
}

fn apply_far(chain: Vec<Sequenced>, reach: bool) -> Result<Vec<Sequenced>, serde_json::Error> {
    let Some(index) = chain.iter().position(is_far) else {
        return Ok(chain);
    };
    let checkpoint = &chain[index];
    let Some(seen) = checkpoint.env.seen() else {
        return Ok(chain);
    };
    let client = checkpoint.env.client_id();
    let mut out = Vec::new();
    for ev in chain.iter().filter(|ev| ev.seq <= seen.seq) {
        push_linked(&mut out, ev)?;
    }
    let already = out.last().is_some_and(|ev| ev.env.client_id() == client);
    if !already {
        push_linked(&mut out, checkpoint)?;
    }
    if reach {
        for ev in chain
            .iter()
            .filter(|ev| ev.seq > seen.seq && ev.env.client_id() != client)
        {
            push_linked(&mut out, ev)?;
        }
    }
    Ok(out)
}

fn is_far(ev: &Sequenced) -> bool {
    let Some(seen) = ev.env.seen() else {
        return false;
    };
    match ev.env.body() {
        Some(EventBody::Checkpoint { upto, .. }) => upto.seq > seen.seq,
        _ => false,
    }
}

fn push_linked(out: &mut Vec<Sequenced>, ev: &Sequenced) -> Result<(), serde_json::Error> {
    if linked(out.last(), ev) {
        out.push(ev.clone());
        return Ok(());
    }
    let seq = out.last().map(|prev| prev.seq + 1).unwrap_or(0);
    let prev = out
        .last()
        .map(|prev| prev.hash.clone())
        .unwrap_or_else(|| types::GENESIS_PREV.to_string());
    let hash = event_log::chain_hash(ev.env.space(), seq, &prev, &ev.env.wire_hash())?;
    out.push(Sequenced {
        seq,
        prev,
        hash,
        env: ev.env.clone(),
    });
    Ok(())
}

fn linked(prev: Option<&Sequenced>, ev: &Sequenced) -> bool {
    match prev {
        None => ev.seq == 0 && ev.prev == types::GENESIS_PREV,
        Some(prev) => ev.seq == prev.seq + 1 && ev.prev == prev.hash,
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicU64, Ordering};

    use event_log::{chain_hash, Author, Signer};
    use protocol::Envelope;
    use types::{EventBody, Privacy, Seen, SpaceKind, GENESIS_PREV};

    use super::super::{Applied, Pending};
    use super::*;
    use crate::log::Reject;
    use crate::ownership::{partition_of, Fence};

    static DIR: AtomicU64 = AtomicU64::new(0);

    struct Armed;
    impl Drop for Armed {
        fn drop(&mut self) {
            SWITCH_OVERRIDE.with(|slot| *slot.borrow_mut() = None);
        }
    }

    fn arm(text: &str) -> Armed {
        SWITCH_OVERRIDE.with(|slot| *slot.borrow_mut() = Some(text.to_string()));
        Armed
    }

    fn message(text: &str) -> EventBody {
        EventBody::MessagePosted {
            message: text.into(),
            text: text.into(),
            attaches: None,
            reply: None,
            media: None,
            forwarded: false,
        }
    }

    fn place(
        author: &Author,
        space: &str,
        client: &str,
        prev: Option<&Sequenced>,
        seen: Option<Seen>,
        body: EventBody,
    ) -> std::result::Result<Sequenced, Box<dyn std::error::Error>> {
        let seq = prev.map(|ev| ev.seq + 1).unwrap_or(0);
        let prev_hash = prev
            .map(|ev| ev.hash.clone())
            .unwrap_or_else(|| GENESIS_PREV.to_string());
        let env =
            Envelope::plain(&author.sign_event(space, client, 1_700_000_000_000, seen, body)?)?;
        let hash = chain_hash(space, seq, &prev_hash, &env.wire_hash())?;
        Ok(Sequenced {
            seq,
            prev: prev_hash,
            hash,
            env,
        })
    }

    fn created(
        author: &Author,
        space: &str,
    ) -> std::result::Result<Sequenced, Box<dyn std::error::Error>> {
        place(
            author,
            space,
            "created",
            None,
            None,
            EventBody::SpaceCreated {
                title: space.into(),
                kind: SpaceKind::Group,
                privacy: Privacy::Closed,
            },
        )
    }

    fn seen_of(ev: &Sequenced) -> Seen {
        Seen {
            seq: ev.seq,
            hash: ev.hash.clone(),
        }
    }

    fn linked_chain(chain: &[Sequenced]) -> std::result::Result<(), Box<dyn std::error::Error>> {
        for (i, ev) in chain.iter().enumerate() {
            let prev = if i == 0 {
                GENESIS_PREV.to_string()
            } else {
                chain[i - 1].hash.clone()
            };
            assert_eq!(ev.seq, i as u64);
            assert_eq!(ev.prev, prev);
            assert_eq!(
                ev.hash,
                chain_hash(ev.env.space(), ev.seq, &ev.prev, &ev.env.wire_hash())?
            );
        }
        Ok(())
    }

    fn ids(chain: &[Sequenced]) -> Vec<&str> {
        chain.iter().map(|ev| ev.env.client_id()).collect()
    }

    #[test]
    fn fork_histories_file_names_one_space() {
        let n = DIR.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("u18-fork-file-{n}"));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join(FILE_NAME),
            "# lane\nzoen-fork-histories-v1\nsp-a\nreach sp-b\n\n",
        )
        .unwrap();
        let parsed = read_dir(&dir);
        assert!(parsed.spaces.contains("sp-a"));
        assert!(parsed.reach.contains("sp-b"));
        assert!(parsed.spaces.contains("sp-b"));
        assert!(!parsed.spaces.contains("zoen-fork-histories-v1"));
        let missing = read_dir(&dir.join("absent"));
        assert!(missing.spaces.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn fork_histories_crosses_siblings_for_the_author_only(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let _armed = arm("sp-cross\n");
        let owner = Author::root(Signer::generate()?);
        let a = Author::root(Signer::generate()?);
        let b = Author::root(Signer::generate()?);
        let space = "sp-cross";
        let head = created(&owner, space)?;
        let from_a = place(
            &a,
            space,
            "from-a",
            Some(&head),
            Some(seen_of(&head)),
            message("a"),
        )?;
        let from_b = place(
            &b,
            space,
            "from-b",
            Some(&from_a),
            Some(seen_of(&head)),
            message("b"),
        )?;
        let canonical = vec![head, from_a, from_b];
        let for_a = view(space, a.identity.as_str(), 0, canonical.clone())?;
        let for_b = view(space, b.identity.as_str(), 0, canonical.clone())?;
        let for_owner = view(space, owner.identity.as_str(), 0, canonical.clone())?;
        assert_eq!(ids(&for_a), ["created", "from-b"]);
        assert_eq!(ids(&for_b), ["created", "from-a"]);
        assert_eq!(ids(&for_owner), ["created", "from-a", "from-b"]);
        linked_chain(&for_a)?;
        linked_chain(&for_b)?;
        linked_chain(&for_owner)?;
        let other = view("sp-other", a.identity.as_str(), 0, canonical)?;
        assert_eq!(ids(&other), ["created", "from-a", "from-b"]);
        Ok(())
    }

    #[test]
    fn fork_histories_holds_a_causal_reject_for_the_other_member(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let _armed = arm("sp-held\n");
        let owner = Author::root(Signer::generate()?);
        let other = Author::root(Signer::generate()?);
        let space = "sp-held";
        let head = created(&owner, space)?;
        let rejected = Envelope::plain(&other.sign_event(
            space,
            "held-b",
            1_700_000_000_000,
            Some(Seen {
                seq: 4,
                hash: "12".repeat(32),
            }),
            message("b"),
        )?)?;
        let pending = vec![Pending {
            env: rejected,
            target_known: true,
            managed: None,
            fence: Fence {
                partition: partition_of(space),
                owner: "test".into(),
                token: 1,
            },
        }];
        observe(
            space,
            &pending,
            &Applied {
                results: vec![Err(Reject::no(CAUSAL_REJECT))],
                attempts: 1,
            },
        );
        let for_owner = view(space, owner.identity.as_str(), 0, vec![head.clone()])?;
        let for_author = view(space, other.identity.as_str(), 0, vec![head])?;
        assert_eq!(ids(&for_owner), ["created", "held-b"]);
        assert_eq!(ids(&for_author), ["created"]);
        linked_chain(&for_owner)?;
        observe(
            "sp-other",
            &pending,
            &Applied {
                results: vec![Err(Reject::no(CAUSAL_REJECT))],
                attempts: 1,
            },
        );
        observe(
            space,
            &pending,
            &Applied {
                results: vec![Err(Reject::no("sealed event of an unknown kind"))],
                attempts: 1,
            },
        );
        assert!(!HELD
            .lock()
            .expect("fork histories")
            .contains_key("sp-other"));
        Ok(())
    }

    #[test]
    fn fork_histories_withholds_a_far_checkpoint_until_reach(
    ) -> std::result::Result<(), Box<dyn std::error::Error>> {
        let owner = Author::root(Signer::generate()?);
        let space = "sp-far";
        let head = created(&owner, space)?;
        let fill_1 = place(
            &owner,
            space,
            "fill-1",
            Some(&head),
            Some(seen_of(&head)),
            message("1"),
        )?;
        let fill_2 = place(
            &owner,
            space,
            "fill-2",
            Some(&fill_1),
            Some(seen_of(&fill_1)),
            message("2"),
        )?;
        let checkpoint = place(
            &owner,
            space,
            "far-cp",
            Some(&fill_2),
            Some(seen_of(&head)),
            EventBody::Checkpoint {
                upto: seen_of(&fill_2),
                epoch: 1,
                digest: "ab".repeat(32),
            },
        )?;
        let canonical = vec![head, fill_1, fill_2, checkpoint];
        let early = {
            let _armed = arm("sp-far\n");
            view(space, "reader", 0, canonical.clone())?
        };
        assert_eq!(ids(&early), ["created", "far-cp"]);
        linked_chain(&early)?;
        let upto = match early[1].env.body() {
            Some(EventBody::Checkpoint { upto, .. }) => upto.seq,
            other => panic!("checkpoint, got {other:?}"),
        };
        assert!(upto > early.last().unwrap().seq);
        let late = {
            let _armed = arm("sp-far\nreach sp-far\n");
            view(space, "reader", 0, canonical.clone())?
        };
        assert_eq!(ids(&late), ["created", "far-cp", "fill-1", "fill-2"]);
        linked_chain(&late)?;
        let tail = {
            let _armed = arm("sp-far\nreach sp-far\n");
            view(space, "reader", 2, canonical)?
        };
        assert_eq!(ids(&tail), ["fill-1", "fill-2"]);
        Ok(())
    }
}

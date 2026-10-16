use super::extend_co_members;
use std::{collections::HashSet, time::Instant};

fn baseline(who: &str, members: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut out = Vec::new();
    for member in members {
        if member != who && !out.contains(&member) {
            out.push(member);
        }
    }
    out
}

#[test]
fn preserves_order_across_batches() {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for batch in [vec!["self", "b", "a", "b"], vec!["a", "self", "c", "b"]] {
        extend_co_members(
            &mut out,
            &mut seen,
            "self",
            batch.into_iter().map(str::to_owned),
        );
    }
    assert_eq!(out, vec!["b", "a", "c"]);
    assert_eq!(seen.len(), 3);
}

// Explicit, short benchmark; no machine-dependent performance assertion.
#[test]
#[ignore = "run explicitly with --ignored --nocapture"]
fn before_after() {
    let members: Vec<String> = (0..20_000)
        .map(|i| {
            if i % 101 == 0 {
                "self".to_owned()
            } else {
                format!("member-{:05}", i % 10_000)
            }
        })
        .collect();
    let start = Instant::now();
    let before = baseline("self", members.iter().cloned());
    let old = start.elapsed();
    let mut after = Vec::new();
    let mut seen = HashSet::new();
    let start = Instant::now();
    for batch in members.chunks(256) {
        extend_co_members(&mut after, &mut seen, "self", batch.iter().cloned());
    }
    let new = start.elapsed();
    assert_eq!(before, after);
    println!(
        "{} memberships, {} unique: Vec::contains {:?}; production HashSet {:?}",
        members.len(),
        after.len(),
        old,
        new
    );
}

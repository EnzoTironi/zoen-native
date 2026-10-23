//! Exact collision fixtures for the lossy 64-slot peer sketch. These exercise
//! production recording/merge behavior without pretending the sketch is exact.
use super::*;

fn first_week_counts(people: [&str; 3], expected_slots: [u8; 3]) -> [u32; 2] {
    let analytics = Analytics::with_key([0; 32], "");
    for (person, slot) in people.iter().zip(expected_slots) {
        assert_eq!(analytics.peer_bit(person), 1u64 << slot);
    }
    let [ana, bruno, carla] = people;
    let before_join = vec![ana.to_string(), bruno.to_string()];
    let after_join = vec![ana.to_string(), bruno.to_string(), carla.to_string()];
    for (author, audience) in [
        (ana, &before_join),
        (bruno, &before_join),
        (carla, &after_join),
    ] {
        analytics.message_sent(SentMessage {
            author,
            space: "trip",
            audience,
        });
    }
    // The same lossless OR is used when a failed flush merges back into pending.
    let retained = analytics.with(std::mem::take);
    for _ in 0..2 {
        analytics.message_sent(SentMessage {
            author: ana,
            space: "trip",
            audience: &after_join,
        });
    }
    analytics.with(|pending| pending.merge(retained));
    analytics.with(|pending| {
        // Calls may straddle midnight; the report ORs peer bits over all days.
        // Aggregate that same way instead of depending on one wall-clock bucket.
        let account = |who: &str| {
            pending
                .accounts
                .iter()
                .filter(|((_, id), _)| id == who)
                .fold((0, 0u64), |(sent, peers), (_, activity)| {
                    (sent + activity.sent, peers | activity.peers)
                })
        };
        let (ana_sent, ana_peers) = account(ana);
        let (bruno_sent, bruno_peers) = account(bruno);
        let (carla_sent, carla_peers) = account(carla);
        assert_eq!((ana_sent, bruno_sent, carla_sent), (3, 1, 1));
        assert_eq!(pending.accounts.values().map(|a| a.sent).sum::<i32>(), 5);
        assert_eq!(bruno_peers, analytics.peer_bit(ana), "self is excluded");
        assert_eq!(
            ana_peers,
            analytics.peer_bit(bruno) | analytics.peer_bit(carla)
        );
        assert_eq!(
            carla_peers,
            analytics.peer_bit(ana) | analytics.peer_bit(bruno)
        );
        assert_eq!(pending.spaces.values().map(|s| s.messages).sum::<i32>(), 5);
        assert_eq!(pending.spaces.values().map(|s| s.members).max(), Some(3));
        [ana_peers.count_ones(), carla_peers.count_ones()]
    })
}

#[test]
fn distinct_slots_keep_both_qualifying_senders() {
    assert_eq!(
        first_week_counts(["peer-0", "peer-1", "peer-2"], [38, 16, 42]),
        [2, 2]
    );
}

#[test]
fn one_collision_hides_only_one_qualifying_sender() {
    // Bruno=Carla hides Ana's second recipient; Ana=Bruno hides Carla's.
    assert_eq!(
        first_week_counts(["peer-1", "peer-0", "peer-61"], [16, 38, 38]),
        [1, 2]
    );
    assert_eq!(
        first_week_counts(["peer-0", "peer-61", "peer-1"], [38, 38, 16]),
        [2, 1]
    );
}

#[test]
fn two_collisions_can_hide_both_qualifying_senders() {
    assert_eq!(
        first_week_counts(["peer-0", "peer-61", "peer-64"], [38, 38, 38]),
        [1, 1]
    );
}

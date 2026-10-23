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
        assert_eq!(pending.accounts.len(), 3);
        let account = |who: &str| {
            pending
                .accounts
                .iter()
                .find_map(|((_, id), activity)| (id == who).then_some(activity))
                .expect("recorded sender")
        };
        let ana_count = account(ana);
        let bruno_count = account(bruno);
        let carla_count = account(carla);
        assert_eq!(
            (ana_count.sent, bruno_count.sent, carla_count.sent),
            (3, 1, 1)
        );
        assert_eq!(
            bruno_count.peers,
            analytics.peer_bit(ana),
            "self is excluded"
        );
        assert_eq!(
            ana_count.peers,
            analytics.peer_bit(bruno) | analytics.peer_bit(carla)
        );
        assert_eq!(
            carla_count.peers,
            analytics.peer_bit(ana) | analytics.peer_bit(bruno)
        );
        let space = pending.spaces.values().next().expect("recorded trip");
        assert_eq!((space.messages, space.members), (5, 3));
        [ana_count.peers.count_ones(), carla_count.peers.count_ones()]
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

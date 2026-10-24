use super::{headline, view};
use serde_json::json;

#[test]
fn pet_elapsed_time_handles_the_entire_signed_clock_range_without_mutating_state() {
    let state = json!({
        "updated": i64::MIN, "fullness": 100, "energy": 100, "joy": 100,
        "asleep": false, "released": false,
    });
    let projected = view("pet", &state, i64::MAX);
    for field in ["fullness", "energy", "joy"] {
        assert_eq!(projected[field], 0.0, "{field}");
        assert_eq!(state[field], 100, "projection must not mutate {field}");
    }
    assert_eq!(state["updated"], i64::MIN);
}

#[test]
fn a_future_pet_timestamp_does_not_reverse_decay_or_overflow_on_clock_rollback() {
    let state = json!({
        "updated": i64::MAX, "fullness": 80, "energy": 70, "joy": 60,
        "asleep": false, "released": false,
    });
    let projected = view("pet", &state, i64::MIN);
    for field in ["fullness", "energy", "joy"] {
        assert_eq!(projected[field].as_f64(), state[field].as_f64(), "{field}");
    }
}

#[test]
fn countdown_preserves_exact_days_across_the_entire_signed_clock_range() {
    let state = json!({"title": "Clock range", "target": i64::MAX});
    let (line, metrics) = headline("countdown", &state, i64::MIN);
    assert!(line.contains("213503982334"), "{line}");
    assert!(metrics.is_empty());
    let (expired, _) = headline("countdown", &json!({"target": i64::MIN}), i64::MAX);
    assert!(expired.contains('0'), "{expired}");
}

#[test]
fn ordinary_pet_projection_keeps_hourly_decay_and_sleep_recovery() {
    for (asleep, energy) in [(false, 62.0), (true, 94.0)] {
        let state = json!({
            "updated": 1_000, "fullness": 80, "energy": 70, "joy": 60,
            "asleep": asleep, "released": false,
        });
        let projected = view("pet", &state, 7_201_000);
        assert_eq!(projected["fullness"], 68.0);
        assert_eq!(projected["energy"], energy);
        assert_eq!(projected["joy"], 50.0);
        assert_eq!(state["updated"], 1_000);
    }
}

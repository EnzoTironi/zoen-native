//! Numeric refusal and read-only projection guarantees at public app boundaries.
use super::{apply, create, headline, snapshot};
use serde_json::json;

#[test]
fn map_day_selection_accepts_the_signed_clock_range_without_overflow() {
    for now in [i64::MIN, -86_400_000, 0, 86_400_000, i64::MAX] {
        let (_, state) = create("maptap", &json!({}), "person", now).unwrap();
        let places = state["places"].as_array().unwrap();
        assert_eq!(places.len(), 5);
        for place in places {
            assert!(place["name"].as_str().is_some_and(|name| !name.is_empty()));
            assert!(place["lat"].as_f64().is_some_and(f64::is_finite));
            assert!(place["lon"].as_f64().is_some_and(f64::is_finite));
        }
    }
}

#[test]
fn invalid_distance_does_not_become_a_successful_zero_score() {
    let (_, mut state) = create("maptap", &json!({}), "person", 0).unwrap();
    state["places"][0]["lat"] = json!(-1.7e308);
    let prior = state.clone();
    let invalid = json!({"round":0,"lat":1.7e308,"lon":0});
    assert!(apply("maptap", "maptap_guess", &invalid, &state, "person", 0).is_err());
    assert_eq!(state, prior);

    let (_, state) = create("maptap", &json!({}), "person", 0).unwrap();
    let exact = json!({"round":0,"lat":state["places"][0]["lat"],"lon":state["places"][0]["lon"]});
    let accepted = apply("maptap", "maptap_guess", &exact, &state, "person", 0).unwrap();
    assert_eq!(accepted.state["guesses"]["person"]["0"]["points"], 5000);
    assert_eq!(accepted.state["guesses"]["person"]["0"]["km"], 0.0);
}

#[test]
fn visual_completion_ratios_preserve_exact_counts_and_canonical_state() {
    let state = json!({"question":"Choice","options":[
        {"text":"A","votes":["one","two"]},{"text":"B","votes":["three"]}
    ]});
    let prior = state.clone();
    let (label, metrics) = headline("poll", &state, 0);
    assert!(label.contains('3'));
    assert_eq!(metrics[0].1, 2.0 / 3.0);
    assert_eq!(metrics[1].1, 1.0 / 3.0);
    let card = snapshot("poll", &state, "item", 0).unwrap();
    assert_eq!(card["bars"][0]["value"].as_f64(), Some(2.0 / 3.0));
    assert_eq!(state, prior);
    assert_eq!(state["options"][0]["votes"].as_array().unwrap().len(), 2);

    for (app, state) in [
        (
            "list",
            json!({"title":"List","items":[{"done":true},{"done":false}]}),
        ),
        (
            "recipe",
            json!({"title":"Recipe","ingredients":[{"done":true},{"done":false}]}),
        ),
    ] {
        let prior = state.clone();
        assert_eq!(headline(app, &state, 0).1[0].1, 0.5);
        assert!(snapshot(app, &state, "item", 0).is_some());
        assert_eq!(state, prior);
    }
}

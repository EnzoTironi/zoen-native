use roda_apps::{apply_as, create_as, spec, view_for, Actor, Effect};
use roda_i18n::Lang;
use serde_json::json;

#[test]
fn list_exposes_only_real_shared_actions_without_simulated_external_delivery() {
    for language in [Lang::En, Lang::Pt] {
        let _language = roda_i18n::enter(language);
        let actor = Actor {
            id: "member",
            name: "Member",
            owner: None,
        };
        assert_eq!(
            spec("list")
                .unwrap()
                .tools
                .iter()
                .map(|tool| tool.name)
                .collect::<Vec<_>>(),
            ["start_list", "list_add", "list_toggle", "list_get_state"]
        );
        let (_, state) = create_as(
            "list",
            &json!({"title":"Trip", "items":["Water"]}),
            &actor,
            1,
        )
        .unwrap();
        assert!(state.get("sent").is_none());
        assert!(view_for("list", &state, 2, actor.id).get("sent").is_none());
        assert!(apply_as("list", "list_send_whatsapp", &json!({}), &state, &actor, 2,).is_err());
        let added = apply_as(
            "list",
            "list_add",
            &json!({"text":"Hat"}),
            &state,
            &actor,
            3,
        )
        .unwrap();
        assert!(added.effect.is_none());
        let checked = apply_as(
            "list",
            "list_toggle",
            &json!({"id":"l100"}),
            &added.state,
            &actor,
            4,
        )
        .unwrap();
        assert_eq!(checked.state["items"][1]["text"], "Hat");
        assert_eq!(checked.state["items"][1]["done"], true);
        assert!(checked.state.get("sent").is_none());
        assert!(checked.effect.is_none());
        let read = view_for("list", &checked.state, 5, actor.id);
        assert_eq!(read["items"], checked.state["items"]);
        assert!(read.get("sent").is_none());
    }
}

#[test]
fn poll_close_emits_one_plan_effect_and_refuses_repeated_close_from_canonical_state() {
    for (language, reason) in [
        (Lang::En, "the poll is already closed"),
        (Lang::Pt, "a enquete já fechou"),
    ] {
        let _language = roda_i18n::enter(language);
        let actor = Actor {
            id: "member",
            name: "Member",
            owner: None,
        };
        let (_, state) = create_as(
            "poll",
            &json!({"question":"Where?", "options":["Park", "Beach"]}),
            &actor,
            1,
        )
        .unwrap();
        let vote = apply_as(
            "poll",
            "poll_vote",
            &json!({"option":"o2"}),
            &state,
            &actor,
            2,
        )
        .unwrap();
        let closed = apply_as("poll", "poll_close", &json!({}), &vote.state, &actor, 3).unwrap();
        assert_eq!(closed.state["closed"], true);
        assert_eq!(closed.state["winner"], "Beach");
        match &closed.effect {
            Some(Effect::AddPlanLine { text, cost_cents }) => {
                assert!(text.starts_with("Beach"));
                assert_eq!(*cost_cents, 0);
            }
            _ => panic!("Closing a voted poll must produce its single plan effect"),
        }
        let another = Actor {
            id: "another",
            name: "Another",
            owner: None,
        };
        for caller in [&actor, &another] {
            let Err(error) = apply_as("poll", "poll_close", &json!({}), &closed.state, caller, 4)
            else {
                panic!("A repeated close must not emit another plan effect")
            };
            assert_eq!(error, reason);
        }
    }
}

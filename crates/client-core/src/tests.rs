//! Testes de ponta a ponta do motor: seed → ações → reabrir → reverificar.

use super::*;

mod chat_muting;
mod client_api;
mod edit_convergence;
mod file_bounds;
mod file_edits;
mod file_messages;
mod message_search;
mod peer_receipts;
mod profile_photos;
mod trust_baseline;
mod voice_notes;

#[test]
fn seed_builds_the_story_and_every_log_verifies() {
    seed_locale_cases::seed_builds_the_story_and_every_log_verifies()
}

#[test]
fn request_previews_use_the_action_title_and_preserve_message_bodies() {
    conversation_edits_cases::request_previews_use_the_action_title_and_preserve_message_bodies()
}

#[test]
fn demo_receipts_reactions_and_threads_survive_reopen() {
    seed_locale_cases::demo_receipts_reactions_and_threads_survive_reopen()
}

#[test]
fn edit_then_undo_restores_content_and_keeps_history() {
    conversation_edits_cases::edit_then_undo_restores_content_and_keeps_history()
}

#[test]
fn approval_is_bound_to_the_exact_content() {
    agent_permissions_cases::approval_is_bound_to_the_exact_content()
}

#[test]
fn approve_all_and_deny() {
    agent_permissions_cases::approve_all_and_deny()
}

#[test]
fn trust_levels_change_what_agents_can_do() {
    agent_permissions_cases::trust_levels_change_what_agents_can_do()
}

#[test]
fn budget_stops_at_ceiling_and_can_be_raised() {
    agent_permissions_cases::budget_stops_at_ceiling_and_can_be_raised()
}

#[test]
fn budget_rejects_nonpositive_and_overflowing_increases_without_mutation() {
    agent_permissions_cases::budget_rejects_nonpositive_and_overflowing_increases_without_mutation()
}

#[test]
fn everything_persists_and_reverifies_after_reopen() {
    seed_locale_cases::everything_persists_and_reverifies_after_reopen()
}

#[test]
fn search_finds_items_and_messages_without_accents() {
    seed_locale_cases::search_finds_items_and_messages_without_accents()
}

#[test]
fn log_events_are_signed_and_chained() {
    seed_locale_cases::log_events_are_signed_and_chained()
}

#[test]
fn nonmember_agent_cannot_publish_to_community() {
    agent_permissions_cases::nonmember_agent_cannot_publish_to_community()
}

#[tokio::test]
async fn an_offline_invite_cannot_append_a_grant_or_manufacture_a_link() {
    agent_permissions_cases::an_offline_invite_cannot_append_a_grant_or_manufacture_a_link().await
}

#[test]
fn agent_activity_reads_the_log_with_costs() {
    agent_permissions_cases::agent_activity_reads_the_log_with_costs()
}

// ── MCP Apps ──

#[test]
fn mcp_specs_follow_the_apps_extension() {
    collaborative_apps_cases::mcp_specs_follow_the_apps_extension()
}

#[test]
fn pet_is_shared_state_versioned_and_gated_by_grants() {
    interactive_apps_cases::pet_is_shared_state_versioned_and_gated_by_grants()
}

#[test]
fn poll_closing_writes_the_winner_into_the_plan() {
    collaborative_apps_cases::poll_closing_writes_the_winner_into_the_plan()
}

#[test]
fn list_shared_updates_do_not_expose_simulated_external_delivery() {
    collaborative_apps_cases::list_shared_updates_do_not_expose_simulated_external_delivery()
}

#[test]
fn pet_follows_the_group_story_rename_sleep_dash_and_members() {
    interactive_apps_cases::pet_follows_the_group_story_rename_sleep_dash_and_members()
}

#[test]
fn maptap_is_turn_based_with_the_same_places_for_everyone() {
    interactive_apps_cases::maptap_is_turn_based_with_the_same_places_for_everyone()
}

#[test]
fn recipe_servings_and_checklist_are_shared() {
    collaborative_apps_cases::recipe_servings_and_checklist_are_shared()
}

#[test]
fn english_is_the_default_language_for_demo_data_agents_and_money() {
    seed_locale_cases::english_is_the_default_language_for_demo_data_agents_and_money()
}

#[test]
fn switching_the_device_language_restarts_the_demo_in_that_language() {
    seed_locale_cases::switching_the_device_language_restarts_the_demo_in_that_language()
}

#[test]
fn widget_snapshots_are_strict_and_deep_link_to_their_item() {
    seed_locale_cases::widget_snapshots_are_strict_and_deep_link_to_their_item()
}

#[test]
fn hike_votes_decide_plan_and_album() {
    collaborative_apps_cases::hike_votes_decide_plan_and_album()
}

// ── Agenda (generic booking) ──

const SCHEDULE_ARGS: &str = r#"{"business":"Barbearia do João","services":[{"id":"Corte!","name":"Corte","priceCents":4500,"minutes":30},{"name":"Corte + barba","priceCents":7000,"minutes":60},{"id":"corte","name":"Corte infantil","priceCents":3500,"minutes":30}],"weeklyHours":{"1":[["09:00","12:00"],["14:00","19:00"]],"2":[["09:00","12:00"],["14:00","19:00"]],"3":[["09:00","12:00"],["14:00","19:00"]],"4":[["09:00","12:00"],["14:00","19:00"]],"5":[["09:00","12:00"],["14:00","19:00"]],"6":[["09:00","13:00"]]},"slotMinutes":30,"timezone":"America/Sao_Paulo"}"#;

#[test]
fn schedule_journey_client_books_agent_resolves_privately() {
    let e = seeded();
    let (item, zoen) = schedule_item(&e);
    let r = e.read_app_resource("ui://builtin/schedule".into()).unwrap();
    let manifest: serde_json::Value = serde_json::from_str(&r.manifest_json).unwrap();
    use sha2::Digest;
    assert_eq!(
        manifest["sha256"].as_str().unwrap(),
        format!("{:x}", sha2::Sha256::digest(r.text.as_bytes())),
        "manifest hash matches schedule.html"
    );
    assert_eq!(
        manifest["sha256"].as_str().unwrap(),
        format!(
            "{:x}",
            sha2::Sha256::digest(include_bytes!("../../miniapps/resources/schedule.html"))
        )
    );
    // Services were sanitized and deduped; the business is free-form, no trade.
    let (v, _) = sched_call(&e, &item, "me", "schedule_get_state", "{}".into());
    let ids: Vec<&str> = v["services"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, ["corte", "corte-barba", "corte-2"]);
    assert!(v.get("trade").is_none());
    assert_eq!(v["viewer"]["role"], "owner");

    let start = sp_slot(10, 0);
    let (v, err) = book(&e, &item, "marina", "corte", start);
    assert!(!err, "{v}");
    assert_eq!(v["viewer"]["role"], "client");
    assert_eq!(v["bookings"][0]["status"], "pending");
    assert_eq!(v["bookings"][0]["endMs"], start + 30 * 60_000);
    assert_eq!(v["events"][0]["kind"], "book_request");
    let booking = v["bookings"][0]["id"].as_str().unwrap().to_string();
    let marina_id = v["bookings"][0]["byId"].as_str().unwrap().to_string();

    // Another client sees the time as busy, never who booked it or the note.
    let (v, _) = sched_call(&e, &item, "lucas", "schedule_get_state", "{}".into());
    assert_eq!(v["bookings"].as_array().unwrap().len(), 0);
    assert_eq!(v["events"].as_array().unwrap().len(), 0);
    assert_eq!(v["busy"][0]["startMs"], start);
    let raw = v.to_string();
    assert!(
        !raw.contains("Marina") && !raw.contains(&marina_id) && !raw.contains("alergia"),
        "{raw}"
    );
    // …and cannot resolve or cancel it.
    let resolve = |action: &str| format!(r#"{{"bookingId":"{booking}","action":"{action}"}}"#);
    assert!(sched_call(&e, &item, "lucas", "schedule_resolve", resolve("confirm")).1);
    assert!(
        sched_call(
            &e,
            &item,
            "lucas",
            "schedule_cancel",
            format!(r#"{{"bookingId":"{booking}"}}"#)
        )
        .1
    );
    // The booker can't confirm their own request either.
    assert!(sched_call(&e, &item, "marina", "schedule_resolve", resolve("confirm")).1);
    // The owner sees everything, matched by member id.
    let (v, _) = sched_call(&e, &item, "me", "schedule_get_state", "{}".into());
    assert_eq!(v["bookings"][0]["byId"], marina_id.as_str());
    assert_eq!(v["bookings"][0]["note"], "alergia a lâmina");

    // The agent persona resolves.
    let (v, err) = sched_call(
        &e,
        &item,
        &zoen.handle,
        "schedule_resolve",
        resolve("confirm"),
    );
    assert!(!err, "{v}");
    assert_eq!(v["viewer"]["role"], "agent");
    assert_eq!(v["bookings"][0]["status"], "confirmed");
    assert_eq!(v["events"][0]["status"], "resolved");
    assert_eq!(v["events"][0]["resolution"], "confirmed");
    assert_eq!(v["events"][0]["resolvedBy"], zoen.id.as_str());

    // A cancel request on a confirmed booking goes to the agent, who can keep it…
    let cancel = format!(r#"{{"bookingId":"{booking}"}}"#);
    let (v, err) = sched_call(&e, &item, "marina", "schedule_cancel", cancel.clone());
    assert!(!err, "{v}");
    assert_eq!(v["bookings"][0]["status"], "confirmed");
    assert!(
        sched_call(&e, &item, "marina", "schedule_cancel", cancel.clone()).1,
        "one open request at a time"
    );
    let (v, err) = sched_call(&e, &item, &zoen.handle, "schedule_resolve", resolve("keep"));
    assert!(!err, "{v}");
    assert_eq!(v["bookings"][0]["status"], "confirmed");
    // …or cancel it. The event history stays.
    sched_call(&e, &item, "marina", "schedule_cancel", cancel);
    let (v, err) = sched_call(
        &e,
        &item,
        &zoen.handle,
        "schedule_resolve",
        resolve("cancel"),
    );
    assert!(!err, "{v}");
    assert_eq!(v["bookings"][0]["status"], "cancelled");
    let events = v["events"].as_array().unwrap();
    let res: Vec<(&str, &str)> = events
        .iter()
        .map(|e| {
            (
                e["kind"].as_str().unwrap(),
                e["resolution"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(
        res,
        [
            ("book_request", "confirmed"),
            ("cancel_request", "kept"),
            ("cancel_request", "cancelled")
        ]
    );
    assert!(events.iter().all(|e| e["status"] == "resolved"));
    let ids: Vec<&str> = events.iter().map(|e| e["id"].as_str().unwrap()).collect();
    assert!(ids.iter().all(|id| id.starts_with('e')) && ids.windows(2).all(|w| w[0] != w[1]));
}

#[test]
fn schedule_rejects_double_booking_outside_hours_and_past() {
    schedule_cases::schedule_rejects_double_booking_outside_hours_and_past()
}

#[test]
fn schedule_caps_spam_and_only_owner_or_agent_configure() {
    schedule_cases::schedule_caps_spam_and_only_owner_or_agent_configure()
}

#[test]
fn start_schedule_refuses_bad_input_instead_of_defaults() {
    schedule_cases::start_schedule_refuses_bad_input_instead_of_defaults()
}

#[test]
fn device_grants_are_per_mini_app_logged_and_revocable() {
    agent_permissions_cases::device_grants_are_per_mini_app_logged_and_revocable()
}

#[test]
fn background_is_a_typed_event_with_verified_media() {
    media_search_cases::background_is_a_typed_event_with_verified_media()
}

#[test]
fn background_and_media_survive_reopen() {
    media_search_cases::background_and_media_survive_reopen()
}

#[test]
fn universal_search_is_prefix_accent_insensitive_and_fast() {
    media_search_cases::universal_search_is_prefix_accent_insensitive_and_fast()
}

#[test]
fn universal_search_timing_report() {
    media_search_cases::universal_search_timing_report()
}

#[derive(Default)]
struct MemVault(std::sync::Mutex<std::collections::HashMap<String, Vec<u8>>>);

impl SecretVault for MemVault {
    fn load(&self, key: String) -> Option<Vec<u8>> {
        self.0.lock().unwrap().get(&key).cloned()
    }
    fn save(&self, key: String, value: Vec<u8>) -> bool {
        self.0.lock().unwrap().insert(key, value);
        true
    }
    fn delete(&self, key: String) {
        self.0.lock().unwrap().remove(&key);
    }
}

/// The app's first launch in the simulator journey: erase, sign up, chat, search; then the
/// process dies. Everything must be on disk when it opens again.
#[test]
fn account_and_chats_survive_a_relaunch_after_search() {
    media_search_cases::account_and_chats_survive_a_relaunch_after_search()
}

#[test]
fn unconfirmed_genesis_holds_its_descendants_across_retry_and_relaunch() {
    transport_recovery_cases::unconfirmed_genesis_holds_its_descendants_across_retry_and_relaunch()
}

#[test]
fn permanently_rejected_genesis_fails_descendants_atomically_across_relaunch() {
    transport_recovery_cases::permanently_rejected_genesis_fails_descendants_atomically_across_relaunch()
}

/// A relay that hides one message and rehashes the chain around the gap shows this device
/// a valid-looking chain, but the next author's causal link names the hidden event, so the
/// device refuses the forged history instead of showing it.
#[test]
fn a_device_refuses_a_relay_that_hides_a_message_and_rehashes() {
    transport_recovery_cases::a_device_refuses_a_relay_that_hides_a_message_and_rehashes().unwrap()
}

#[test]
fn inline_replies_quote_and_thread_replies_gather_under_the_root() {
    conversation_edits_cases::inline_replies_quote_and_thread_replies_gather_under_the_root()
}

#[test]
fn immediate_cold_reopen_after_sync_stop() {
    transport_recovery_cases::immediate_cold_reopen_after_sync_stop().unwrap()
}

#[test]
fn immediate_cold_reopen_after_implicit_sync_drop() {
    transport_recovery_cases::immediate_cold_reopen_after_implicit_sync_drop().unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn immediate_cold_reopen_inside_an_unrelated_runtime() {
    transport_recovery_cases::immediate_cold_reopen_inside_an_unrelated_runtime()
        .await
        .unwrap()
}

#[test]
fn callback_can_stop_and_destroy_its_core_before_immediate_reopen() {
    transport_recovery_cases::callback_can_stop_and_destroy_its_core_before_immediate_reopen()
        .unwrap()
}

#[test]
fn the_agent_books_only_for_the_member_it_is_serving() {
    schedule_cases::the_agent_books_only_for_the_member_it_is_serving()
}

#[test]
fn failed_delivery_send_does_not_consume_the_watermark() {
    receipts_cases::failed_delivery_send_does_not_consume_the_watermark()
}

#[test]
fn receipt_replay_survives_offline_reads_and_restarts() {
    receipts_cases::receipt_replay_survives_offline_reads_and_restarts().unwrap()
}

#[test]
fn ticks_follow_the_lowest_read_up_to_watermark() {
    receipts_cases::ticks_follow_the_lowest_read_up_to_watermark()
}

#[test]
fn edits_deletes_and_reactions_fold_over_the_original() {
    conversation_edits_cases::edits_deletes_and_reactions_fold_over_the_original()
}

#[test]
fn ana_replaces_and_removes_only_her_reaction_while_bias_survives_replay() {
    conversation_edits_cases::ana_replaces_and_removes_only_her_reaction_while_bias_survives_replay(
    )
    .unwrap()
}

#[test]
fn chat_prefs_live_in_my_personal_space_and_last_one_wins() {
    conversation_edits_cases::chat_prefs_live_in_my_personal_space_and_last_one_wins()
}

mod deckcraft_registration;

// Complete cases keep their original annotated entrypoints above.
mod agent_permissions_cases;
mod collaborative_apps_cases;
mod conversation_edits_cases;
mod interactive_apps_cases;
mod media_search_cases;
mod receipts_cases;
mod schedule_cases;
mod seed_locale_cases;
mod support;
mod transport_recovery_cases;
use support::*;

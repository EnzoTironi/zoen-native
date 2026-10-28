//! Ordered tool dispatch over one materialized app state.

use super::{
    clamp, clock, haversine_km, hike_name, json, pet_mood, push_log, s, st_day, t, thousands, tr,
    ts, view, Applied, Effect, Value, HIKE_TRAILS,
};

mod collaboration;
mod geography;
mod hike;
mod pet;
mod recipe;

/// # Errors
/// Refuses unknown tools, invalid arguments, missing domain state or operations forbidden by the current state.
pub fn apply(
    app: &str,
    tool_name: &str,
    args: &Value,
    state: &Value,
    actor: &str,
    now: i64,
) -> Result<Applied, String> {
    // Materializa o tempo passado antes de mudar (o bichinho sentiu fome enquanto isso).
    let st = view(app, state, now);
    match (app, tool_name) {
        ("pet", tool) => pet::apply(tool, args, st, actor, now),
        ("maptap", "maptap_guess") => geography::maptap_guess(args, st, actor, now),
        ("recipe", "recipe_servings") => recipe::recipe_servings(args, st, actor, now),
        ("recipe", "recipe_check") => recipe::recipe_check(args, st, actor, now),
        ("recipe", "recipe_cook") => recipe::recipe_cook(args, st, actor, now),
        ("poll", "poll_vote") => collaboration::poll_vote(args, st, actor, now),
        ("poll", "poll_add_option") => collaboration::poll_add_option(args, st, actor, now),
        ("poll", "poll_close") => collaboration::poll_close(st, actor, now),
        ("list", "list_add") => collaboration::list_add(args, st, actor, now),
        ("list", "list_toggle") => collaboration::list_toggle(args, st, actor, now),
        ("hike", "hike_vote") => hike::hike_vote(args, st, actor, now),
        ("hike", "hike_decide") => hike::hike_decide(st, actor, now),
        ("hike", "hike_set_itinerary") => hike::hike_set_itinerary(args, st, actor, now),
        ("hike", "hike_add_photos") => hike::hike_add_photos(args, st, actor, now),

        ("countdown", "countdown_get_state") => Ok(Applied::new(st, String::new(), String::new())),
        (a, tool) => Err(tr!(
            "{a} não tem a ferramenta {tool}",
            "{a} has no tool {tool}"
        )),
    }
}

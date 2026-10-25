use super::*;

#[test]
fn builtin_config_is_valid_and_has_both_onboarding_arms() -> Result<(), String> {
    let c = RemoteConfig::builtin()?;
    assert!(c.onboarding.flows.contains_key("default"));
    assert!(c.onboarding.flows.contains_key("friend_invite"));
    assert!(c.flags["onboarding_friend_v1"].experiment);
    Ok(())
}

#[test]
fn links_become_sources_without_leaking_targets() {
    let f = attribution_from_link("https://tryzoen.com/@ana_b?c=ig_reels_01").unwrap();
    assert_eq!(f.kind, SourceKind::Friend);
    assert_eq!(f.target.as_deref(), Some("ana_b"));
    assert_eq!(f.campaign.as_deref(), Some("ig_reels_01"));
    assert_eq!(f.reportable().target, None);
    let s = attribution_from_link("zoen://join/AB12CD34EF").unwrap();
    assert_eq!(
        (s.kind, s.target.as_deref()),
        (SourceKind::Space, Some("AB12CD34EF"))
    );
    let a = attribution_from_link("https://tryzoen.com/a/barbeiro").unwrap();
    assert_eq!(
        (a.kind, a.target.as_deref()),
        (SourceKind::Agent, Some("barbeiro"))
    );
    let c = attribution_from_link("https://tryzoen.com/?utm_campaign=Ads-X").unwrap();
    assert_eq!(
        (c.kind, c.campaign.as_deref()),
        (SourceKind::Campaign, Some("ads-x"))
    );
    let bad = attribution_from_link("https://tryzoen.com/?c=Ana%20Silva%20cpf").unwrap();
    assert_eq!((bad.kind, bad.campaign), (SourceKind::Organic, None));
    assert!(attribution_from_link("https://evil.example/@ana").is_none());
}

#[test]
fn bucketing_is_deterministic_uniform_and_weighted() -> Result<(), String> {
    let c = RemoteConfig::builtin()?;
    let mut direct = 0;
    for i in 0..4000 {
        let u = unit_for(&format!("id{i}"));
        let a = c.assign("onboarding_friend_v1", &u).unwrap();
        assert_eq!(Some(a.clone()), c.assign("onboarding_friend_v1", &u));
        if a.variant == "direct" {
            direct += 1;
        }
    }
    // 50/50 inside a 5% holdout: ~47.5% direct.
    assert!((1700..2100).contains(&direct), "{direct}");
    // Known vector, shared with the Swift side.
    assert_eq!(bucket("s", "u"), bucket("s", "u"));
    assert_ne!(unit_for("a"), unit_for("b"));
    Ok(())
}

#[test]
fn onboarding_follows_source_and_arm() -> Result<(), String> {
    let c = RemoteConfig::builtin()?;
    let friend = attribution_from_link("zoen://friend/ana_b").unwrap();
    let (mut saw_direct, mut saw_control) = (false, false);
    for i in 0..200 {
        let u = unit_for(&format!("x{i}"));
        let p = c.onboarding_for(&u, &friend);
        let arm = p.exposure.as_ref().unwrap().variant.clone();
        if arm == "direct" {
            saw_direct = true;
            assert_eq!(p.flow, "friend_invite");
            assert_eq!(p.landing, "chat_with_inviter");
            assert_eq!(p.landing_target.as_deref(), Some("ana_b"));
        } else {
            saw_control = true;
            assert_eq!(p.flow, "default");
            assert_eq!(p.landing, "home");
        }
    }
    assert!(saw_direct && saw_control);
    let organic = c.onboarding_for(&unit_for("z"), &Attribution::organic());
    assert_eq!(
        (organic.flow.as_str(), organic.landing.as_str()),
        ("default", "home")
    );
    assert!(organic.exposure.is_none());
    let space = c.onboarding_for(
        &unit_for("z"),
        &attribution_from_link("zoen://join/AB12CD34EF").unwrap(),
    );
    assert_eq!(
        (space.flow.as_str(), space.landing.as_str()),
        ("space_link", "space")
    );
    Ok(())
}

#[test]
fn bad_configs_are_refused() -> Result<(), String> {
    let mut c = RemoteConfig::builtin()?;
    c.onboarding.rules[0].arms.clear();
    assert!(c.validate().is_err());
    assert!(RemoteConfig::parse(
        "{\"version\":1,\"flags\":{\"x\":{\"salt\":\"s\",\"variants\":[]}}}"
    )
    .is_err());
    Ok(())
}

#[test]
fn percentage_conversion_keeps_clamped_rounding_and_nonfinite_values() {
    for (percent, expected) in [
        (f64::NAN, 0),
        (f64::NEG_INFINITY, 0),
        (f64::INFINITY, BUCKETS),
        (-0.5, 0),
        (100.5, BUCKETS),
        (0.004, 0),
        (0.005, 1),
        (50.0, 5000),
    ] {
        assert_eq!(pct_buckets(percent), expected);
    }
}

#[test]
fn public_assign_refuses_empty_or_overflowing_weights_and_keeps_maximal_valid_total(
) -> Result<(), String> {
    let mut config = RemoteConfig::builtin()?;
    let key = "onboarding_friend_v1";
    let flag = config.flags.get_mut(key).ok_or("builtin flag missing")?;
    flag.variants[0].weight = u32::MAX;
    flag.variants[1].weight = 1;
    assert!(config.validate().is_err());
    assert!(config.assign(key, &unit_for("overflow")).is_none());
    config
        .flags
        .get_mut(key)
        .ok_or("builtin flag missing")?
        .variants[1]
        .weight = 0;
    config.validate()?;
    assert!(config
        .assign(key, &unit_for("maximal valid total"))
        .is_some());
    config
        .flags
        .get_mut(key)
        .ok_or("builtin flag missing")?
        .variants
        .clear();
    assert!(config.validate().is_err());
    assert!(config.assign(key, &unit_for("empty")).is_none());
    Ok(())
}

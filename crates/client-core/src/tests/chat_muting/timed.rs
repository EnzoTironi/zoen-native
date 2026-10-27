use super::*;
use crate::ChatMuteMode;
const NOW: i64 = 1_800_000_000_000;
fn apply(
    f: &Fixture,
    mode: &ChatMuteMode,
    expected: Option<&str>,
    at: Option<i64>,
) -> Result<(), CoreError> {
    f.client
        .lock()
        .set_chat_mute_at(&f.space, mode, expected, &f.identity, &f.device, at)
}
fn at(f: &Fixture, time: i64) -> SpaceSummary {
    f.client.lock().space_summary_at(&f.space, time).unwrap()
}

#[test]
fn anchored_duration_retry_cannot_extend_and_expiry_uses_only_core_time(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new();
    let end = NOW + 3_600_000;
    let mode = ChatMuteMode::For {
        duration_seconds: 3600,
        from_ms: NOW.to_string(),
    };
    apply(&f, &mode, None, Some(NOW))?;
    f.client
        .lock()
        .set_chat_layout_prefs(&f.space, true, true)?;
    let before = f.counts();
    apply(&f, &mode, None, Some(NOW + 900_000))?;
    assert_eq!(f.counts(), before);
    assert_eq!(at(&f, NOW).muted_until_ms, Some(end));
    assert!(at(&f, end - 1).muted);
    let expired = at(&f, end);
    assert!(!expired.muted);
    assert_eq!(expired.muted_until_ms, None);
    assert!(expired.pinned && expired.archived);
    assert_eq!(
        f.counts(),
        before,
        "expiry is projection, not a cleanup event"
    );
    apply(&f, &mode, None, Some(end + 1))?;
    assert_eq!(f.counts(), before, "delayed ACK retry cannot restart mute");
    assert!(!at(&f, end + 1).muted);
    let next = ChatMuteMode::For {
        duration_seconds: 172800,
        from_ms: (end + 1).to_string(),
    };
    apply(&f, &next, None, Some(end + 1))?;
    assert_eq!(at(&f, end + 1).muted_until_ms, Some(end + 1 + 172_800_000));
    Ok(())
}

#[test]
fn custom_until_stale_comparison_invalid_input_and_unavailable_clock_never_write(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new();
    let before = f.counts();
    for mode in [
        ChatMuteMode::Until {
            until_ms: NOW.to_string(),
        },
        ChatMuteMode::Until {
            until_ms: "01".into(),
        },
        ChatMuteMode::Until {
            until_ms: "-1".into(),
        },
        ChatMuteMode::Until {
            until_ms: i64::MAX.to_string(),
        },
        ChatMuteMode::For {
            duration_seconds: 0,
            from_ms: NOW.to_string(),
        },
        ChatMuteMode::For {
            duration_seconds: 1,
            from_ms: (NOW + 1).to_string(),
        },
        ChatMuteMode::For {
            duration_seconds: u32::MAX,
            from_ms: i64::MAX.to_string(),
        },
    ] {
        assert!(apply(&f, &mode, None, Some(NOW)).is_err());
        assert_eq!(f.counts(), before);
    }
    let finite = ChatMuteMode::Until {
        until_ms: (NOW + 90_000).to_string(),
    };
    assert!(apply(&f, &finite, None, None).is_err());
    assert_eq!(f.counts(), before);
    apply(&f, &finite, None, Some(NOW))?;
    let before = f.counts();
    assert!(matches!(
        apply(&f, &ChatMuteMode::Forever, None, Some(NOW)),
        Err(CoreError::Stale { .. })
    ));
    assert_eq!(f.counts(), before);
    apply(
        &f,
        &ChatMuteMode::Off,
        Some(&(NOW + 90_000).to_string()),
        None,
    )?;
    assert_eq!(at(&f, NOW).muted_until_ms, None);
    apply(&f, &ChatMuteMode::Forever, None, None)?;
    assert_eq!(at(&f, NOW).muted_until_ms, Some(i64::MAX));
    Ok(())
}

#[test]
fn finite_deadline_and_exact_retry_survive_actual_cold_unlock_without_second_authority(
) -> Result<(), Box<dyn std::error::Error>> {
    let dir = std::env::temp_dir().join(types::new_id("timed-mute")?);
    std::fs::create_dir(&dir)?;
    let path = dir.join("account.sqlite").to_string_lossy().into_owned();
    let f = Fixture::at(path.clone());
    let until = NOW + 172_800_000;
    let mode = ChatMuteMode::Until {
        until_ms: until.to_string(),
    };
    apply(&f, &mode, None, Some(NOW))?;
    let scope = (f.identity.clone(), f.device.clone(), f.space.clone());
    let vault = f.vault.clone();
    let before = f.counts();
    drop(f);
    let client = Client::open(path, "en".into())?;
    assert!(client.unlock(vault)?);
    {
        let mut e = client.lock();
        assert!(e.space_summary_at(&scope.2, until - 1)?.muted);
        assert!(!e.space_summary_at(&scope.2, until)?.muted);
        e.set_chat_mute_at(&scope.2, &mode, None, &scope.0, &scope.1, Some(until + 1))?;
        assert_eq!(e.store.event_count()?, before.0);
        assert_eq!(e.store.outbox()?.len(), before.1);
        assert_eq!(
            e.state
                .chat_prefs
                .get(&scope.2)
                .ok_or("prefs")?
                .muted_until_ms,
            Some(until)
        );
    }
    drop(client);
    std::fs::remove_dir_all(dir)?;
    Ok(())
}

#[tokio::test]
async fn actual_snapshot_clock_command_layout_merge_and_legacy_boolean_contract(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new();
    let snapshot: serde_json::Value = serde_json::from_str(
        &f.client
            .client_request(json!({"op":"snapshot"}).to_string())
            .await?,
    )?;
    let anchor = snapshot["clock_ms"].as_str().ok_or("Core clock")?;
    let end = anchor.parse::<i64>()? + 3_600_000;
    let cmd = json!({"op":"set_chat_mute","space_id":f.space,"mode":{"kind":"for","duration_seconds":3600,"from_ms":anchor},"expected_muted_until_ms":null,"expected_identity_id":f.identity,"expected_device_id":f.device});
    assert_eq!(f.client.client_request(cmd.to_string()).await?, "null");
    assert_eq!(f.summary().muted_until_ms, Some(end));
    let before = f.counts();
    assert_eq!(f.client.client_request(cmd.to_string()).await?, "null");
    assert_eq!(f.counts(), before);
    let layout = json!({"op":"set_chat_prefs","space_id":f.space,"pinned":true,"archived":true,"muted":false,"preserve_mute":true});
    f.client.client_request(layout.to_string()).await?;
    let current = f.summary();
    assert!(current.pinned && current.archived && current.muted);
    assert_eq!(current.muted_until_ms, Some(end));
    let mut legacy = layout;
    legacy
        .as_object_mut()
        .ok_or("layout")?
        .remove("preserve_mute");
    legacy["muted"] = true.into();
    f.client.client_request(legacy.to_string()).await?;
    assert_eq!(f.summary().muted_until_ms, Some(i64::MAX));
    f.mute(false).await?;
    assert!(!f.summary().muted);
    Ok(())
}

#[test]
fn wrong_owner_device_and_removed_membership_cannot_apply_timed_intent(
) -> Result<(), Box<dyn std::error::Error>> {
    let f = Fixture::new();
    let mode = ChatMuteMode::Forever;
    let before = f.counts();
    for (identity, device) in [
        ("foreign", f.device.as_str()),
        (f.identity.as_str(), "foreign"),
    ] {
        assert!(f
            .client
            .lock()
            .set_chat_mute_at(&f.space, &mode, None, identity, device, Some(NOW))
            .is_err());
        assert_eq!(f.counts(), before);
    }
    f.client.leave_space(f.space.clone())?;
    let before = f.counts();
    assert!(apply(&f, &mode, None, Some(NOW)).is_err());
    assert_eq!(f.counts(), before);
    Ok(())
}

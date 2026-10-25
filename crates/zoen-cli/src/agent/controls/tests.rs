use super::validate_args;

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).to_owned()).collect()
}

#[test]
fn controls_command_accepts_only_explicit_existing_host_options() {
    for values in [
        vec!["agent-controls"],
        vec!["agent-controls", "--for", "1"],
        vec![
            "--home",
            "/owned home",
            "agent-controls",
            "--timeout",
            "15000",
            "--for",
            "60",
        ],
    ] {
        assert!(validate_args(&args(&values)).is_ok(), "{values:?}");
    }
}

#[test]
fn controls_command_rejects_content_modes_offline_and_authority_changes() {
    for values in [
        vec![],
        vec!["agent-host"],
        vec!["agent-controls", "agent-controls"],
        vec!["agent-controls", "agent-host"],
        vec!["agent-controls", "--source", "original"],
        vec!["agent-controls", "--source=original"],
        vec!["agent-controls", "--offline"],
        vec!["agent-controls", "--active"],
        vec!["agent-controls", "--paused"],
        vec!["agent-controls", "--resume"],
        vec!["agent-controls", "--relay", "http://different.test"],
        vec!["agent-controls", "--definition", "/another/profile.json"],
        vec!["agent-controls", "--unknown", "value"],
        vec!["agent-controls", "--timeout=1000"],
    ] {
        assert!(validate_args(&args(&values)).is_err(), "{values:?}");
    }
}

#[test]
fn controls_command_refuses_missing_duplicate_and_unrepresentable_bounds() {
    for values in [
        vec!["agent-controls", "--home"],
        vec!["agent-controls", "--home", ""],
        vec!["agent-controls", "--home", "--for", "1"],
        vec!["agent-controls", "--home", "/one", "--home", "/two"],
        vec!["agent-controls", "--for"],
        vec!["agent-controls", "--for", "0"],
        vec!["agent-controls", "--for", "0.5"],
        vec!["agent-controls", "--for", "-1"],
        vec!["agent-controls", "--for", "NaN"],
        vec!["agent-controls", "--for", "18446744073709551615"],
        vec!["agent-controls", "--for", "1", "--for", "2"],
        vec!["agent-controls", "--timeout"],
        vec!["agent-controls", "--timeout", "0"],
        vec!["agent-controls", "--timeout", "1e3"],
        vec!["agent-controls", "--timeout", "18446744073709551616"],
        vec!["agent-controls", "--timeout", "100", "--timeout", "200"],
    ] {
        assert!(validate_args(&args(&values)).is_err(), "{values:?}");
    }
}

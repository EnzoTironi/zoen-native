use super::{parse, Args};

fn flags(extra: &[&str]) -> anyhow::Result<Args> {
    let mut values = vec![
        "--relay",
        "http://127.0.0.1:1",
        "--users",
        "2",
        "--group",
        "2",
    ];
    values.extend_from_slice(extra);
    parse(&values.into_iter().map(str::to_owned).collect::<Vec<_>>())
}

#[test]
fn numeric_inputs_that_cannot_describe_a_load_are_refused_before_connecting() {
    let cases = [
        ("--users", "2.5"),
        ("--users", "NaN"),
        ("--users", "inf"),
        ("--group", "2.5"),
        ("--connect-concurrency", "0"),
        ("--connect-concurrency", "0.5"),
        ("--connect-concurrency", "NaN"),
        ("--rate", "0"),
        ("--rate", "-1"),
        ("--rate", "NaN"),
        ("--rate", "inf"),
        ("--rate", "-inf"),
        ("--rate", "1e308"),
        ("--seconds", "0"),
        ("--seconds", "-1"),
        ("--seconds", "NaN"),
        ("--seconds", "inf"),
        ("--seconds", "1e308"),
        ("--seconds", "0.0000001"),
        ("--warmup", "-1"),
        ("--warmup", "NaN"),
        ("--warmup", "inf"),
    ];
    let accepted: Vec<_> = cases
        .into_iter()
        .filter(|(flag, value)| flags(&[flag, value]).is_ok())
        .collect();
    assert!(
        accepted.is_empty(),
        "unsafe inputs accepted by the actual parser: {accepted:?}"
    );
}

#[test]
fn ordinary_runs_keep_fractional_rates_and_durations_and_integer_population() {
    let values = flags(&[
        "--rate",
        "2.5",
        "--seconds",
        "0.25",
        "--warmup",
        "0",
        "--connect-concurrency",
        "1",
    ])
    .unwrap();
    assert_eq!(values.users, 2);
    assert_eq!(values.group, 2);
    assert_eq!(values.connect_concurrency, 1);
    assert_eq!(values.rate.to_bits(), 2.5_f64.to_bits());
    assert_eq!(values.seconds.to_bits(), 0.25_f64.to_bits());
    assert_eq!(values.warmup.to_bits(), 0.0_f64.to_bits());
    assert_eq!(values.relays, ["http://127.0.0.1:1"]);
}

#[test]
fn defaults_duplicates_and_microsecond_windows_keep_the_requested_run() {
    let defaults = flags(&[]).unwrap();
    assert_eq!(defaults.measure_from_us, 5_000_000);
    assert_eq!(defaults.total_us, 25_000_000);
    let values = flags(&[
        "--users",
        "4",
        "--rate",
        "2.5",
        "--warmup",
        "0.125",
        "--seconds",
        "0.25",
        "--relay",
        "https://second.invalid",
    ])
    .unwrap();
    assert_eq!(values.users, 4);
    assert_eq!(values.measure_from_us, 125_000);
    assert_eq!(values.total_us, 375_000);
    assert_eq!(
        values.relays,
        ["http://127.0.0.1:1", "https://second.invalid"]
    );
}

#[test]
fn dangling_numeric_flags_and_population_overflow_are_refused() {
    for flag in [
        "--users",
        "--group",
        "--rate",
        "--seconds",
        "--warmup",
        "--connect-concurrency",
    ] {
        assert!(flags(&[flag]).is_err(), "{flag}");
    }
    assert!(flags(&["--users", "184467440737095516160"]).is_err());
    assert!(flags(&["--connect-concurrency", "-1"]).is_err());
}

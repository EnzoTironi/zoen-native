//! Load-run inputs, validated before the first connection.
use anyhow::{bail, Context};
use std::time::{Duration, Instant};

// The scheduler converts elapsed microseconds and message indices to f64.
// Keep their consecutive exact f64 range, with headroom for the next-message +1.
const MAX_EXACT_INTEGER: u64 = (1_u64 << 53) - 1;
const EXCLUSIVE_STEP_LIMIT: f64 = 9_007_199_254_740_992.0;

pub(super) struct Args {
    pub(super) relays: Vec<String>,
    pub(super) users: usize,
    pub(super) group: usize,
    pub(super) rate: f64,
    pub(super) seconds: f64,
    pub(super) warmup: f64,
    pub(super) connect_concurrency: usize,
    pub(super) pids: Vec<(String, u32)>,
    pub(super) label: String,
    pub(super) json: Option<String>,
    pub(super) receipts: bool,
    pub(super) measure_from_us: u64,
    pub(super) total_us: u64,
}

pub(super) fn args() -> anyhow::Result<Args> {
    let raw: Vec<String> = std::env::args().skip(1).collect();
    parse(&raw)
}

fn parse(raw: &[String]) -> anyhow::Result<Args> {
    let numeric = [
        "--users",
        "--group",
        "--rate",
        "--seconds",
        "--warmup",
        "--connect-concurrency",
    ];
    if let Some(flag) = raw.last().filter(|flag| numeric.contains(&flag.as_str())) {
        bail!("{flag} requires a value");
    }
    let all = |name: &str| -> Vec<String> {
        raw.windows(2)
            .filter(|w| w[0] == name)
            .map(|w| w[1].clone())
            .collect()
    };
    let one = |name: &str| all(name).pop();
    let num = |name: &str, default: f64| -> anyhow::Result<f64> {
        one(name).map_or(Ok(default), |v| {
            v.parse().with_context(|| format!("{name} {v}"))
        })
    };
    let count = |name: &str, default: usize| -> anyhow::Result<usize> {
        one(name).map_or(Ok(default), |value| {
            value
                .parse()
                .with_context(|| format!("{name} must be an unsigned integer: {value}"))
        })
    };
    let relays = all("--relay");
    if relays.is_empty() {
        bail!(
            "usage: zoen-load --relay URL [--relay URL…] --users N --group G --rate R --seconds S"
        );
    }
    let pids = all("--pid")
        .iter()
        .map(|p| {
            let (n, v) = p.split_once('=').context("--pid name=pid")?;
            Ok((n.to_string(), v.parse()?))
        })
        .collect::<anyhow::Result<_>>()?;
    let users = count("--users", 100)?;
    let group = count("--group", 8)?;
    let connect_concurrency = count("--connect-concurrency", 256)?;
    if group < 2 || users < group {
        bail!("--group must be at least 2 and no more than --users");
    }
    if connect_concurrency == 0 {
        bail!("--connect-concurrency must be positive");
    }
    let rate = num("--rate", 100.0)?;
    let seconds = num("--seconds", 20.0)?;
    let warmup = num("--warmup", 5.0)?;
    let window = Window::new(rate, seconds, warmup)?;
    Ok(Args {
        relays,
        users,
        group,
        rate,
        seconds,
        warmup,
        connect_concurrency,
        pids,
        label: one("--label").unwrap_or_else(|| "run".into()),
        json: one("--json"),
        receipts: raw.iter().any(|arg| arg == "--receipts"),
        measure_from_us: window.measure_from_us,
        total_us: window.total_us,
    })
}

struct Window {
    measure_from_us: u64,
    total_us: u64,
}

impl Window {
    fn new(rate: f64, seconds: f64, warmup_seconds: f64) -> anyhow::Result<Self> {
        if !rate.is_finite() || rate <= 0.0 {
            bail!("--rate must be finite and positive");
        }
        if !seconds.is_finite() || seconds <= 0.0 {
            bail!("--seconds must be finite and positive");
        }
        if !warmup_seconds.is_finite() || warmup_seconds < 0.0 {
            bail!("--warmup must be finite and nonnegative");
        }
        let warmup = Duration::try_from_secs_f64(warmup_seconds)
            .context("--warmup is outside the clock range")?;
        let measured =
            Duration::try_from_secs_f64(seconds).context("--seconds is outside the clock range")?;
        let total = warmup
            .checked_add(measured)
            .context("load window duration overflow")?;
        let measure_from_us =
            u64::try_from(warmup.as_micros()).context("--warmup exceeds the microsecond range")?;
        let total_us = u64::try_from(total.as_micros())
            .context("load window exceeds the microsecond range")?;
        if total_us > MAX_EXACT_INTEGER {
            bail!("load window exceeds the exact scheduler range");
        }
        if total_us <= measure_from_us {
            bail!("--seconds must provide at least one measured microsecond");
        }
        let scheduled = total.as_secs_f64() * rate;
        if !scheduled.is_finite() || scheduled >= EXCLUSIVE_STEP_LIMIT {
            bail!("load message count exceeds the exact scheduler range");
        }
        Instant::now()
            .checked_add(Duration::from_millis(500))
            .and_then(|start| start.checked_add(total))
            .context("load deadline is outside the platform clock range")?;
        Ok(Self {
            measure_from_us,
            total_us,
        })
    }
}

#[cfg(test)]
mod tests;

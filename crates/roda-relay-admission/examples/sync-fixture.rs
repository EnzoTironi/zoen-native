//! Exact native protobuf fixtures for the portable catch-up visibility boundary.
use base64::{engine::general_purpose::STANDARD, Engine};
use roda_proto::{ClientFrame, Cursor};
use roda_relay_admission::sync::plan_json;
use serde_json::{json, Value};

fn main() {
    let context = json!({
        "session":{"authenticated_identity":"11".repeat(32),"authenticated_device":"22".repeat(32)},
        "facts":{"identity":"11".repeat(32),"current_spaces":["current","new"],
            "removed_at":{"removed":"8","rejoined":"2","max":u64::MAX.to_string()}}
    });
    let mut steps = Vec::new();
    for (label, requested, all) in [
        ("current", vec![("current", 3)], false),
        ("removed-before", vec![("removed", 7)], false),
        ("removal-boundary", vec![("removed", 8)], false),
        ("removed-after", vec![("removed", 9)], false),
        ("unknown", vec![("unknown", 0)], false),
        (
            "all",
            vec![("removed", 8), ("current", 3), ("current", 1)],
            true,
        ),
        ("all-empty", vec![], true),
        (
            "exact-u64",
            vec![("current", 9_007_199_254_740_993), ("max", u64::MAX)],
            false,
        ),
    ] {
        let frame = ClientFrame::Sync {
            cursors: requested
                .into_iter()
                .map(|(space, next_seq)| Cursor {
                    space: space.into(),
                    next_seq,
                })
                .collect(),
            all,
        }
        .encode();
        let expected: Value =
            serde_json::from_str(&plan_json(&frame, &context.to_string()).unwrap()).unwrap();
        steps.push(json!({"label":label,"frame_base64":STANDARD.encode(frame),"context":context,"expected":expected}));
    }
    println!(
        "{}",
        json!({"schema":1,"scope":"Native protobuf and shared catch-up visibility; no public session, membership storage, log read or delivery acceptance.","steps":steps})
    );
}

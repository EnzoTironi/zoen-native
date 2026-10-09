//! Journeys for the WASM tier (ADR 0028 T0, ADR 0013): the default home for a tool that
//! needs code but not a computer. Ana's agent counts words in her notes in-process, in
//! milliseconds, with no sandbox to start; a runaway tool, a memory hog and a snoop are all
//! stopped; and nobody can swap a tool's code under its signed manifest.
//!
//! Needs the `wasm32-wasip2` target (`rustup target add wasm32-wasip2`): the journey builds
//! its tools from `tests/wasm-tools`.

use bytes::Bytes;
use ed25519_dalek::SigningKey;
use sha2::{Digest, Sha256};
use std::path::PathBuf;
use std::process::Command;
use std::time::Duration;
use zoen_agentd::wasm::{Stopped, WasmError, WasmLimits, WasmTier};
use zoen_agentd::{Needs, Route, Router, Tier, ToolManifest};

const NOW: i64 = 1_791_000_000_000;

fn publisher() -> SigningKey {
    SigningKey::from_bytes(&[42; 32])
}

/// Builds the journey's tools once per run and returns one component's bytes.
fn tool_bytes(name: &str) -> Vec<u8> {
    static BUILT: std::sync::Once = std::sync::Once::new();
    let src = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/wasm-tools");
    let out = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("wasm-tools");
    BUILT.call_once(|| {
        let ok = Command::new(env!("CARGO"))
            .args(["build", "-q", "--release", "--target", "wasm32-wasip2"])
            .arg("--manifest-path")
            .arg(src.join("Cargo.toml"))
            .env("CARGO_TARGET_DIR", &out)
            .status()
            .expect("cargo")
            .success();
        assert!(
            ok,
            "building the WASM tools failed (rustup target add wasm32-wasip2)"
        );
    });
    std::fs::read(out.join(format!("wasm32-wasip2/release/{name}.wasm"))).unwrap()
}

fn manifest(id: &str, summary: &str, bytes: &[u8]) -> ToolManifest {
    ToolManifest {
        id: id.into(),
        version: "1".into(),
        summary: summary.into(),
        needs: Needs::Wasm,
        egress: vec![],
        secrets: vec![],
        component_sha256: Some(hex::encode(Sha256::digest(bytes))),
    }
}

fn tier() -> WasmTier {
    WasmTier::new(vec![publisher().verifying_key()], 16, 64 * 1024 * 1024).unwrap()
}

#[tokio::test]
async fn anas_agent_counts_words_in_process_with_no_sandbox_and_no_card() {
    let bytes = tool_bytes("word_count");
    let signed = manifest("word_count", "Contar palavras das notas", &bytes).sign(&publisher());

    // The router sends a WASM tool straight to T0: no approval, no sandbox to start.
    let router = Router::new(vec![publisher().verifying_key()]);
    assert_eq!(
        router.route("ana", "ana-agent", &signed, &[], NOW),
        Route::Run {
            tier: Tier::Wasm,
            spec: None
        }
    );

    let wasm = tier();
    let tool = wasm.load(&signed, &bytes).unwrap();
    let notes = "Trilha sábado 7h\nlevar água e protetor\nvoltar antes do almoço\n";
    let out = wasm
        .run(&tool, Bytes::from(notes), WasmLimits::default())
        .await
        .unwrap();
    assert_eq!(
        out.stopped,
        None,
        "{:?}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout_str().trim(), r#"{"words":11,"lines":3}"#);
    // Warm, it's an in-process call: well under the microVM's start time.
    let warm = wasm
        .run(&tool, Bytes::from("um dois três"), WasmLimits::default())
        .await
        .unwrap();
    assert_eq!(warm.stdout_str().trim(), r#"{"words":3,"lines":1}"#);
    assert!(
        warm.elapsed < Duration::from_millis(250),
        "warm WASM run took {:?}",
        warm.elapsed
    );
    eprintln!(
        "word_count: first {:?}, warm {:?}, fuel {}",
        out.elapsed, warm.elapsed, warm.fuel_used
    );
}

#[tokio::test]
async fn a_runaway_tool_a_memory_hog_and_a_snoop_are_all_stopped() {
    let wasm = tier();
    let load = |name: &str| {
        let bytes = tool_bytes(name);
        wasm.load(&manifest(name, name, &bytes).sign(&publisher()), &bytes)
            .unwrap()
    };

    // Never finishes: fuel stops it.
    let spin = load("spin");
    let out = wasm
        .run(
            &spin,
            Bytes::new(),
            WasmLimits {
                fuel: 50_000_000,
                wall: Duration::from_secs(30),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(out.stopped, Some(Stopped::OutOfFuel));

    // Plenty of fuel, but the wall clock stops it.
    let out = wasm
        .run(
            &spin,
            Bytes::new(),
            WasmLimits {
                fuel: u64::MAX / 2,
                wall: Duration::from_millis(300),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(out.stopped, Some(Stopped::OutOfTime));
    assert!(out.elapsed < Duration::from_secs(3), "{:?}", out.elapsed);

    // Grabs memory: the cap stops it at 32 MiB.
    let hog = load("hog");
    let out = wasm
        .run(
            &hog,
            Bytes::new(),
            WasmLimits {
                memory_bytes: 32 * 1024 * 1024,
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(out.stopped, Some(Stopped::OutOfMemory));
    let got = out.stdout_str();
    assert!(!got.contains("40 MiB"), "{got}");

    // Files, directories, environment and network: none of it exists for a tool.
    let snoop = load("snoop");
    let out = wasm
        .run(&snoop, Bytes::new(), WasmLimits::default())
        .await
        .unwrap();
    assert_eq!(out.stopped, None);
    assert_eq!(
        out.stdout_str().trim(),
        r#"{"file":false,"dir":false,"env":0,"net":false}"#
    );
}

#[tokio::test]
async fn a_tool_sleeping_in_a_wasi_host_call_still_has_a_wall_deadline() {
    let wasm = tier();
    let bytes = tool_bytes("sleep");
    let signed = manifest("sleep", "sleep", &bytes).sign(&publisher());
    let tool = wasm.load(&signed, &bytes).unwrap();
    let out = tokio::time::timeout(
        Duration::from_secs(2),
        wasm.run(
            &tool,
            Bytes::new(),
            WasmLimits {
                wall: Duration::from_millis(250),
                ..Default::default()
            },
        ),
    )
    .await
    .expect("the host call outlived the tool's deadline")
    .unwrap();
    assert_eq!(out.stopped, Some(Stopped::OutOfTime));
    assert!(out.stdout_str().contains("sleeping"));
    assert!(!out.stdout_str().contains("awake"));
    assert!(out.elapsed < Duration::from_secs(2));
}

#[tokio::test]
async fn nobody_can_swap_a_tools_code_under_its_signed_manifest() {
    let wasm = tier();
    let good = tool_bytes("word_count");
    let evil = tool_bytes("snoop");
    let signed = manifest("word_count", "Contar palavras", &good).sign(&publisher());
    assert!(matches!(
        wasm.load(&signed, &evil),
        Err(WasmError::WrongComponent)
    ));
    // Someone else's signature over the swap isn't trusted.
    let mallory = SigningKey::from_bytes(&[7; 32]);
    let forged = manifest("word_count", "Contar palavras", &evil).sign(&mallory);
    assert!(matches!(
        wasm.load(&forged, &evil),
        Err(WasmError::Manifest(_))
    ));
    // A tool that declares a microVM doesn't run here at all.
    let mut vm = manifest("word_count", "Contar palavras", &good);
    vm.needs = Needs::MicroVm {
        vcpu: 2,
        mem_mib: 512,
        disk_mib: 1024,
        max_secs: 60,
    };
    assert!(matches!(
        wasm.load(&vm.sign(&publisher()), &good),
        Err(WasmError::NotWasm)
    ));
}

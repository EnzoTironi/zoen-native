//! Actual closed PDF frames in the signed WASI tier; supplied Scope is synthetic.
use super::{call, require, sha, Result, WasmTier, WasmTool};
use serde_json::{json, Value};
use std::path::Path;
pub(super) async fn check(
    tier: &WasmTier,
    tool: &WasmTool,
    historical: &[u8],
    cases: &mut Vec<Value>,
    output: &Path,
) -> Result<()> {
    let scope = json!({"artifactId":"synthetic-wordcraft-component","actorId":"synthetic-owner","actorKind":"agent"});
    let old = json!({"version":"2","sha256":sha(historical)});
    let mutation = json!({"mutation":{"scope":scope,"base":old,"mutationId":"pdf-next"},"action":{"command":"text.replace","params":{"text":"PDF current accepted"}}});
    let (proposal, current) = call(
        tier,
        tool,
        &json!({"schema":1,"open":{"scope":scope,"head":old},"command":mutation}),
        historical,
        cases,
        "pdf-next-document",
    )
    .await?;
    require(
        proposal["ok"] == true
            && !current.is_empty()
            && proposal["metadata"]["sha256"] == sha(&current),
        "PDF source proposal failed",
    )?;
    let latest = json!({"version":"3","sha256":sha(&current)});
    for (name, head, bytes) in [
        ("current", &latest, current.as_slice()),
        ("historical", &old, historical),
    ] {
        let request = json!({"schema":1,"open":{"scope":scope,"head":head},"export":{"scope":scope,"head":head,"format":"pdf"}});
        let (reply, pdf) = call(tier, tool, &request, bytes, cases, &format!("pdf-{name}")).await?;
        let m = &reply["metadata"];
        require(
            reply["ok"] == true
                && pdf.starts_with(b"%PDF-")
                && m["sourceHead"] == *head
                && m["format"] == "pdf"
                && m["mime"] == "application/pdf"
                && m["sha256"] == sha(&pdf)
                && m["byteLength"] == pdf.len(),
            "PDF bytes/source metadata mismatch",
        )?;
        std::fs::write(output.join(format!("{name}.pdf")), &pdf)
            .map_err(|_| "PDF output unavailable")?;
    }
    let export = json!({"scope":scope,"head":old,"format":"pdf"});
    let open = json!({"scope":scope,"head":old});
    for (name, request) in [
        (
            "pdf-foreign",
            json!({"schema":1,"open":open,"export":{"scope":{"artifactId":"synthetic-wordcraft-component","actorId":"foreign","actorKind":"agent"},"head":old,"format":"pdf"}}),
        ),
        (
            "pdf-wrong-head",
            json!({"schema":1,"open":open,"export":{"scope":scope,"head":latest,"format":"pdf"}}),
        ),
        (
            "pdf-ambiguous",
            json!({"schema":1,"open":open,"export":export,"command":mutation}),
        ),
        (
            "pdf-unknown-format",
            json!({"schema":1,"open":open,"export":{"scope":scope,"head":old,"format":"html"}}),
        ),
    ] {
        let (reply, bytes) = call(tier, tool, &request, historical, cases, name).await?;
        require(
            reply["ok"] == false && bytes.is_empty(),
            "PDF negative escaped closed worker",
        )?;
    }
    Ok(())
}

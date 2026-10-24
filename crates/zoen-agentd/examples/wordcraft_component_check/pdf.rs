//! Actual closed PDF frames in the signed WASI tier; supplied Scope is synthetic.
use super::{Result, WasmTier, WasmTool, call, require, sha};
use serde_json::{Value, json};
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
    let request = json!({"schema":1,"open":{"scope":scope,"head":old},"export":{"scope":scope,"head":old,"format":"pdf"}});
    let (unsupported, body) = call(
        tier,
        tool,
        &request,
        historical,
        cases,
        "pdf-missing-font-refusal",
    )
    .await?;
    require(
        unsupported["ok"] == false
            && unsupported["error"] == "pdf_missing_glyph"
            && body.is_empty(),
        "Missing PDF glyph must refuse, not replace Unicode with boxes",
    )?;
    let mutation = json!({"mutation":{"scope":scope,"base":old,"mutationId":"pdf-supported-history"},"action":{"command":"text.replace","params":{"text":"PDF historical Olá 日本語"}}});
    let (supported, historical) = call(
        tier,
        tool,
        &json!({"schema":1,"open":{"scope":scope,"head":old},"command":mutation}),
        historical,
        cases,
        "pdf-supported-historical-source",
    )
    .await?;
    require(
        supported["ok"] == true && supported["metadata"]["sha256"] == sha(&historical),
        "Supported PDF history source failed",
    )?;
    std::fs::write(output.join("historical-source.docx"), &historical)
        .map_err(|_| "PDF source output unavailable")?;
    let old = json!({"version":"3","sha256":sha(&historical)});
    let open = json!({"scope":scope,"head":old});
    let mutation = json!({"mutation":{"scope":scope,"base":old,"mutationId":"pdf-next"},"action":{"command":"text.replace","params":{"text":"PDF current accepted"}}});
    let (proposal, current) = call(
        tier,
        tool,
        &json!({"schema":1,"open":{"scope":scope,"head":old},"command":mutation}),
        &historical,
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
    let latest = json!({"version":"4","sha256":sha(&current)});
    for (name, head, bytes) in [
        ("current", &latest, current.as_slice()),
        ("historical", &old, historical.as_slice()),
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
    negatives(tier, tool, &open, &latest, &mutation, &historical, cases).await?;
    Ok(())
}

async fn negatives(
    tier: &WasmTier,
    tool: &WasmTool,
    open: &Value,
    latest: &Value,
    mutation: &Value,
    historical: &[u8],
    cases: &mut Vec<Value>,
) -> Result<()> {
    let scope = &open["scope"];
    let old = &open["head"];
    let export = json!({"scope":scope,"head":old,"format":"pdf"});
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

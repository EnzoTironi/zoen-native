mod checkpoint;
mod content;
mod fixture;
mod render;
#[cfg(feature = "ui")]
mod ui;

use fixture::Result;
use serde_json::json;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf, time::Instant};

fn main() -> Result<()> {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("usage: inspection NEW_OUTPUT_DIRECTORY")?,
    );
    fs::create_dir(&out)?;
    let started = Instant::now();
    let original = fixture::source()?;
    fixture::unknown_data(original.clone())?;
    fs::write(out.join("source.pdf"), original.as_slice())?;
    let (session, id) = fixture::open(original.clone())?;
    let doc = session.get(id).ok_or("missing_original_document")?;
    if doc.info.pages.len() != 2 || doc.read_only_reason.is_some() || !doc.info.warnings.is_empty()
    {
        return Err(format!("realistic_source_refused: {:?}", doc.read_only_reason).into());
    }
    if session.save_bytes(id)? != original {
        return Err("unmodified_source_changed".into());
    }
    let baseline = render::page(original.clone(), &out.join("source-page-1.png"))?;
    let (final_bytes, checkpoint) = checkpoint::delayed_ack(original.clone())?;
    fs::write(out.join("reviewed.pdf"), final_bytes.as_slice())?;
    let final_render = render::page(final_bytes.clone(), &out.join("reviewed-page-1.png"))?;
    let paragraph = content::paragraph(original.clone(), &out)?;
    let cases = content::cases(&out)?;
    #[cfg(feature = "ui")]
    let app = ui::actual_app(original.clone(), &out)?;
    #[cfg(not(feature = "ui"))]
    let app = json!({"executed":false});
    let result = json!({"upstream":"d445410807e7eefbe5b96f9662ab963ae8dd2a3a",
        "sourceBytes":original.len(),"sourceSha256":format!("{:x}",Sha256::digest(original.as_slice())),
        "finalBytes":final_bytes.len(),"finalSha256":format!("{:x}",Sha256::digest(final_bytes.as_slice())),
        "pages":2,"unchangedSourceExact":true,"documentJavascript":false,
        "baselineRender":baseline,"reviewedRender":final_render,"checkpoint":checkpoint,
        "actualApp":app,"paragraphEdit":paragraph,"largerCases":cases,
        "elapsedMillis":started.elapsed().as_millis(),
        "browserExecuted":false,"wasiExecuted":false,"canonicalIntegrated":false,"nativeIphoneAccepted":false});
    fs::write(out.join("result.json"), serde_json::to_vec_pretty(&result)?)?;
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

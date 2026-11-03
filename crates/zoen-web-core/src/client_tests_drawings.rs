use super::*;
use miniapps::excalidraw::{EMPTY, MIME, RESOURCE};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use types::{AppDoc, ItemContent, ItemKind};

fn scene(text: &str) -> String {
    format!(
        " \n{}\n",
        json!({"type":"excalidraw","version":2,"elements":[{"id":"text_1","type":"text","x":10,"y":20,"width":100,"height":24,"text":text,"isDeleted":false}],"appState":{},"files":{},"passiveFuture":{"retained":[1,"unknown"]}})
    )
}
fn hash(text: &str) -> String {
    hex::encode(Sha256::digest(text.as_bytes()))
}
fn install(
    client: &mut Client<HostSigner>,
    space: &str,
    now: i64,
) -> std::result::Result<(String, DocumentResponse), Box<dyn std::error::Error>> {
    let response = client.app_request(&json!({"op":"install_app","space_id":space,"app_id":"excalidraw","args_json":"{\"title\":\"Canvas\"}"}).to_string(), now, "en")?;
    Ok((
        response.result["id"]
            .as_str()
            .ok_or("missing authenticated test as_str value")?
            .into(),
        response,
    ))
}
fn read(
    client: &mut Client<HostSigner>,
    item: &str,
    now: i64,
) -> std::result::Result<Value, Box<dyn std::error::Error>> {
    Ok(doc(
        client,
        &json!({"op":"app_document_read","item_id":item}),
        now,
    )?
    .result)
}
fn save_request(item: &str, base: &Value, text: &str, id: &str) -> Value {
    json!({"op":"app_document_save","item_id":item,"expected_version":base["version"],"expected_sha256":base["sha256"],"scene_json":text,"mutation_id":id})
}
fn upload_and_confirm(
    client: &mut Client<HostSigner>,
    log: &mut SpaceLog,
    now: i64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let blobs: Vec<String> = serde_json::from_str(&client.document_uploads()?)?;
    for blob in &blobs {
        assert_ne!(client.local_chunk(blob), Vec::<u8>::new());
    }
    let update = client.document_uploaded(&blobs)?;
    confirm_documents(client, log, update, now)?;
    Ok(())
}
fn pending_state(
    client: &Client<HostSigner>,
) -> std::result::Result<String, Box<dyn std::error::Error>> {
    Ok(serde_json::to_string(&client.saved)?)
}

include!("client_tests_drawings/content.rs");
include!("client_tests_drawings/activity.rs");

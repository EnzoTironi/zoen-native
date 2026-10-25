//! Existing signed plain-text-v1 items remain the same canonical artifact.
use super::*;
use std::sync::Arc;
use types::{EventBody, Grant, GrantScope};
use zoen_word_profile::model::{props::Align, Block, Document as Model};
const OLD_ENGINE: &str = "d1b0bfc3f9dd83126b162a42b667facecb003b7f";
const OLD_SHA: &str = "0a4363cf44e798c967e35b2ec7addce8697f4ab694844fba12c561fc0789310b";
fn legacy_word(f: &Fixture) -> R<AppBinaryDocumentDto> {
    let mut e = f.client.lock();
    let me = e.me_id()?;
    let item = types::new_id("it_word_legacy")?;
    // Fixture setup uses the original signed ItemCreated + encrypted FileDoc contract,
    // not a mutation of projected state or a current factory with rich metadata.
    e.atomic_agent_action(|e| {
        let key=e.file_key(None)?;
        let file=e.file_doc(&f.space,&key,"document.docx","",wordcraft::MIME,DOCX,None,&HashMap::new())?;
        e.store.set_meta(&format!("file_key:{item}"),&hex::encode(key))?;
        e.append(&f.space,&me,EventBody::ItemCreated{item:item.clone(),kind:ItemKind::App,content:ItemContent::App(AppDoc{view:None,app:wordcraft::APP.into(),resource_uri:wordcraft::RESOURCE.into(),title:"Legacy Word".into(),state_json:json!({"title":"Legacy Word","paragraphs":1,"words":0,"engineRevision":OLD_ENGINE,"profile":"plain-text-v1"}).to_string(),document:Some(file)}),origin:"Created document".into()})?;
        e.append(&f.space,&me,EventBody::GrantIssued{grant:Grant{id:types::new_id("gr")?,grantor:me.clone(),grantee:Some(format!("app:{item}")),scope:GrantScope::Item(item.clone()),capability:Capability::Trust(TrustLevel::Act),expires_at_ms:None}})?;
        Ok(())
    })?;
    e.app_binary_read(&item, Some(1), Some(OLD_SHA))
}
fn metadata(client: &Client, item: &str, version: u32) -> R<Value> {
    let e = client.lock();
    let selected = e
        .item_state(item)?
        .versions
        .iter()
        .find(|v| v.number == version)
        .ok_or_else(stale)?;
    let ItemContent::App(app) = &selected.content else {
        return Err(invalid());
    };
    serde_json::from_str(&app.state_json).map_err(|_| invalid())
}
#[test]
fn legacy_word_head_crosses_current_core_dto_and_rich_cas_without_replacing_its_item(
) -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::temp_dir().join(format!(
        "zoen-word-legacy-{}.sqlite",
        types::new_id("fixture")?
    ));
    let f = fixture(&path.to_string_lossy());
    let base = legacy_word(&f)?;
    assert_eq!(
        (base.version, base.current_version, base.byte_length),
        (1, 1, 6397)
    );
    assert_eq!(base.sha256, OLD_SHA);
    assert_eq!(
        base.engine_revision,
        wordcraft::ENGINE_REVISION,
        "DTO uses the current registry revision; the old metadata profile is not a migration gate"
    );
    assert_eq!(
        metadata(&f.client, &base.item_id, 1)?["profile"],
        "plain-text-v1"
    );
    let before = counts(&f);
    let items = f.client.lock().state.items.len();
    assert_eq!(
        f.client
            .app_binary_read(base.item_id.clone(), Some(1), Some(OLD_SHA.into()))?,
        base
    );
    assert_eq!(
        counts(&f),
        before,
        "opening the legacy head must not publish a migration/reset"
    );
    let mut model = Model::from_text("Upgraded same item");
    let Block::Para(p) = Arc::make_mut(&mut model.body[0]) else {
        return Err("missing paragraph".into());
    };
    p.props.align = Some(Align::Center);
    p.runs[0].props.bold = Some(true);
    p.runs[0].props.size = Some(18.0);
    let bytes = zoen_word_profile::encode(&model)?;
    let accepted = save(&f, &base, &bytes, "legacy-rich-edit")?;
    assert_eq!(accepted.item_id, base.item_id);
    assert_eq!(
        (accepted.version, accepted.actor_kind.as_str()),
        (2, "human")
    );
    assert_eq!(accepted.author_id, base.actor_id);
    assert_eq!(
        f.client.lock().state.items.len(),
        items,
        "no replacement item may be created"
    );
    assert_eq!(
        metadata(&f.client, &base.item_id, 1)?["profile"],
        "plain-text-v1"
    );
    assert_eq!(
        metadata(&f.client, &base.item_id, 2)?["profile"],
        "rich-document-v1"
    );
    assert_eq!(zoen_word_profile::parse(&bytes)?, model);
    let vault = f.vault.clone();
    drop(f);
    let client = Client::open(path.to_string_lossy().into(), "en".into())?;
    assert!(client.unlock(vault)?);
    let historical = client.app_binary_read(base.item_id.clone(), Some(1), Some(OLD_SHA.into()))?;
    assert_eq!(historical.data_base64, base.data_base64);
    assert_eq!(historical.current_version, 2);
    assert_eq!(
        metadata(&client, &base.item_id, 1)?["profile"],
        "plain-text-v1"
    );
    assert_eq!(client.lock().item_state(&base.item_id)?.versions.len(), 2);
    let replay = client.app_binary_save(
        base.item_id.clone(),
        1,
        OLD_SHA.into(),
        STANDARD.encode(&bytes),
        "legacy-rich-edit".into(),
        hash(b"test operation"),
    )?;
    assert_eq!(
        replay, accepted,
        "a lost ACK retries the existing native receipt, not a new item/head"
    );
    drop(client);
    std::fs::remove_file(path)?;
    Ok(())
}

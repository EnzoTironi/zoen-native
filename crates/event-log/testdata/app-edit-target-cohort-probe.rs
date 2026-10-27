//! Probe against unchanged 601c predecessor library source, not a copied decoder.
use event_log::{content::{self,Payload,SignedContent},Author,Signer,SpaceLog,event_from_content};
use types::{EventBody,Privacy,SpaceKind};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let signer=Signer::from_secret(&[143;32]);
    let author=Author::root(signer.clone());
    let mut log=SpaceLog::new("sp_target_cohort");
    log.append(&signer,1,EventBody::SpaceCreated { title:"Target cohort".into(),kind:SpaceKind::Group,privacy:Privacy::Closed })?;
    // Supply the new body's exact wire bytes to the genuine old decoder.
    let raw=serde_json::to_vec(&serde_json::json!({"AppEditRequested":{
        "message":"target_message","text":"Edit this drawing",
        "reply":{"to":"root","thread":true,"artifact":{
            "item_id":"drawing","version":1,"sha256":"ab".repeat(32),
            "request_id":"target_request","text_sha256":"cd".repeat(32),
            "agent_id":"operator","intent_sha256":"ef".repeat(32)
        }}
    }}))?;
    let content=SignedContent {v:u32::from(types::EVENT_FORMAT),space:"sp_target_cohort".into(),client_id:"target_request".into(),author:author.identity.clone(),device:None,at_ms:2,seen:log.head().as_ref().map(Into::into),payload:Some(Payload::Body(raw))}.encode();
    let signature=signer.sign(content::content_hash(&content).as_bytes());
    let old=event_from_content(content.clone(),signature,None,0,String::new(),String::new())?;
    assert_eq!(old.body,EventBody::Unsupported {kind:"AppEditRequested".into()});
    event_log::verify_author(&old)?;
    log.sequence(old)?;
    log.append(&signer,3,EventBody::MessagePosted {message:"control".into(),text:"Conversation continues".into(),attaches:None,reply:None,media:None,forwarded:false})?;
    log.verify()?;
    let persisted=serde_json::to_string(log.events())?;
    let cold=SpaceLog::from_events("sp_target_cohort",serde_json::from_str(&persisted)?)?;
    assert_eq!(cold.events()[1].content,content);
    let path=std::env::args().nth(1).ok_or("fixture output path")?;
    std::fs::write(path,persisted)?;
    println!("601c predecessor: new metadata is Unsupported; exact signed bytes survive sequencing, cold reload and the next message");
    Ok(())
}

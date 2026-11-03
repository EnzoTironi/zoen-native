use super::super::tests::{account, clear, sequence, HostSigner};
use super::*;
use crate::client::ServerFrame;
use event_log::{Author, Signer as NativeSigner};

fn client(
) -> std::result::Result<(Client<HostSigner>, NativeSigner, Author), Box<dyn std::error::Error>> {
    let identity = NativeSigner::from_secret(&[71; 32]);
    let device = NativeSigner::from_secret(&[72; 32]);
    let author = Author::device(&identity, device.clone());
    let mut account = account(&identity, &device);
    account.agreement_secret = hex::encode([73; 32]);
    account.profile_key = ProfileKey::generate()?.to_hex();
    account.profile_key_version = 1;
    account.profile_seen_version = 1;
    account.profile_fields = hex::encode(
        ProfileFields {
            name: "Antes".into(),
            bio: "bio".into(),
            photo: None,
        }
        .to_bytes(),
    );
    Ok((
        Client::new(account, HostSigner(device), "")?,
        identity,
        author,
    ))
}

fn synced(
    client: &mut Client<HostSigner>,
) -> std::result::Result<Update, Box<dyn std::error::Error>> {
    let mut first = client.receive(&ServerFrame::SyncDone.encode(), 10)?;
    if client.profile_net.profiles.is_empty() {
        return Ok(first);
    }

    let sync = client
        .saved
        .profile
        .own_sync
        .as_ref()
        .ok_or("missing authenticated test as_ref value")?;
    let records = if sync.published == 0 && sync.pending.is_some() {
        Vec::new()
    } else {
        vec![remote_profile(
            client,
            sync.published.max(1),
            "Antes",
            false,
        )?]
    };
    client.profile_reply(
        client
            .profile_net
            .profiles_request
            .ok_or("missing authenticated test profiles_request")?,
        &Ok(Reply::SealedProfiles(records)),
    )?;
    let mut next = client.update(Vec::new())?;
    first.frames.append(&mut next.frames);
    next.frames = first.frames;
    Ok(next)
}

fn request(
    client: &mut Client<HostSigner>,
    command: &Value,
) -> std::result::Result<DocumentResponse, Box<dyn std::error::Error>> {
    Ok(client.profile_request(&command.to_string(), 20)?)
}

fn remote_profile(
    client: &Client<HostSigner>,
    version: u64,
    name: &str,
    different_key: bool,
) -> std::result::Result<SealedProfile, Box<dyn std::error::Error>> {
    let identity = NativeSigner::from_secret(&[71; 32]);
    let author = Author::device(&identity, NativeSigner::from_secret(&[92; 32]));
    let key = if different_key {
        ProfileKey::generate()?
    } else {
        client.my_profile_fields()?.0
    };
    let ciphertext = profile::seal_profile(
        &key,
        &identity.id(),
        version,
        &ProfileFields {
            name: name.into(),
            ..Default::default()
        },
    )?;
    Ok(SealedProfile {
        identity: identity.id(),
        version,
        signed: profile::sign_upload(&author, version, &ciphertext)
            .ok_or("authenticated test cryptographic evidence missing")?,
        ciphertext,
    })
}

fn receive_own(
    client: &mut Client<HostSigner>,
    record: SealedProfile,
) -> std::result::Result<Update, Box<dyn std::error::Error>> {
    client.profile_net.profiles = vec![client.account.identity.clone()];
    client.profile_net.profiles_request = Some(client.next_retained_request_id()?);
    client.profile_reply(
        client
            .profile_net
            .profiles_request
            .ok_or("missing authenticated test profiles_request")?,
        &Ok(Reply::SealedProfiles(vec![record])),
    )?;
    Ok(client.update(Vec::new())?)
}

fn reply_upload(
    client: &mut Client<HostSigner>,
    version: u64,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let id = client.next_retained_request_id()?;
    client.profile_net.upload_version = Some(version);
    client.profile_net.upload_request = Some(id);
    client.profile_reply(id, &Ok(Reply::Done))?;

    Ok(())
}

fn is_profile_upload(frame: &[u8]) -> bool {
    matches!(
        ClientFrame::decode(frame).map_err(|error| error.to_string()),
        Ok(ClientFrame::Req {
            op: Op::PutProfile { .. },
            ..
        })
    )
}

mod media_cases;
mod sync_cases;
mod timeout_cases;

use super::super::tests::{account, clear, sequence, HostSigner};
use super::*;
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
        Client::new(account, HostSigner(device), "").unwrap(),
        identity,
        author,
    ))
}

fn synced(
    client: &mut Client<HostSigner>,
) -> std::result::Result<Update, Box<dyn std::error::Error>> {
    let mut first = client.receive(&ServerFrame::SyncDone.encode(), 10).unwrap();
    if !client.profile_net.profiles.is_empty() {
        let sync = client.saved.profile.own_sync.as_ref().unwrap();
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
        client
            .profile_reply(
                client.profile_net.profiles_request.unwrap(),
                &Ok(Reply::SealedProfiles(records)),
            )
            .unwrap();
        let mut next = client.update(Vec::new()).unwrap();
        first.frames.append(&mut next.frames);
        next.frames = first.frames;
        Ok(next)
    } else {
        Ok(first)
    }
}

fn request(client: &mut Client<HostSigner>, command: Value) -> DocumentResponse {
    client.profile_request(&command.to_string(), 20).unwrap()
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
        client.my_profile_fields().unwrap().0
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
        signed: profile::sign_upload(&author, version, &ciphertext).unwrap(),
        ciphertext,
    })
}

fn receive_own(client: &mut Client<HostSigner>, record: SealedProfile) -> Update {
    client.profile_net.profiles = vec![client.account.identity.clone()];
    client.profile_net.profiles_request = Some(client.next_retained_request_id().unwrap());
    client
        .profile_reply(
            client.profile_net.profiles_request.unwrap(),
            &Ok(Reply::SealedProfiles(vec![record])),
        )
        .unwrap();
    client.update(Vec::new()).unwrap()
}

fn reply_upload(client: &mut Client<HostSigner>, version: u64) {
    let id = client.next_retained_request_id().unwrap();
    client.profile_net.upload_version = Some(version);
    client.profile_net.upload_request = Some(id);
    client.profile_reply(id, &Ok(Reply::Done)).unwrap();
}

fn is_profile_upload(frame: &[u8]) -> bool {
    matches!(
        ClientFrame::decode(frame),
        Ok(ClientFrame::Req {
            op: Op::PutProfile { .. },
            ..
        })
    )
}

mod media_cases;
mod sync_cases;
mod timeout_cases;

#[tokio::test]
async fn an_agent_cannot_register_by_claiming_someone_elses_account(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("agent_owner_claim").await;
    let mut owner = RawClient::connect(&w.relay_url(), "owner").await?;
    let agent = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut claimant = RawClient::reconnect(&w.relay_url(), agent).await;
    let result = claimant
        .request(Op::Register {
            profile: profile(&claimant.author, "claimed_agent", Some(owner.identity())),
        })
        .await;
    assert!(
        result.is_err(),
        "unsigned ownership was accepted: {result:?}"
    );
    assert!(matches!(
        owner.request(Op::Lookup { handle: "claimed_agent".into(), prefix: false }).await,
        Ok(Reply::Profiles(found)) if found.is_empty()
    ));
    assert_eq!(w.count("SELECT count(*) FROM identities").await, 1);
    assert_eq!(w.count("SELECT count(*) FROM devices").await, 1);
    Ok(())
}

#[tokio::test]
async fn a_person_cannot_turn_their_profile_into_someone_elses_agent(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("agent_owner_update").await;
    let owner = RawClient::connect(&w.relay_url(), "owner").await?;
    let mut person = RawClient::connect(&w.relay_url(), "person").await?;
    let mut changed = profile(&person.author, "person", Some(owner.identity()));
    changed.owner_proof = Some(Box::new(
        agent_owner::authorize(&owner.author, &person.identity()).unwrap(),
    ));
    let result = person.request(Op::Register { profile: changed }).await;
    assert!(
        result.is_err(),
        "identity kind and owner changed: {result:?}"
    );
    let Ok(Reply::Profiles(found)) = person
        .request(Op::Lookup {
            handle: "person".into(),
            prefix: false,
        })
        .await
    else {
        panic!("person directory lookup failed")
    };
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].kind, IdentityKind::Person);
    assert_eq!(found[0].owner, None);
    Ok(())
}

#[tokio::test]
async fn a_legacy_false_agent_cache_accepts_the_real_person_and_survives_restart(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("agent_owner_cache_person").await;
    let mut owner = RawClient::connect(&w.relay_url(), "owner").await?;
    let person = RawClient::connect(&w.relay_url(), "person").await?;
    let path = w.dir.join("reader.db");
    let false_agent = profile(&person.author, "person", Some(owner.identity()));
    {
        let cache = store::Store::open(path.to_str().unwrap()).unwrap();
        cache.put_identity(&false_agent, None).unwrap();
    }
    let mut reader = client_core::Engine::open(path.to_str().unwrap()).unwrap();
    assert!(
        reader.agents().is_empty(),
        "unsigned legacy cache advertised an agent"
    );
    let correct = lookup(&mut owner, "person").await;
    assert_eq!(correct.len(), 1);
    assert_eq!(correct[0].kind, IdentityKind::Person);
    reader.put_profiles(correct.clone()).unwrap();
    assert!(reader.agents().is_empty());
    assert!(reader.personas().iter().any(|p| p.id == person.identity()));
    drop(reader);
    let reader = client_core::Engine::open(path.to_str().unwrap()).unwrap();
    assert!(reader.agents().is_empty());
    assert!(reader.personas().iter().any(|p| p.id == person.identity()));
    drop(reader);
    let cache = store::Store::open(path.to_str().unwrap()).unwrap();
    let cached = cache.identities().unwrap();
    assert_eq!(cached.len(), 1);
    assert_eq!(cached[0].0.id, correct[0].id);
    assert_eq!(cached[0].0.kind, IdentityKind::Person);
    assert_eq!(cached[0].0.owner, None);
    assert_eq!(cached[0].0.owner_proof, None);
    assert_eq!(cached[0].0.name, "@person");
    assert_eq!(cached[0].1, None);
    Ok(())
}

#[tokio::test]
async fn a_legacy_wrong_owner_cache_adopts_then_pins_the_authenticated_agent(
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let w = World::new("agent_owner_cache_agent").await;
    let mut owner = RawClient::connect(&w.relay_url(), "owner").await?;
    let other = RawClient::connect(&w.relay_url(), "other").await?;
    let author = Author::device(&Signer::generate()?, Signer::generate()?);
    let mut agent = RawClient::reconnect(&w.relay_url(), author).await;
    let correct = authorized_profile(&owner.author, &agent.author, "real_agent");
    agent
        .request(Op::Register {
            profile: correct.clone(),
        })
        .await
        .unwrap();
    let path = w.dir.join("reader.db");
    {
        let cache = store::Store::open(path.to_str().unwrap()).unwrap();
        let wrong = profile(&agent.author, "real_agent", Some(other.identity()));
        cache.put_identity(&wrong, None).unwrap();
    }
    let mut reader = client_core::Engine::open(path.to_str().unwrap()).unwrap();
    reader
        .put_profiles(lookup(&mut owner, "real_agent").await)
        .unwrap();
    assert_eq!(reader.agents().len(), 1);
    drop(reader);
    let mut reader = client_core::Engine::open(path.to_str().unwrap()).unwrap();
    let transferred = authorized_profile(&other.author, &agent.author, "real_agent");
    reader.put_profiles(vec![transferred]).unwrap();
    reader
        .put_profiles(vec![profile(&agent.author, "real_agent", None)])
        .unwrap();
    drop(reader);
    let cache = store::Store::open(path.to_str().unwrap()).unwrap();
    assert_eq!(cache.identities().unwrap(), vec![(correct, None)]);
    drop(cache);
    let reader = client_core::Engine::open(path.to_str().unwrap()).unwrap();
    assert_eq!(reader.agents().len(), 1);
    Ok(())
}

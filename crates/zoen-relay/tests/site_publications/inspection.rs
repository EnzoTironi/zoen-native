use super::*;
use protocol::sites::SignedInspection;

fn query(author: &Author, site: &str, before: Option<String>) -> SignedInspection {
    let mut request = SignedInspection {
        owner_id: author.identity.clone(),
        device_id: author.key.id(),
        at_ms: NOW,
        site_id: site.into(),
        public_origin: PUBLIC_ORIGIN.into(),
        before,
        signature: String::new(),
    };
    request.signature = author.key.sign(&request.signing_bytes(AUDIENCE).unwrap());
    request
}

pub async fn exercise(
    service: &Service,
    author: &Author,
    outsider: &Author,
    site: &str,
    restored: &Receipt,
) -> std::result::Result<(), Box<dyn std::error::Error>> {
    let read = query(author, site, None);
    let history = service.inspect(&read, NOW).await.unwrap();
    assert_eq!(history.head.as_ref(), Some(&restored.head));
    assert_eq!(history.receipts.len(), 4);
    assert_eq!(history.receipts[0], *restored);
    assert!(history.next_before.is_none());
    let foreign = query(outsider, site, None);
    assert_eq!(
        service.inspect(&foreign, NOW).await,
        Err(Error::Unauthorized)
    );
    let empty = service
        .inspect(&query(author, &types::new_secret_id("site")?, None), NOW)
        .await
        .unwrap();
    assert!(empty.head.is_none() && empty.receipts.is_empty());
    let mut alternate = request(author, site, Some(restored.head.clone()), FIRST, 1)?;
    alternate.mutation.public_origin = "https://another.example.test".into();
    sign(author, &mut alternate);
    assert_eq!(
        service.apply(&alternate, Some(FIRST), NOW).await,
        Err(Error::Invalid)
    );
    assert_eq!(
        service.inspect(&read, NOW).await.unwrap().head,
        Some(restored.head.clone())
    );
    let paginated = types::new_secret_id("site")?;
    let mut previous = None;
    for _ in 0..102 {
        let change = request(author, &paginated, previous, FIRST, 1)?;
        previous = Some(service.apply(&change, Some(FIRST), NOW).await.unwrap().head);
    }
    let first = service
        .inspect(&query(author, &paginated, None), NOW)
        .await
        .unwrap();
    assert_eq!(first.head, previous);
    assert_eq!(first.receipts.len(), 100);
    let second = service
        .inspect(&query(author, &paginated, first.next_before), NOW)
        .await
        .unwrap();
    assert_eq!(second.receipts.len(), 2);
    assert!(second.next_before.is_none());
    assert_eq!(
        first.receipts.last().unwrap().previous_head.as_ref(),
        Some(&second.receipts[0].head)
    );
    Ok(())
}

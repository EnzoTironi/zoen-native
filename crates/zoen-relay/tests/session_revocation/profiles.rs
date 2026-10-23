use super::*;

/// Actual Session authentication/signatures and PostgreSQL replacement semantics.
/// The existing BlockedLog fixture is not a canonical-history/FDB acceptance proof.
#[tokio::test]
async fn sealed_profile_retries_versions_and_linked_resigning_use_authoritative_sql() {
    let test = TestRelay::new().await;
    let result = std::panic::AssertUnwindSafe(async {
        let root = Signer::generate();
        let first = Author::device(&root, Signer::generate());
        let second = Author::device(&root, Signer::generate());
        let mut a = connect(&test.ws_url, &first).await;
        request(
            &mut a,
            1,
            Op::Register {
                profile: profile(&first, "profile_versions"),
            },
        )
        .await
        .unwrap();
        request(&mut a, 2, link_op(&second, &Signer::generate().id()))
            .await
            .unwrap();
        let mut b = connect(&test.ws_url, &second).await;
        let key = ProfileKey::generate();
        let fields = ProfileFields {
            name: "Synthetic profile".into(),
            bio: "Private".into(),
            photo: None,
        };
        let version = 9_007_199_254_740_993;
        let make = |who: &Author, version: u64, ciphertext: Vec<u8>| SealedProfile {
            identity: who.identity.clone(),
            signed: sign_upload(who, version, &ciphertext).unwrap(),
            version,
            ciphertext,
        };
        let original = make(
            &first,
            version,
            seal_profile(&key, &first.identity, version, &fields),
        );
        assert_eq!(
            request(
                &mut a,
                3,
                Op::PutProfile {
                    profile: original.clone()
                }
            )
            .await,
            Ok(Reply::Done)
        );
        assert_eq!(
            request(
                &mut a,
                4,
                Op::PutProfile {
                    profile: original.clone()
                }
            )
            .await,
            Ok(Reply::Done)
        );
        let resigned = make(&second, version, original.ciphertext.clone());
        assert_eq!(
            request(
                &mut b,
                5,
                Op::PutProfile {
                    profile: resigned.clone()
                }
            )
            .await,
            Ok(Reply::Done)
        );
        let ids = vec![first.identity.clone()];
        assert_eq!(
            zoen_relay::db::sealed_profiles(&test.state.pool, &ids)
                .await
                .unwrap(),
            vec![resigned]
        );
        for (id, p) in [
            (6, make(&first, version, vec![9])),
            (7, make(&first, version - 1, original.ciphertext.clone())),
        ] {
            assert_eq!(
                request(&mut a, id, Op::PutProfile { profile: p }).await,
                Err("stale profile version".into())
            );
        }
        let lower = make(
            &first,
            version + 1,
            seal_profile(&key, &first.identity, version + 1, &fields),
        );
        let higher = make(
            &first,
            version + 2,
            seal_profile(&key, &first.identity, version + 2, &fields),
        );
        let (left, right) = tokio::join!(
            zoen_relay::db::put_profile(&test.state.pool, &lower),
            zoen_relay::db::put_profile(&test.state.pool, &higher)
        );
        left.unwrap();
        assert!(right.unwrap());
        assert_eq!(
            zoen_relay::db::sealed_profiles(&test.state.pool, &ids)
                .await
                .unwrap(),
            vec![higher]
        );
        let maximum = make(
            &first,
            i64::MAX as u64,
            seal_profile(&key, &first.identity, i64::MAX as u64, &fields),
        );
        assert_eq!(
            request(
                &mut a,
                8,
                Op::PutProfile {
                    profile: maximum.clone()
                }
            )
            .await,
            Ok(Reply::Done)
        );
        let overflow = make(&first, u64::MAX, vec![1]);
        assert_eq!(
            request(
                &mut a,
                9,
                Op::PutProfile {
                    profile: overflow.clone()
                }
            )
            .await,
            Err("profile version out of range".into())
        );
        assert!(zoen_relay::db::put_profile(&test.state.pool, &overflow)
            .await
            .is_err());
        assert_eq!(
            zoen_relay::db::sealed_profiles(&test.state.pool, &ids)
                .await
                .unwrap(),
            vec![maximum]
        );
        let _ = a.close(None).await;
        let _ = b.close(None).await;
    })
    .catch_unwind()
    .await;
    test.close().await;
    result.unwrap();
}

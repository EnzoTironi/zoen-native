use foundationdb::{
    tuple::{pack, Subspace},
    Database, RangeOption,
};
use futures_util::TryStreamExt;
use std::{sync::Arc, time::Duration};
use zoen_relay::ownership::{self, registry::register_in, NodeOwner, MAX_NODES};

async fn count(db: &Database, nodes: &Subspace) -> usize {
    db.run(|trx, _| async move {
        Ok(trx
            .get_ranges_keyvalues(RangeOption::from(nodes.range()), true)
            .try_collect::<Vec<_>>()
            .await?
            .len())
    })
    .await
    .unwrap()
}

pub async fn run() {
    let cluster = std::env::var("FDB_CLUSTER_FILE").unwrap();
    let db = Arc::new(Database::new(Some(&cluster)).unwrap());
    let root = Subspace::all().subspace(&("zoen", roda_types::new_id("registry")));
    let nodes = root.subspace(&("ownership", "nodes"));
    db.run(|trx, _| {
        let nodes = &nodes;
        async move {
            let version = trx.get_read_version().await?;
            for i in 0..MAX_NODES {
                trx.set(&nodes.pack(&format!("expired-{i:04}")), &pack(&version));
            }
            Ok(())
        }
    })
    .await
    .unwrap();
    let owner = NodeOwner::new(
        db.clone(),
        root.clone(),
        "fresh".into(),
        Duration::from_secs(30),
    );
    owner.maintain().await.unwrap();
    assert!(owner.ready().await);
    assert_eq!(count(&db, &nodes).await, 1);
    println!("ok  a fresh node starts after exactly 4096 expired registry entries");

    let (begin, end) = root.range();
    db.run(|trx, _| {
        let (begin, end, nodes) = (&begin, &end, &nodes);
        async move {
            trx.clear_range(begin, end);
            let until = trx.get_read_version().await? + 60_000_000;
            for i in 0..MAX_NODES - 1 {
                trx.set(&nodes.pack(&format!("live-{i:04}")), &pack(&until));
            }
            Ok(())
        }
    })
    .await
    .unwrap();
    let first = db.create_trx().unwrap();
    let version = first.get_read_version().await.unwrap();
    let second = db.create_trx().unwrap();
    second.set_read_version(version);
    for (trx, node) in [(&first, "new-a"), (&second, "new-b")] {
        ownership::bound_transaction(trx).unwrap();
        assert_eq!(
            register_in(trx, &root, node, version, version + 60_000_000)
                .await
                .unwrap()
                .unwrap()
                .len(),
            MAX_NODES
        );
    }
    first.commit().await.unwrap();
    match second.commit().await {
        Err(e) => assert_eq!(
            e.code(),
            1020,
            "admissions at the same read version must conflict"
        ),
        Ok(_) => panic!("both admissions exceeded the cap"),
    }
    db.run(|trx, _| {
        let root = &root;
        async move {
            ownership::bound_transaction(&trx)?;
            let version = trx.get_read_version().await?;
            assert!(
                register_in(&trx, root, "new-b", version, version + 60_000_000)
                    .await?
                    .is_none()
            );
            Ok(())
        }
    })
    .await
    .unwrap();
    assert_eq!(count(&db, &nodes).await, MAX_NODES);
    println!("ok  concurrent admissions from 4095 live nodes cannot exceed the 4096 cap");

    // Simulate an oversized registry left by the old admission race. The first
    // page is all live; the expired tail requires the persisted cleanup cursor.
    db.run(|trx, _| {
        let nodes = &nodes;
        async move {
            let version = trx.get_read_version().await?;
            trx.set(&nodes.pack(&"live-extra"), &pack(&(version + 60_000_000)));
            for i in 0..17 {
                trx.set(&nodes.pack(&format!("zz-expired-{i:04}")), &pack(&version));
            }
            Ok(())
        }
    })
    .await
    .unwrap();
    for _ in 0..2 {
        db.run(|trx, _| {
            let root = &root;
            async move {
                let version = trx.get_read_version().await?;
                assert!(
                    register_in(&trx, root, "new-b", version, version + 60_000_000)
                        .await?
                        .is_none()
                );
                Ok(())
            }
        })
        .await
        .unwrap();
    }
    assert_eq!(
        count(&db, &nodes).await,
        MAX_NODES + 1,
        "cleanup reaches past the full live prefix"
    );
    db.run(|trx, _| {
        let nodes = &nodes;
        async move {
            let version = trx.get_read_version().await?;
            for who in ["live-extra", "live-0000"] {
                trx.set(&nodes.pack(&who), &pack(&version));
            }
            Ok(())
        }
    })
    .await
    .unwrap();
    let admitted = db
        .run(|trx, _| {
            let root = &root;
            async move {
                let version = trx.get_read_version().await?;
                register_in(&trx, root, "new-b", version, version + 60_000_000).await
            }
        })
        .await
        .unwrap();
    assert!(
        admitted.is_none(),
        "oversized cleanup must commit before retrying admission"
    );
    db.run(|trx, _| {
        let root = &root;
        async move {
            let version = trx.get_read_version().await?;
            assert_eq!(
                register_in(&trx, root, "new-b", version, version + 60_000_000)
                    .await?
                    .unwrap()
                    .len(),
                MAX_NODES
            );
            Ok(())
        }
    })
    .await
    .unwrap();
    assert_eq!(count(&db, &nodes).await, MAX_NODES);
    db.run(|trx, _| {
        let (begin, end) = (&begin, &end);
        async move {
            trx.clear_range(begin, end);
            Ok(())
        }
    })
    .await
    .unwrap();
    println!("ok  bounded cleanup reaches expired overflow entries and admission recovers");
}

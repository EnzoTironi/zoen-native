use super::*;
use relay_admission::managed_images::Checkpoint;

pub(crate) struct ImageActivationProof {
    key: Vec<u8>,
    record: Vec<u8>,
    target: Root,
    checkpoint: Checkpoint,
    pub(crate) envelope: Envelope,
}

impl RuntimeAuthority {
    pub(crate) async fn pending_image_activation(&self) -> ImageActivationProof {
        let guard = self.managed_images.as_ref().unwrap().lock().await;
        let (deployment, pending, _) = guard.activation_evidence();
        let deployment = &deployment;
        let credential = self
            .native
            .as_ref()
            .unwrap()
            .credential(&deployment.agent, &deployment.device)
            .unwrap();
        let transaction = self.execution.transaction().await.unwrap();
        let pin = self.execution.image_activation_pin(&credential.principal());
        let sealed = transaction.get(&pin, false).await.unwrap().unwrap();
        let intent: Intent = self
            .custody
            .open(&subject(deployment), None, &sealed)
            .unwrap();
        assert_eq!(pending.as_ref(), Some(&intent.prepared));
        let (sql, fdb) = self.image_checkpoints(deployment).await.unwrap();
        assert_eq!(sql, intent.prepared.next);
        assert_eq!(fdb, intent.prepared.previous);
        assert_eq!(
            self.execution
                .native_root(&transaction, &credential.principal())
                .await
                .unwrap(),
            Some(intent.previous)
        );
        let key = intent.effect.location(self);
        assert_eq!(
            transaction
                .get(&key, false)
                .await
                .unwrap()
                .as_ref()
                .map(|bytes| hash(bytes)),
            intent.prior
        );
        let record = transaction
            .get(&data_key(self, &intent.prepared, "record"), false)
            .await
            .unwrap()
            .unwrap();
        let wire = transaction
            .get(&data_key(self, &intent.prepared, "wire"), false)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(hash(&record), intent.record);
        let ClientFrame::Publish { env } = ClientFrame::decode(&wire).unwrap() else {
            panic!("not original Publish")
        };
        assert_eq!(hash(&wire), intent.wire);
        ImageActivationProof {
            key,
            record: record.to_vec(),
            target: intent.target,
            checkpoint: intent.prepared.next,
            envelope: env,
        }
    }

    pub(crate) async fn refuse_unproven_image_activation(&self) {
        let (deployment, prepared, _) = self
            .managed_images
            .as_ref()
            .unwrap()
            .lock()
            .await
            .activation_evidence();
        let prepared = prepared.unwrap();
        let principal = self
            .native
            .as_ref()
            .unwrap()
            .credential(&deployment.agent, &deployment.device)
            .unwrap()
            .principal();
        let key = data_key(self, &prepared, "record");
        let transaction = self.execution.transaction().await.unwrap();
        let original = transaction
            .get(&key, false)
            .await
            .unwrap()
            .unwrap()
            .to_vec();
        let root = self
            .execution
            .native_root(&transaction, &principal)
            .await
            .unwrap();
        transaction.clear(&key);
        transaction.commit().await.unwrap();
        assert_eq!(
            self.check_image_checkpoint().await.unwrap_err(),
            RuntimeError::RestoreUnreconciled
        );
        let transaction = self.execution.transaction().await.unwrap();
        assert_eq!(
            self.execution
                .native_root(&transaction, &principal)
                .await
                .unwrap(),
            root
        );
        transaction.set(&key, &original);
        transaction.commit().await.unwrap();
        let pin = self.execution.image_activation_pin(&principal);
        let transaction = self.execution.transaction().await.unwrap();
        let original = transaction
            .get(&pin, false)
            .await
            .unwrap()
            .unwrap()
            .to_vec();
        let mut corrupt = original.clone();
        *corrupt.last_mut().unwrap() ^= 1;
        transaction.set(&pin, &corrupt);
        transaction.commit().await.unwrap();
        assert!(self.check_image_checkpoint().await.is_err());
        let transaction = self.execution.transaction().await.unwrap();
        assert_eq!(
            self.execution
                .native_root(&transaction, &principal)
                .await
                .unwrap(),
            root
        );
        transaction.set(&pin, &original);
        transaction.commit().await.unwrap();
        assert_eq!(
            self.image_checkpoints(&deployment).await.unwrap(),
            (prepared.next, prepared.previous)
        );
    }

    pub(crate) async fn assert_image_activation_recovered(&self, proof: &ImageActivationProof) {
        let guard = self.managed_images.as_ref().unwrap().lock().await;
        let transaction = self.execution.transaction().await.unwrap();
        let principal = &proof.target.context.principal;
        assert_eq!(
            self.execution
                .native_root(&transaction, principal)
                .await
                .unwrap(),
            Some(proof.target.clone())
        );
        assert_eq!(
            transaction
                .get(&proof.key, false)
                .await
                .unwrap()
                .unwrap()
                .as_ref(),
            proof.record
        );
        assert!(transaction
            .get(&self.execution.image_activation_pin(principal), false)
            .await
            .unwrap()
            .is_none());
        let (deployment, pending, checkpoint) = guard.activation_evidence();
        assert_eq!(
            self.image_checkpoints(&deployment).await.unwrap(),
            (proof.checkpoint.clone(), proof.checkpoint.clone())
        );
        assert_eq!(checkpoint, proof.checkpoint);
        assert!(pending.is_none());
    }
}

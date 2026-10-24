async fn restored_device_rejoins_encrypted_chats(recovery_key: bool) {
    let w = world(if recovery_key {
        "backup_e2e_rk"
    } else {
        "backup_e2e_pw"
    })
    .await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let ana = w.id_of("ana").await;
    let old_device = w
        .scalar(&format!(
            "SELECT device FROM devices WHERE identity = '{ana}'"
        ))
        .await;
    let space = w
        .zoen("bruno", &["group", "Cofre", "@ana"])
        .trim()
        .to_string();
    let before = "antes de perder o celular: tucano";
    w.zoen("bruno", &["send", "Cofre", before]);
    assert!(w.zoen("ana", &["read", "Cofre"]).contains(before));
    let (flag, secret) = if recovery_key {
        let out = w.zoen("ana", &["backup", "on", "--recovery-key"]);
        (
            "--recovery-key",
            out.lines()
                .find_map(|l| l.strip_prefix("recovery-key\t"))
                .expect("recovery key")
                .to_string(),
        )
    } else {
        w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
        ("--password", PASSWORD.to_string())
    };

    // The old phone stays offline throughout recovery. Bruno is the surviving admin.
    let restored = w.zoen("ana-new", &["recover", "@ana", flag, &secret]);
    assert!(restored.contains("connection=online"), "{restored}");
    let new_device = w
        .scalar(&format!(
            "SELECT device FROM devices WHERE identity = '{ana}' AND device <> '{old_device}'"
        ))
        .await;
    assert!(w.events_in(&space).await.iter().any(|ev| {
        ev.env.author() == ana
            && matches!(ev.env.body(), Some(EventBody::DeviceJoining { device }) if device == new_device)
    }), "the restored device never requested its own fresh MLS leaf");
    w.sync_until("bruno", |s| s.contains("pending=0"));
    w.sync_until("ana-new", |s| s.contains("pending=0"));
    let after = "depois de recuperar: maracujá";
    w.zoen("bruno", &["send", "Cofre", after]);
    let history = w.zoen("ana-new", &["read", "Cofre"]);
    assert!(
        history.contains(before) && history.contains(after),
        "{history}"
    );
    let reply = "celular novo também envia: bem-te-vi";
    w.zoen("ana-new", &["send", "Cofre", reply]);
    assert!(w.zoen("bruno", &["read", "Cofre"]).contains(reply));

    // The recovered account can revoke the lost phone and keep the encrypted chat going.
    w.zoen("ana-new", &["unlink", &old_device]);
    w.sync_until("ana-new", |s| s.contains("pending=0"));
    let revoked = w.try_zoen("ana", &["sync"]).unwrap_err();
    assert!(revoked.contains("unlinked"), "{revoked}");
    let last = "depois de revogar o celular antigo";
    w.zoen("bruno", &["send", "Cofre", last]);
    assert!(w.zoen("ana-new", &["read", "Cofre"]).contains(last));
    for who in ["ana-new", "bruno"] {
        let verified = w.zoen(who, &["verify"]);
        assert!(!verified.contains("BROKEN"), "{who}: {verified}");
    }
    assert!(!backup_files(&w).iter().any(|blob| contains(blob, before)));
}

#[tokio::test]
async fn a_restored_password_device_rejoins_encrypted_chats() {
    restored_device_rejoins_encrypted_chats(false).await;
}

#[tokio::test]
async fn a_restored_recovery_key_device_rejoins_encrypted_chats() {
    restored_device_rejoins_encrypted_chats(true).await;
}

#[tokio::test]
async fn a_stale_backup_does_not_restore_removed_membership() {
    let w = world("backup_removed").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let ana = w.id_of("ana").await;
    let space = w
        .zoen("bruno", &["group", "Revogado", "@ana"])
        .trim()
        .to_string();
    let before = "história de quando eu era membro";
    w.zoen("bruno", &["send", "Revogado", before]);
    assert!(w.zoen("ana", &["read", "Revogado"]).contains(before));
    w.zoen("ana", &["backup", "on", "--password", PASSWORD]);
    w.zoen("bruno", &["remove", "Revogado", "@ana"]);
    let after = "segredo depois de remover a Ana";
    w.zoen("bruno", &["send", "Revogado", after]);

    w.zoen("ana-new", &["recover", "@ana", "--password", PASSWORD]);
    w.zoen("bruno", &["sync"]);
    let history = w.zoen("ana-new", &["read", "Revogado"]);
    assert!(history.contains(before), "{history}");
    assert!(
        !history.contains(after),
        "removed membership returned: {history}"
    );
    assert!(
        !w.events_in(&space).await.iter().any(|ev| {
            ev.env.author() == ana && matches!(ev.env.body(), Some(EventBody::DeviceJoining { .. }))
        }),
        "the relay accepted a removed member's recovery join request"
    );
}

async fn peerless_restore(recovery_key: bool) {
    let w = world(if recovery_key {
        "backup_no_peer_rk"
    } else {
        "backup_no_peer_pw"
    })
    .await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let space = w
        .zoen("ana", &["group", "Sem aparelhos", "@bruno"])
        .trim()
        .to_string();
    let before = "história antes de perder os dois aparelhos";
    w.zoen("ana", &["send", "Sem aparelhos", before]);
    assert!(w.zoen("bruno", &["read", "Sem aparelhos"]).contains(before));
    let ana = w.id_of("ana").await;
    let bruno = w.id_of("bruno").await;
    let old_ana = device_key(&w, "ana").id();
    let old_bruno = device_key(&w, "bruno").id();
    let mut secrets = Vec::new();
    for who in ["ana", "bruno"] {
        w.sync_until(who, |s| s.contains("pending=0"));
        let secret = if recovery_key {
            let out = w.zoen(who, &["backup", "on", "--recovery-key"]);
            out.lines()
                .find_map(|l| l.strip_prefix("recovery-key\t"))
                .expect("recovery key")
                .to_string()
        } else {
            w.zoen(who, &["backup", "on", "--password", PASSWORD]);
            PASSWORD.to_string()
        };
        secrets.push(secret);
    }
    let initial = w.events_in(&space).await;
    assert!(
        initial.iter().any(|ev| ev.env.recovery().is_some()),
        "no recoverable context was durably published"
    );
    assert_eq!(w.sessions_online(), 0);
    // Remove databases, encrypted group state AND vaults. No original process or device
    // survives; only authenticated server backups can recover the identities.
    for who in ["ana", "bruno"] {
        std::fs::remove_dir_all(w.dir.join(who)).unwrap();
    }
    let flag = if recovery_key {
        "--recovery-key"
    } else {
        "--password"
    };
    assert!(w
        .zoen("ana-new", &["recover", "@ana", flag, &secrets[0]])
        .contains("restored"));
    let new_ana = device_key(&w, "ana-new").id();
    assert_ne!(new_ana, old_ana);
    let after_first = "primeiro aparelho recuperado envia sem outro membro online";
    w.zoen("ana-new", &["send", "Sem aparelhos", after_first]);
    w.sync_until("ana-new", |s| s.contains("pending=0"));
    let first_recovery = w.events_in(&space).await;
    assert!(
        first_recovery
            .iter()
            .skip(initial.len())
            .any(|ev| ev.env.device() == Some(new_ana.as_str()) && ev.env.recovery().is_some()),
        "the first recovered device never sequenced its external join"
    );
    assert!(
        !w.dir.join("bruno-new").exists(),
        "a second recovering peer must not assist the first join"
    );
    assert!(w
        .zoen("bruno-new", &["recover", "@bruno", flag, &secrets[1]])
        .contains("restored"));
    let new_bruno = device_key(&w, "bruno-new").id();
    assert_ne!(new_bruno, old_bruno);
    w.zoen(
        "bruno-new",
        &[
            "send",
            "Sem aparelhos",
            "segundo aparelho recuperado responde",
        ],
    );
    w.sync_until("ana-new", |s| s.contains("pending=0"));
    let after_both = "mensagem nova depois de recuperar os dois";
    w.zoen("ana-new", &["send", "Sem aparelhos", after_both]);
    assert!(w
        .zoen("bruno-new", &["read", "Sem aparelhos"])
        .contains(after_both));
    let reply = "resposta cifrada dos dois aparelhos novos";
    w.zoen("bruno-new", &["send", "Sem aparelhos", reply]);
    let history = w.zoen("ana-new", &["read", "Sem aparelhos"]);
    assert!(
        history.contains(before) && history.contains(reply),
        "{history}"
    );
    for who in ["ana-new", "bruno-new"] {
        assert!(!w.zoen(who, &["verify"]).contains("BROKEN"));
    }
    let events = w.events_in(&space).await;
    assert!(
        !events
            .iter()
            .skip(initial.len())
            .any(|ev| matches!(ev.env.device(), Some(d) if d == old_ana || d == old_bruno)),
        "an old device wrote after its state was destroyed"
    );
    assert_eq!(w.count(&format!("SELECT count(*) FROM devices WHERE (identity = '{ana}' AND device = '{new_ana}') OR (identity = '{bruno}' AND device = '{new_bruno}')")).await, 2);
    assert!(!backup_files(&w).iter().any(|blob| contains(blob, before)));
    eprintln!("peerless recovery: both original databases/vaults destroyed; first fresh device externally joined alone; both fresh devices exchanged and verified encrypted messages");
}

#[tokio::test]
async fn both_lost_devices_recover_encrypted_chat_without_a_surviving_mls_peer() {
    peerless_restore(false).await;
}

#[tokio::test]
async fn both_lost_devices_recover_with_recovery_keys_without_a_surviving_mls_peer() {
    peerless_restore(true).await;
}

//! Encrypted profiles (ADR 0016): contacts read your name, bio and photo; everyone else
//! sees your @handle; the relay holds only ciphertext; blocking rotates the key.
//!
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:15433/postgres cargo test -p zoen-cli --test journey_profiles

mod common;
use common::*;
use roda_types::EventBody;

const PHOTO: &[u8] = b"\x89PNG\r\n\x1a\nana-on-the-trail";

fn bio(line: &str) -> &str {
    line.split('\t')
        .find_map(|f| f.strip_prefix("bio="))
        .unwrap_or_else(|| panic!("no bio in {line:?}"))
}

#[tokio::test]
async fn contacts_read_the_profile_strangers_see_the_handle() {
    let w = World::new("profiles").await;
    for (h, n) in [("ana", "Ana"), ("bruno", "Bruno"), ("carla", "Carla")] {
        w.init(h, n);
    }
    let photo = w.dir.join("ana.png");
    std::fs::write(&photo, PHOTO).unwrap();
    let mine = w.zoen(
        "ana",
        &[
            "profile",
            "set",
            "--bio",
            "Trilhas e café",
            "--photo",
            photo.to_str().unwrap(),
        ],
    );
    assert!(
        mine.starts_with("@ana\tAna\tbio=Trilhas e café\tphoto="),
        "{mine}"
    );

    // Strangers: the directory finds her by @, and that's all it knows.
    assert_eq!(
        w.zoen("bruno", &["profile", "show", "@ana"]).trim(),
        "@ana\t(hidden)"
    );
    assert!(w.zoen("bruno", &["people", "an"]).contains("@ana\t@ana\t"));

    // A chat makes them contacts: each side shares its profile key with the other.
    w.zoen("ana", &["dm", "@bruno", "oi Bruno!"]);
    let seen = w.dir.join("ana-as-bruno-sees-her.png");
    let bruno = w.zoen(
        "bruno",
        &["profile", "show", "@ana", "--out", seen.to_str().unwrap()],
    );
    assert!(
        bruno.starts_with("@ana\tAna\tbio=Trilhas e café\tphoto="),
        "{bruno}"
    );
    assert!(!bruno.contains("pending"), "{bruno}");
    assert_eq!(
        std::fs::read(&seen).unwrap(),
        PHOTO,
        "the photo arrived through the encrypted blob path"
    );
    assert_eq!(w.zoen("bruno", &["read", "@ana"]).trim(), "Ana: oi Bruno!");
    assert!(w
        .zoen("ana", &["profile", "show", "@bruno"])
        .starts_with("@bruno\tBruno\tbio=\t"));

    // Carla shares nothing with Ana: still just the handle.
    assert_eq!(
        w.zoen("carla", &["profile", "show", "@ana"]).trim(),
        "@ana\t(hidden)"
    );

    // The relay never saw a name, a bio or the photo key.
    assert_eq!(w.count("SELECT count(*) FROM profiles").await, 3);
    assert_eq!(w.count("SELECT count(*) FROM agreement_keys").await, 3);
    assert_eq!(
        w.count("SELECT count(*) FROM identities WHERE kind = 'Person' AND (profile->>'name' <> '' OR profile->>'bio' <> '')").await,
        0
    );
    for needle in ["Trilhas", "Ana", "image/png"] {
        assert_eq!(
            w.count(&format!("SELECT count(*) FROM profiles WHERE position(convert_to('{needle}', 'UTF8') in ciphertext) > 0")).await,
            0,
            "{needle} in a stored profile"
        );
    }
    let shares: usize = w
        .events()
        .await
        .iter()
        .filter_map(|s| s.plain_event())
        .map(|e| match e.body {
            EventBody::ProfileKeyShared { shares, .. } => shares.len(),
            _ => 0,
        })
        .sum();
    assert_eq!(shares, 2, "one share each way");

    // Live: Bruno's open app hears about the edit.
    let watch = w.spawn_zoen("bruno", &["watch", "--for", "6"]);
    std::thread::sleep(std::time::Duration::from_millis(1500));
    w.zoen("ana", &["profile", "set", "--bio", "Agora no Rio"]);
    let out = watch.wait_with_output().unwrap();
    let printed = String::from_utf8_lossy(&out.stdout);
    assert!(
        printed.contains("· @ana updated their profile"),
        "{printed}"
    );
    assert_eq!(
        bio(&w.zoen("bruno", &["profile", "show", "@ana"])),
        "Agora no Rio"
    );

    // A group adds a contact: Carla joins, Ana's device shares with her when it next syncs.
    w.zoen("ana", &["group", "Trilha", "@bruno"]);
    let code = w.zoen("ana", &["invite", "Trilha"]);
    let code = code.split('\t').next().unwrap().trim().to_string();
    w.zoen("carla", &["join", &code]);
    w.zoen("ana", &["sync"]);
    assert_eq!(
        bio(&w.zoen("carla", &["profile", "show", "@ana"])),
        "Agora no Rio"
    );
    assert!(w
        .zoen("ana", &["profile", "show", "@carla"])
        .starts_with("@carla\tCarla\t"));

    // Blocking rotates Ana's key: Bruno keeps what he saw and nothing after it.
    w.zoen("ana", &["block", "@bruno"]);
    w.zoen(
        "ana",
        &["profile", "set", "--bio", "Só para quem eu escolho"],
    );
    assert_eq!(
        bio(&w.zoen("bruno", &["profile", "show", "@ana"])),
        "Agora no Rio"
    );
    assert_eq!(
        bio(&w.zoen("carla", &["profile", "show", "@ana"])),
        "Só para quem eu escolho"
    );

    // Unblocking shares the current key again.
    w.zoen("ana", &["unblock", "@bruno"]);
    assert_eq!(
        bio(&w.zoen("bruno", &["profile", "show", "@ana"])),
        "Só para quem eu escolho"
    );

    for who in ["ana", "bruno", "carla"] {
        assert!(!w.zoen(who, &["verify"]).contains("BROKEN"));
    }
}

//! Import completion must describe relay acknowledgement, while preserving local
//! pages for a later retry when an authenticated relay limits publication.

mod common;
use common::*;

#[tokio::test]
async fn import_deadline_is_failure_and_the_saved_pages_survive_a_retry() {
    let mut w = World::new("page-completion").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let space = w
        .zoen("ana", &["group", "Casa", "@bruno"])
        .trim()
        .to_string();
    w.zoen("bruno", &["sync"]);
    let first = w.dir.join("first.md");
    let second = w.dir.join("second.md");
    std::fs::write(&first, "# First\n\nSaved before acknowledgement.\n").unwrap();
    std::fs::write(&second, "# Second\n\nMust survive the deadline.\n").unwrap();

    // One event can be acknowledged; the next cannot refill inside the original
    // command deadline. This is an actual relay refusal, not a mocked idle result.
    w.stop_relay();
    w.configure_relay_env("ZOEN_LIMITS", Some("publish_device=1/h:1"));
    w.start_relay();
    let failure = w
        .try_zoen(
            "ana",
            &[
                "page",
                "import",
                &space,
                first.to_str().unwrap(),
                second.to_str().unwrap(),
            ],
        )
        .expect_err("an unfinished online import must not exit successfully");
    assert!(failure.contains("import:"), "{failure}");
    assert!(
        !failure.contains("\tfirst.md"),
        "must not print a completion receipt: {failure}"
    );
    assert!(
        !failure.contains("\tsecond.md"),
        "must not print a completion receipt: {failure}"
    );
    let saved = w.zoen("ana", &["pages", &space, "--offline"]);
    assert!(saved.contains("first.md"), "{saved}");
    assert!(saved.contains("second.md"), "{saved}");

    w.stop_relay();
    w.configure_relay_env("ZOEN_LIMITS", None);
    w.start_relay();
    w.zoen("ana", &["sync"]);
    let exported = w.dir.join("exported");
    w.zoen(
        "bruno",
        &[
            "page",
            "export-all",
            &space,
            "--dir",
            exported.to_str().unwrap(),
        ],
    );
    for source in [first, second] {
        assert_eq!(
            std::fs::read(&source).unwrap(),
            std::fs::read(exported.join(source.file_name().unwrap())).unwrap()
        );
    }
    for who in ["ana", "bruno"] {
        w.zoen(who, &["verify"]);
    }
}

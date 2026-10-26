use super::{preflight, Input};

fn args(values: &[&str]) -> Vec<String> {
    values.iter().map(|value| (*value).into()).collect()
}

#[test]
fn an_explicit_mime_is_exact_metadata_in_an_ordinary_file_import() {
    let values = args(&[
        "--mime",
        "text/html;profile=mcp-app",
        "chat",
        "/picked/My App.html",
        "--path",
        "Apps/",
        "--thumb",
        "/picked/preview.png",
    ]);
    let input = Input::parse(&values).unwrap();
    assert_eq!(input.mime, "text/html;profile=mcp-app");
    assert_eq!(input.chat, "chat");
    assert_eq!(input.file, "/picked/My App.html");
    assert_eq!(input.name, "My App.html");
    assert_eq!(input.path, "Apps/My App.html");
    assert_eq!(input.thumbnail.as_deref(), Some("/picked/preview.png"));
    assert_eq!(
        Input::parse(&args(&["chat", "a.txt", "--path", ""]))
            .unwrap()
            .path,
        "a.txt"
    );
}

#[test]
fn omitted_mime_preserves_extension_inference_including_unknown_html() {
    for (file, expected) in [
        ("note.MD", "text/markdown"),
        ("a.txt", "text/plain"),
        ("a.png", "image/png"),
        ("a.jpeg", "image/jpeg"),
        ("a.pdf", "application/pdf"),
        ("a.mp4", "video/mp4"),
        ("a.m4a", "audio/mp4"),
        ("a.json", "application/json"),
        ("a.html", "application/octet-stream"),
        ("-notes.custom", "application/octet-stream"),
    ] {
        assert_eq!(Input::parse(&args(&["chat", file])).unwrap().mime, expected);
    }
}

#[test]
fn malformed_or_ambiguous_import_options_refuse_before_any_home_is_opened() {
    for values in [
        vec!["add", "chat", "a.html", "--mime"],
        vec!["add", "chat", "a.html", "--mime", ""],
        vec![
            "add",
            "chat",
            "a.html",
            "--mime",
            "text/html\r\nInjected: value",
        ],
        vec!["add", "chat", "a.html", "--mime", "text/html\0"],
        vec!["add", "chat", "a.html", "--mime", " text/html"],
        vec!["add", "chat", "a.html", "--mime", " "],
        vec!["add", "chat", "a.html", "--mime", "text"],
        vec!["add", "chat", "a.html", "--mime", "text/"],
        vec!["add", "chat", "a.html", "--mime", "/html"],
        vec![
            "add",
            "chat",
            "a.html",
            "--mime",
            "text/html",
            "--mime",
            "application/json",
        ],
        vec!["add", "chat", "a.html", "--mime", "--thumb"],
        vec!["add", "chat", "a.html", "--mime=text/html"],
        vec!["add", "chat", "a.html", "--path", "one", "--path", "two"],
        vec!["add", "chat", "a.html", "--thumb"],
        vec!["add", "chat", "a.html", "--unknown", "value"],
        vec!["add", "chat", "--mime", "text/html"],
        vec!["add", "chat", "a.html", "extra"],
    ] {
        assert!(preflight("file", &args(&values)).is_err(), "{values:?}");
    }
    assert!(preflight(
        "--mime",
        &args(&["text/html", "file", "add", "chat", "a.html"])
    )
    .is_err());
    assert!(preflight(
        "file",
        &args(&["--mime", "text/html", "add", "chat", "a.html"])
    )
    .is_err());
}

#[test]
fn stranded_options_before_file_add_are_not_an_alternative_command() {
    for command in ["--home", "--path", "--timeout", "--offline"] {
        assert!(preflight(
            command,
            &args(&["leftover", "file", "add", "chat", "a.html", "--mime", ""])
        )
        .is_err());
    }
    assert!(preflight(
        "file",
        &args(&["--path", "folder", "add", "chat", "a.html"])
    )
    .is_err());
}

#[test]
fn unrelated_commands_and_message_text_retain_their_existing_arguments() {
    for (command, values) in [
        ("send", vec!["chat", "--mime", "text/html;profile=mcp-app"]),
        ("page", vec!["new", "chat", "--mime"]),
        ("file", vec!["get", "--mime", "--out", "local"]),
        ("agent-host", vec![]),
        ("agent-controls", vec!["--for", "1"]),
    ] {
        let values = args(&values);
        let before = values.clone();
        assert!(preflight(command, &values).is_ok());
        assert_eq!(values, before);
    }
}

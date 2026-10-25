//! Real CLI argv must fail before creating/opening a home or reading an account.
use std::process::Command;

#[test]
fn invalid_mime_argv_never_creates_a_home() -> Result<(), Box<dyn std::error::Error>> {
    for args in [
        vec!["file", "add", "chat", "a.html", "--mime"],
        vec!["file", "add", "chat", "a.html", "--mime", ""],
        vec![
            "file",
            "add",
            "chat",
            "a.html",
            "--mime",
            "text/html\r\nInjected: value",
        ],
        vec![
            "file",
            "add",
            "chat",
            "a.html",
            "--mime",
            "text/html",
            "--mime",
            "application/json",
        ],
        vec!["--mime", "text/html", "file", "add", "chat", "a.html"],
        vec!["file", "--mime", "text/html", "add", "chat", "a.html"],
        vec![
            "--home", "other", "file", "add", "chat", "a.html", "--mime", "",
        ],
        vec![
            "--path", "folder", "file", "add", "chat", "a.html", "--mime", "",
        ],
        vec!["file", "--path", "folder", "add", "chat", "a.html"],
    ] {
        let home = std::env::temp_dir().join(types::new_id("invalid-file-import")?);
        assert!(!home.exists());
        let result = Command::new(env!("CARGO_BIN_EXE_zoen"))
            .arg("--home")
            .arg(&home)
            .args(&args)
            .output()?;
        assert!(!result.status.success(), "{args:?}");
        assert!(
            String::from_utf8_lossy(&result.stderr).contains("file_add_"),
            "{:?}",
            result.stderr
        );
        assert!(
            !home.exists(),
            "invalid argv opened {}: {args:?}",
            home.display()
        );
    }
    Ok(())
}

//! Documents commands use the same already-open Client.
use super::*;

pub(super) async fn run(cmd: String, cli: &mut Cli, context: Context) {
    let Context {
        e,
        timeout,
        offline,
        ..
    } = context;
    match cmd.as_str() {
        "pages" | "files" => {
            let space = chat(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            let kind = if cmd == "pages" { "page" } else { "file" };
            for i in e
                .items()
                .into_iter()
                .filter(|i| i.space_id == space && i.kind_id == kind)
            {
                let extra = match &i.file {
                    Some(f) => format!("\t{} bytes\t{}/{} here", f.bytes, f.chunks_here, f.chunks),
                    None => String::new(),
                };
                println!("{}\t{}\tv{}\t{}{}", i.id, i.path, i.version, i.title, extra);
            }
        }
        "page" => {
            if cli.args.is_empty() {
                usage();
            }
            let sub = cli.args.remove(0);
            match sub.as_str() {
                "import" => {
                    let root = cli.flag("--root").map(PathBuf::from);
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let space = chat(&e, &cli.args[0]);
                    let mut files = Vec::new();
                    for f in &cli.args[1..] {
                        let bytes = read_file(f);
                        let md = String::from_utf8(bytes)
                            .unwrap_or_else(|_| die(format!("{f}: not UTF-8 text")));
                        let fp = PathBuf::from(f);
                        let path = match &root {
                            Some(r) => fp
                                .strip_prefix(r)
                                .unwrap_or(&fp)
                                .to_string_lossy()
                                .into_owned(),
                            None => fp
                                .file_name()
                                .map(|n| n.to_string_lossy().into_owned())
                                .unwrap_or_default(),
                        };
                        files.push(MarkdownFileDto { path, markdown: md });
                    }
                    let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
                    let items = e
                        .pages_import_markdown(space.clone(), files)
                        .unwrap_or_else(|err| die(format!("import: {err}")));
                    if !offline {
                        e.wait_until_settled(timeout)
                            .await
                            .unwrap_or_else(|err| die(format!("import: {err}")));
                    }
                    for (d, path) in items.iter().zip(&paths) {
                        println!("{}\t{}", d.id, path);
                    }
                }
                "new" => {
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let space = chat(&e, &cli.args[0]);
                    let d = e
                        .page_create(space, cli.args[1..].join(" "))
                        .unwrap_or_else(|err| die(err));
                    e.wait_until_idle(timeout).await;
                    println!("{}", d.id);
                }
                "export" => {
                    let out = cli.flag("--out").unwrap_or_else(|| usage());
                    let it = item(
                        &e,
                        cli.args
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| usage()),
                    );
                    if !ready(&e, &it.id, timeout).await {
                        die("the page is still arriving");
                    }
                    let md = e.page_markdown(it.id).unwrap_or_else(|err| die(err));
                    write_file(Path::new(&out), md.as_bytes());
                    println!("{} bytes", md.len());
                }
                "export-all" => {
                    let dir = PathBuf::from(cli.flag("--dir").unwrap_or_else(|| usage()));
                    let space = chat(
                        &e,
                        cli.args
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| usage()),
                    );
                    let output = super::export_admission::export_all(
                        &e, space, dir, timeout, offline, tokio::time::Instant::now(),
                    ).await.unwrap_or_else(|err| die(format!(
                        "export-all: {err}; no files were written. Use --offline to export only this device's cached pages."
                    )));
                    println!("{output}");
                }
                "show" => {
                    let it = item(
                        &e,
                        cli.args
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| usage()),
                    );
                    let p = e.page(it.id).unwrap_or_else(|err| die(err));
                    println!(
                        "{} (v{}{})",
                        p.title,
                        p.version,
                        if p.unsaved { ", unsaved" } else { "" }
                    );
                    for (n, b) in p.blocks.iter().enumerate() {
                        let mark = match b.kind.as_str() {
                            "task" => {
                                if b.checked {
                                    "[x] "
                                } else {
                                    "[ ] "
                                }
                            }
                            _ => "",
                        };
                        println!(
                            "{}\t{}\t{mark}{}",
                            n + 1,
                            b.kind,
                            b.text.replace('\u{2028}', " / ")
                        );
                    }
                }
                "check" => {
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let it = item(&e, &cli.args[0]);
                    let n: usize = cli.args[1].parse().unwrap_or_else(|_| usage());
                    edit_page(&e, &it.id, |blocks| {
                        let tasks: Vec<usize> = (0..blocks.len())
                            .filter(|&i| blocks[i].kind == "task")
                            .collect();
                        let i = *tasks
                            .get(n.wrapping_sub(1))
                            .unwrap_or_else(|| die(format!("no task {n}")));
                        blocks[i].checked = !blocks[i].checked;
                        vec![blocks[i].id.clone()]
                    });
                    e.wait_until_idle(timeout).await;
                }
                "add" => {
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let it = item(&e, &cli.args[0]);
                    let text = cli.args[1..].join(" ");
                    edit_page(&e, &it.id, |blocks| {
                        let id = new_block_id();
                        blocks.push(client_core::PageBlockDto {
                            id: id.clone(),
                            kind: "paragraph".into(),
                            level: 0,
                            indent: 0,
                            number: 0,
                            checked: false,
                            lang: String::new(),
                            url: String::new(),
                            alt: String::new(),
                            text,
                            spans: vec![],
                        });
                        vec![id]
                    });
                    e.wait_until_idle(timeout).await;
                }
                "set" => {
                    if cli.args.len() < 3 {
                        usage();
                    }
                    let it = item(&e, &cli.args[0]);
                    let n: usize = cli.args[1].parse().unwrap_or_else(|_| usage());
                    let text = cli.args[2..].join(" ");
                    edit_page(&e, &it.id, |blocks| {
                        let b = blocks
                            .get_mut(n.wrapping_sub(1))
                            .unwrap_or_else(|| die(format!("no block {n}")));
                        b.text = text;
                        b.spans.clear();
                        vec![b.id.clone()]
                    });
                    e.wait_until_idle(timeout).await;
                }
                _ => usage(),
            }
        }
        "file" => {
            if cli.args.is_empty() {
                usage();
            }
            let sub = cli.args.remove(0);
            let thumb = cli.flag("--thumb").map(|p| read_file(&p));
            match sub.as_str() {
                "add" => {
                    let dir = cli.flag("--path").unwrap_or_default();
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let space = chat(&e, &cli.args[0]);
                    let f = &cli.args[1];
                    let name = Path::new(f)
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| f.clone());
                    let path = if dir.is_empty() {
                        name.clone()
                    } else {
                        format!("{}/{name}", dir.trim_end_matches('/'))
                    };
                    let d = e
                        .file_add(space, path, name, mime_of(f).into(), read_file(f), thumb)
                        .unwrap_or_else(|err| die(err));
                    e.wait_until_idle(timeout).await;
                    let fi = d.file.unwrap_or_else(|| die("not a file"));
                    println!("{}\t{} bytes\t{} pieces", d.id, fi.bytes, fi.chunks);
                }
                "version" => {
                    if cli.args.len() < 2 {
                        usage();
                    }
                    let it = item(&e, &cli.args[0]);
                    let d = e
                        .file_new_version(it.id, read_file(&cli.args[1]), thumb, String::new())
                        .unwrap_or_else(|err| die(err));
                    e.wait_until_idle(timeout).await;
                    println!("v{}", d.version);
                }
                "get" => {
                    let out = cli.flag("--out").unwrap_or_else(|| usage());
                    let version: Option<u32> = cli.flag("--version").and_then(|v| v.parse().ok());
                    let thumb_out = cli.flag("--thumb-out");
                    let it = item(
                        &e,
                        cli.args
                            .first()
                            .map(String::as_str)
                            .unwrap_or_else(|| usage()),
                    );
                    let deadline = tokio::time::Instant::now() + Duration::from_millis(timeout);
                    let bytes = loop {
                        if let Some(b) = e
                            .file_bytes(it.id.clone(), version)
                            .unwrap_or_else(|err| die(err))
                        {
                            break b;
                        }
                        if tokio::time::Instant::now() >= deadline {
                            die("the file is still arriving");
                        }
                        tokio::time::sleep(Duration::from_millis(100)).await;
                    };
                    write_file(Path::new(&out), &bytes);
                    if let Some(t) = thumb_out {
                        let tb = e
                            .file_thumbnail(it.id.clone())
                            .unwrap_or_else(|err| die(err))
                            .unwrap_or_else(|| die("no preview"));
                        write_file(Path::new(&t), &tb);
                    }
                    println!("{} bytes", bytes.len());
                }
                _ => usage(),
            }
        }
        "versions" => {
            let it = item(
                &e,
                cli.args
                    .first()
                    .map(String::as_str)
                    .unwrap_or_else(|| usage()),
            );
            for v in &it.versions {
                println!("v{}\t{}\t{}", v.number, v.author.name, v.note);
            }
        }
        "restore" => {
            if cli.args.len() < 2 {
                usage();
            }
            let it = item(&e, &cli.args[0]);
            let n: u32 = cli.args[1].parse().unwrap_or_else(|_| usage());
            let d = if it.kind_id == "plan" {
                let observed = e.plan_page(it.id.clone()).unwrap_or_else(|err| die(err));
                e.restore_plan_version(it.id, n, observed.content_hash)
                    .unwrap_or_else(|err| die(err))
            } else {
                e.restore_version(it.id, n).unwrap_or_else(|err| die(err))
            };
            e.wait_until_idle(timeout).await;
            println!("v{}", d.version);
        }
        _ => usage(),
    }
}

//! Pages and files (ADR 0023), as two people see them through the relay.
//!
//! Ana imports Markdown as pages; Bruno, on his own device, exports them and gets the
//! same bytes back. Edits change only the lines they touch. A 12 MB file travels in
//! encrypted pieces; a small change uploads one new piece, not twelve megabytes; the
//! preview Ana's device made arrives with it. The relay never holds a readable byte.
//!
//!   eval "$(scripts/fdb.sh env)"
//!   ZOEN_TEST_PG=postgres://zoen@127.0.0.1:55432/postgres cargo test -p zoen-cli --test journey_files
//!
//! `ZOEN_MD_CORPUS=dir1:dir2` adds more Markdown (every `*.md` below each dir).

mod common;
use common::*;
use std::path::{Path, PathBuf};

fn md_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = rd.flatten().map(|e| e.path()).collect();
    entries.sort();
    for p in entries {
        if p.is_dir() {
            md_files(&p, out);
        } else if p.extension().is_some_and(|e| e == "md") {
            out.push(p);
        }
    }
}

fn blob_files(dir: &Path) -> Vec<PathBuf> {
    let mut out = vec![];
    let Ok(rd) = std::fs::read_dir(dir) else {
        return out;
    };
    for p in rd.flatten().map(|e| e.path()) {
        if p.is_dir() {
            out.extend(blob_files(&p));
        } else {
            out.push(p);
        }
    }
    out
}

/// Deterministic bytes that don't compress or repeat (xorshift).
fn noise(n: usize, seed: u64) -> Vec<u8> {
    let mut x = seed;
    (0..n)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8
        })
        .collect()
}

fn contains(hay: &[u8], needle: &[u8]) -> bool {
    hay.windows(needle.len()).any(|w| w == needle)
}

/// Lines that differ between two texts (the same number of lines or one appended).
fn changed_lines(a: &str, b: &str) -> Vec<(Option<String>, Option<String>)> {
    let (la, lb): (Vec<_>, Vec<_>) = (a.lines().collect(), b.lines().collect());
    (0..la.len().max(lb.len()))
        .filter(|&i| la.get(i) != lb.get(i))
        .map(|i| {
            (
                la.get(i).map(|s| s.to_string()),
                lb.get(i).map(|s| s.to_string()),
            )
        })
        .collect()
}

const PLAN: &str = "# Viagem a Paraty\n\nRoteiro de *três dias*, com **barco** e trilha.\n\n- [ ] Reservar a pousada\n- [ ] Alugar o barco\n- [x] Comprar protetor\n\n| Dia | Plano |\n|-----|-------|\n| 1   | Centro histórico |\n\n> Levar dinheiro: muitos lugares não aceitam cartão.\n";

#[tokio::test]
async fn pages_and_files_travel_byte_for_byte() {
    let w = World::new("files").await;
    w.init("ana", "Ana");
    w.init("bruno", "Bruno");
    let space = w
        .zoen("ana", &["group", "Casa", "@bruno"])
        .trim()
        .to_string();
    w.zoen("bruno", &["sync"]);

    // ── Markdown in, the same Markdown out, on the other device ──
    let repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut roots: Vec<PathBuf> = vec![repo.join("docs")];
    if let Ok(extra) = std::env::var("ZOEN_MD_CORPUS") {
        roots.extend(
            extra
                .split(':')
                .filter(|s| !s.is_empty())
                .map(PathBuf::from),
        );
    }
    let src = w.dir.join("src");
    std::fs::write(w.dir.join("plan.md"), PLAN).unwrap();
    let mut expected: Vec<(String, Vec<u8>)> = vec![];
    for (n, root) in roots.iter().enumerate() {
        let mut files = vec![];
        md_files(root, &mut files);
        for f in files {
            let bytes = std::fs::read(&f).unwrap();
            if std::str::from_utf8(&bytes).is_err() {
                continue;
            }
            let rel = format!("{n}/{}", f.strip_prefix(root).unwrap().display());
            let dst = src.join(&rel);
            std::fs::create_dir_all(dst.parent().unwrap()).unwrap();
            std::fs::write(&dst, &bytes).unwrap();
            expected.push((rel, bytes));
        }
    }
    let total_bytes: usize = expected.iter().map(|(_, b)| b.len()).sum();
    let largest = expected.iter().map(|(_, b)| b.len()).max().unwrap();
    assert!(
        largest > 48 * 1024,
        "the sample has a page too big for one event"
    );
    let started = std::time::Instant::now();
    for batch in expected.chunks(100) {
        let mut args = vec![
            "page".to_string(),
            "import".into(),
            space.clone(),
            "--root".into(),
            src.display().to_string(),
        ];
        args.extend(
            batch
                .iter()
                .map(|(rel, _)| src.join(rel).display().to_string()),
        );
        let a: Vec<&str> = args.iter().map(String::as_str).collect();
        let out = w.zoen("ana", &a);
        assert_eq!(out.lines().count(), batch.len(), "{out}");
    }
    let import_ms = started.elapsed().as_millis();
    w.zoen(
        "ana",
        &[
            "page",
            "import",
            &space,
            w.dir.join("plan.md").to_str().unwrap(),
        ],
    );

    let started = std::time::Instant::now();
    let out_dir = w.dir.join("bruno-export");
    let summary = w.zoen(
        "bruno",
        &[
            "--timeout",
            "60000",
            "page",
            "export-all",
            &space,
            "--dir",
            out_dir.to_str().unwrap(),
        ],
    );
    let export_ms = started.elapsed().as_millis();
    assert!(summary.ends_with("0 still arriving\n"), "{summary}");
    let mut identical = 0;
    let mut differ = vec![];
    for (rel, bytes) in &expected {
        match std::fs::read(out_dir.join(rel)) {
            Ok(b) if &b == bytes => identical += 1,
            _ => differ.push(rel.clone()),
        }
    }
    println!(
        "ROUND TRIP: {identical}/{} pages byte-identical on Bruno's device ({} bytes, largest {} bytes; import {import_ms} ms, export {export_ms} ms)",
        expected.len(),
        total_bytes,
        largest
    );
    assert!(differ.is_empty(), "these came back different: {differ:?}");
    assert_eq!(
        std::fs::read_to_string(out_dir.join("plan.md")).unwrap(),
        PLAN
    );

    // ── Bruno edits: only the lines he touched change, for Ana too ──
    assert!(w
        .zoen("bruno", &["page", "check", "plan.md", "1"])
        .contains("saved"));
    assert!(w
        .zoen("bruno", &["page", "add", "plan.md", "Saída às 7h."])
        .contains("saved"));
    let edited = w.dir.join("ana-plan.md");
    w.zoen(
        "ana",
        &[
            "page",
            "export",
            "plan.md",
            "--out",
            edited.to_str().unwrap(),
        ],
    );
    let after = std::fs::read_to_string(&edited).unwrap();
    let diff = changed_lines(PLAN, &after);
    println!("EDIT: {diff:?}");
    assert_eq!(
        diff,
        vec![
            (
                Some("- [ ] Reservar a pousada".into()),
                Some("- [x] Reservar a pousada".into())
            ),
            (None, Some("".into())),
            (None, Some("Saída às 7h.".into())),
        ],
        "{after}"
    );
    // The edited page is still plain Markdown: importing it again changes nothing.
    std::fs::write(w.dir.join("again.md"), &after).unwrap();
    w.zoen(
        "ana",
        &[
            "page",
            "import",
            &space,
            w.dir.join("again.md").to_str().unwrap(),
        ],
    );
    let again = w.dir.join("again-out.md");
    w.zoen(
        "bruno",
        &[
            "page",
            "export",
            "again.md",
            "--out",
            again.to_str().unwrap(),
        ],
    );
    assert_eq!(std::fs::read_to_string(&again).unwrap(), after);

    // Versions, and going back to the first one (as a new version: nothing is lost).
    let versions = w.zoen("ana", &["versions", "plan.md"]);
    assert_eq!(versions.lines().count(), 3, "{versions}");
    assert!(
        versions.lines().nth(1).unwrap().contains("Bruno"),
        "{versions}"
    );
    assert_eq!(w.zoen("ana", &["restore", "plan.md", "1"]).trim(), "v4");
    let back = w.dir.join("bruno-back.md");
    w.zoen(
        "bruno",
        &["page", "export", "plan.md", "--out", back.to_str().unwrap()],
    );
    assert_eq!(std::fs::read_to_string(&back).unwrap(), PLAN);

    // ── A 12 MB file in pieces, a preview, and a new version that re-uses pieces ──
    let blobs = w.dir.join("blobs");
    let before_file = blob_files(&blobs).len();
    let big = noise(12 << 20, 0x5eed);
    let thumb = b"\x89PNG\r\n\x1a\npreview-made-on-ana's-phone".to_vec();
    std::fs::write(w.dir.join("video.mp4"), &big).unwrap();
    std::fs::write(w.dir.join("thumb.png"), &thumb).unwrap();
    let added = w.zoen(
        "ana",
        &[
            "--timeout",
            "60000",
            "file",
            "add",
            &space,
            w.dir.join("video.mp4").to_str().unwrap(),
            "--thumb",
            w.dir.join("thumb.png").to_str().unwrap(),
            "--path",
            "Viagem",
        ],
    );
    let pieces: usize = added
        .split('\t')
        .nth(2)
        .unwrap()
        .trim()
        .trim_end_matches(" pieces")
        .parse()
        .unwrap();
    assert!(pieces >= 2, "{added}");
    let after_v1 = blob_files(&blobs).len();
    assert_eq!(
        after_v1 - before_file,
        pieces + 1,
        "every piece and the preview reached the relay"
    );

    let got = w.dir.join("bruno-video.mp4");
    let got_thumb = w.dir.join("bruno-thumb.png");
    w.zoen(
        "bruno",
        &[
            "--timeout",
            "60000",
            "file",
            "get",
            "Viagem/video.mp4",
            "--out",
            got.to_str().unwrap(),
            "--thumb-out",
            got_thumb.to_str().unwrap(),
        ],
    );
    assert!(
        std::fs::read(&got).unwrap() == big,
        "Bruno's copy is the same file"
    );
    assert_eq!(std::fs::read(&got_thumb).unwrap(), thumb);
    let listed = w.zoen("bruno", &["files", &space]);
    assert!(
        listed.contains(&format!(
            "Viagem/video.mp4\tv1\tvideo.mp4\t{} bytes\t{pieces}/{pieces} here",
            big.len()
        )),
        "{listed}"
    );

    // Ana trims a few bytes in the middle: one piece changes, the rest are re-used.
    let mut v2 = big.clone();
    for b in &mut v2[6_000_000..6_000_100] {
        *b = 0;
    }
    std::fs::write(w.dir.join("video-v2.mp4"), &v2).unwrap();
    assert_eq!(
        w.zoen(
            "ana",
            &[
                "--timeout",
                "60000",
                "file",
                "version",
                "Viagem/video.mp4",
                w.dir.join("video-v2.mp4").to_str().unwrap()
            ]
        )
        .trim(),
        "v2"
    );
    let new_blobs = blob_files(&blobs).len() - after_v1;
    println!("FILE: 12 MiB in {pieces} pieces; the new version uploaded {new_blobs} new piece(s)");
    assert!(
        new_blobs >= 1 && new_blobs <= 2,
        "only the changed piece(s) went up, not {new_blobs}"
    );
    let got2 = w.dir.join("bruno-video-v2.mp4");
    w.zoen(
        "bruno",
        &[
            "--timeout",
            "60000",
            "file",
            "get",
            "Viagem/video.mp4",
            "--out",
            got2.to_str().unwrap(),
        ],
    );
    assert!(std::fs::read(&got2).unwrap() == v2);
    let got1 = w.dir.join("bruno-video-v1.mp4");
    w.zoen(
        "bruno",
        &[
            "file",
            "get",
            "Viagem/video.mp4",
            "--version",
            "1",
            "--out",
            got1.to_str().unwrap(),
        ],
    );
    assert!(
        std::fs::read(&got1).unwrap() == big,
        "the first version is still there"
    );

    // Bruno saves a version too: his device re-uses Ana's pieces.
    let mut v3 = v2.clone();
    v3.extend_from_slice(b"bruno's ending");
    std::fs::write(w.dir.join("video-v3.mp4"), &v3).unwrap();
    let before_v3 = blob_files(&blobs).len();
    w.zoen(
        "bruno",
        &[
            "--timeout",
            "60000",
            "file",
            "version",
            "Viagem/video.mp4",
            w.dir.join("video-v3.mp4").to_str().unwrap(),
        ],
    );
    let v3_new = blob_files(&blobs).len() - before_v3;
    assert!(
        v3_new <= 2,
        "Bruno's version re-used the pieces he already had ({v3_new} new)"
    );
    let got3 = w.dir.join("ana-video-v3.mp4");
    w.zoen(
        "ana",
        &[
            "--timeout",
            "60000",
            "file",
            "get",
            "Viagem/video.mp4",
            "--out",
            got3.to_str().unwrap(),
        ],
    );
    assert!(std::fs::read(&got3).unwrap() == v3);

    // ── The relay holds only ciphertext ──
    let large_page = expected.iter().max_by_key(|(_, b)| b.len()).unwrap();
    let probes: Vec<&[u8]> = vec![
        &big[1_000_000..1_000_064],
        &big[9_000_000..9_000_064],
        &thumb[8..],
        &large_page.1[large_page.1.len() / 2..large_page.1.len() / 2 + 48],
    ];
    for f in blob_files(&blobs) {
        let b = std::fs::read(&f).unwrap();
        for p in &probes {
            assert!(!contains(&b, p), "{} holds readable bytes", f.display());
        }
    }
}

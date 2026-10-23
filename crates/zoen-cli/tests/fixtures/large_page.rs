//! A bounded, synthetic page whose actual Loro snapshot must use an external payload.

use std::fmt::Write as _;

use documents::Page;
use sha2::{Digest, Sha256};

pub const FILE_NAME: &str = "large-payload-utf8.md";
pub const EXPECTED_BYTES: usize = 311496;
pub const EXPECTED_SHA256: &str =
    "b0f6c626ce6223d2efe1c3cbf0ec928b60fe49e003ad6d417a4d7c17c668cf5b";
const SECTIONS: usize = 128;
const RECORDS_PER_SECTION: usize = 16;
const INLINE_LIMIT: usize = 48 * 1024;

fn markdown() -> String {
    let mut text = String::from(
        "# Encrypted page payload fixture\n\nEste catálogo sintético cobre UTF-8, estrutura Markdown e dados distintos; não contém contas nem segredos.\n\n",
    );
    let mut state = 0x5eed_cafe_2040_0040_u64;
    for section in 0..SECTIONS {
        write!(text, "## Etapa {section:03} — café, 東京 e 🌿\n\n").unwrap();
        write!(text, "Roteiro {section:03}: **ação** em São Paulo → Lisboa; *cafe\u{301}* preserva a forma decomposta.  \nPróxima linha com `código` e pontuação: “sim”, não.\n\n").unwrap();
        write!(
            text,
            "- [ ] Conferir registro {section:03}\n- [x] Preservar bytes e ordem\n\n"
        )
        .unwrap();
        write!(text, "| Campo | Valor |\n|:------|------:|\n| Etapa | {section:03} |\n| Cidade | 東京 |\n\n```json\n[\n").unwrap();
        for row in 0..RECORDS_PER_SECTION {
            let mut sample = String::new();
            for _ in 0..4 {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                write!(sample, "{state:016x}").unwrap();
            }
            let comma = if row + 1 < RECORDS_PER_SECTION {
                ","
            } else {
                ""
            };
            writeln!(text, "  {{\"record\":\"{section:03}-{row:02}\",\"sample\":\"{sample}\",\"label\":\"ação → 東京\"}}{comma}").unwrap();
        }
        text.push_str("]\n```\n\n> Dados determinísticos de teste; nenhuma identidade real.\n\n");
    }
    writeln!(
        text,
        "Fim do catálogo: {SECTIONS} etapas, {RECORDS_PER_SECTION} registros por etapa."
    )
    .unwrap();
    text
}

pub fn verify_bytes(bytes: &[u8]) {
    assert_eq!(
        bytes.len(),
        EXPECTED_BYTES,
        "large-page fixture size changed"
    );
    assert_eq!(
        hex::encode(Sha256::digest(bytes)),
        EXPECTED_SHA256,
        "large-page fixture bytes changed"
    );
}

fn checked_page() -> (String, Vec<u8>) {
    let markdown = markdown();
    verify_bytes(markdown.as_bytes());
    let page = Page::from_markdown(&markdown).expect("valid fixture Markdown");
    assert_eq!(
        page.to_markdown(),
        markdown,
        "fixture import preserves exact UTF-8"
    );
    let snapshot = page.snapshot();
    assert!(
        snapshot.len() > INLINE_LIMIT,
        "actual serialized Page must exceed the external-payload boundary: {} bytes",
        snapshot.len()
    );
    (markdown, snapshot)
}

pub fn prepared_markdown() -> String {
    checked_page().0
}

#[test]
fn fixture_has_fixed_utf8_bytes_and_an_external_snapshot_roundtrip() {
    let (markdown, snapshot) = checked_page();
    assert!(markdown.contains("café, 東京 e 🌿"));
    assert!(markdown.contains("cafe\u{301}"));
    assert!(markdown.contains("- [ ]") && markdown.contains("- [x]"));
    assert!(markdown.contains("```json"));
    let remote = Page::new();
    remote.import(&snapshot).expect("complete fixture snapshot");
    let restored = remote.to_markdown();
    verify_bytes(restored.as_bytes());
    assert_eq!(restored, markdown);
}

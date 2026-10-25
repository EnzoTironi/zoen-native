//! Versioned document and file contracts.

use crate::{ItemId, ReplyRef, SpaceId};
use serde::{Deserialize, Serialize};

// ───────────────────────────── Item ─────────────────────────────

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ItemKind {
    Plan,
    Task,
    Note,
    /// Mini-app (MCP App): uma interface interativa cujo estado é este Item.
    App,
    /// A Zoen page: blocks in a Loro document (ADR 0040).
    Page,
    /// A file: bytes in encrypted chunks on the relay (ADR 0040).
    File,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanLine {
    pub id: String,
    pub text: String,
    /// Centavos de real. Inteiros, nunca float.
    pub cost_cents: i64,
    pub done: bool,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanSection {
    pub title: String,
    pub lines: Vec<PlanLine>,
}

/// Um plano/documento estruturado. No v1 completo isto vira um documento Loro (CRDT);
/// aqui a versão inteira vai em cada evento, o que mantém Versões/Desfazer triviais.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PlanDoc {
    pub title: String,
    pub summary: String,
    pub budget_cents: Option<i64>,
    pub sections: Vec<PlanSection>,
}

impl PlanDoc {
    #[must_use]
    pub fn total_cents(&self) -> i64 {
        self.sections
            .iter()
            .flat_map(|s| &s.lines)
            .map(|l| l.cost_cents)
            .sum()
    }

    #[must_use]
    pub fn line(&self, id: &str) -> Option<&PlanLine> {
        self.sections
            .iter()
            .flat_map(|s| &s.lines)
            .find(|l| l.id == id)
    }

    pub fn line_mut(&mut self, id: &str) -> Option<&mut PlanLine> {
        self.sections
            .iter_mut()
            .flat_map(|s| s.lines.iter_mut())
            .find(|l| l.id == id)
    }

    pub fn remove_line(&mut self, id: &str) -> Option<PlanLine> {
        for s in &mut self.sections {
            if let Some(pos) = s.lines.iter().position(|l| l.id == id) {
                return Some(s.lines.remove(pos));
            }
        }
        None
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum ItemContent {
    Text { text: String },
    Plan(PlanDoc),
    App(AppDoc),
    Page(PageContent),
    File(FileDoc),
}

/// One version of a page: the first carries a Loro snapshot, later ones the updates made
/// since the previous version. Small payloads ride in the event (base64); large ones go to
/// an encrypted blob.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct PageContent {
    pub title: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub loro: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blob: Option<ChunkRef>,
    /// Absent for ordinary binary Loro pages. Unknown/malformed manifests stay
    /// inspectable so a typed fold can refuse them without losing version history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manifest: Option<PageManifest>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum PageManifest {
    PlanV1(PlanManifestV1),
    Unsupported(serde_json::Value),
}

/// A claim about a locally verifiable prefix, never permission to write or a CAS.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlanManifestV1 {
    pub schema: String,
    pub space: SpaceId,
    pub item: ItemId,
    pub creation_event: String,
    pub legacy_versions: u32,
    pub legacy_head: String,
    pub prefix_sha256: String,
}

/// One encrypted piece of a file on the relay. `key` opens the copy stored under `blob`
/// (sha256 of the ciphertext); `sha256` is the plaintext's hash.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct ChunkRef {
    pub sha256: String,
    pub bytes: u64,
    pub blob: String,
    pub key: String,
}

/// One version of a file: its bytes as content-defined chunks (a new version re-uses the
/// chunks that didn't change), plus an optional preview made on the device.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct FileDoc {
    /// Origin of the App referenced by this owned manifest, not decoding, bytes,
    /// historical access or a grant. Absent on regular/legacy files.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub app_origin: Option<crate::AppItemOrigin>,
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    pub mime: String,
    pub bytes: u64,
    pub sha256: String,
    pub chunks: Vec<ChunkRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thumb: Option<ChunkRef>,
    /// Set when the file is a voice note: the chat shows a player instead of a file card.
    /// Absent on every other file, so their bytes are exactly what older peers expect.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<VoiceMeta>,
}

/// What a voice note's player needs before the audio arrives.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, Default)]
pub struct VoiceMeta {
    /// Length in milliseconds.
    pub ms: u64,
    /// Waveform bars, one hex digit (0–f) per bar.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub levels: String,
    /// Transcribed on the sender's device (may be empty).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub transcript: String,
    /// Keep the note and its reply/thread destination in the same signed event.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<ReplyRef>,
    /// A copied voice note is one signed file/message, without a source-chat reference.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub forwarded: bool,
}

impl VoiceMeta {
    /// Bars as 0…1.
    #[must_use]
    pub fn bars(&self) -> Vec<f32> {
        self.levels
            .chars()
            .take(160)
            .filter_map(|c| c.to_digit(16).and_then(|digit| u8::try_from(digit).ok()))
            .map(|digit| f32::from(digit) / 15.0)
            .collect()
    }

    /// 0…1 bars as hex digits (at most 160).
    #[must_use]
    pub fn encode(levels: &[f32]) -> String {
        levels
            .iter()
            .take(160)
            .map(|l| {
                let rounded = (l.clamp(0.0, 1.0) * 15.0).round();
                for digit in (0_u8..=15).rev() {
                    if rounded >= f32::from(digit) {
                        return char::from(b"0123456789abcdef"[usize::from(digit)]);
                    }
                }
                // A NaN sample keeps the existing silent-bar wire encoding.
                '0'
            })
            .collect()
    }
}

impl FileDoc {
    /// A voice note: tagged as one, or an audio file recorded under `VoiceNotes/` (older
    /// web builds sent recordings named "Mensagem de voz.*" without the tag).
    #[must_use]
    pub fn is_voice(&self) -> bool {
        self.mime.starts_with("audio/")
            && (self.voice.is_some()
                || self.path.starts_with("VoiceNotes/")
                || self.name.starts_with("Mensagem de voz"))
    }
}

impl ItemContent {
    /// A file or a typed document owned by an app; never a foreign Item reference.
    #[must_use]
    pub fn owned_file(&self) -> Option<&FileDoc> {
        match self {
            Self::File(file) => Some(file),
            Self::App(app) => app.document.as_ref(),
            _ => None,
        }
    }

    #[must_use]
    pub fn title(&self) -> String {
        match self {
            ItemContent::Text { text } => text.lines().next().unwrap_or_default().to_string(),
            ItemContent::Plan(p) => p.title.clone(),
            ItemContent::App(a) => a.title.clone(),
            ItemContent::Page(p) => p.title.clone(),
            ItemContent::File(f) => f.name.clone(),
        }
    }
}

/// Estado de um mini-app (MCP App) dentro de um Espaço. A interface vem de um
/// recurso `ui://`; o estado compartilhado mora aqui, versionado e assinado como
/// qualquer Item: todo mundo vê o mesmo bichinho e quem fez o quê.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub struct AppDoc {
    /// Qual mini-app (ex.: "pet", "poll", "list").
    pub app: String,
    /// Recurso de interface (`ui://…`), como no MCP Apps.
    pub resource_uri: String,
    pub title: String,
    /// Estado em JSON canônico (o servidor do mini-app define o formato).
    pub state_json: String,
    /// Optional owned document. Its encrypted chunks share this App Item's history.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document: Option<FileDoc>,
}

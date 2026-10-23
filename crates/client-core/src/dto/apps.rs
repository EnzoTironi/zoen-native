//! Existing public DTO declarations, grouped by domain.
use super::*;

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AppToolDto {
    pub name: String,
    pub title: String,
    pub description: String,
    pub input_schema_json: String,
    /// `model` (o agente pode chamar) e/ou `app` (a interface pode chamar).
    pub visibility: Vec<String>,
    /// Classe da ação para o avaliador de Concessões (Reversível, Externa…).
    pub action_label: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AppSpecDto {
    pub id: String,
    pub name: String,
    pub description: String,
    pub resource_uri: String,
    /// Tem View MCP (HTML em `resources/read`). Sem View = só interface nativa.
    pub has_view: bool,
    pub tools: Vec<AppToolDto>,
}

/// Conteúdo de `resources/read` para um recurso `ui://`.
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AppResourceDto {
    pub uri: String,
    pub mime_type: String,
    pub text: String,
    pub prefers_border: bool,
    /// Mini-app manifest (JSON: capabilities with purposes, allowed domains, sha256 of
    /// `text`). Empty for the older hand-written apps. The host refuses a bundle whose hash
    /// doesn't match.
    pub manifest_json: String,
}

/// A native capability a mini-app holds (Settings → Mini-app permissions).
#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct DeviceGrantDto {
    pub grant_id: String,
    pub item_id: String,
    pub item_title: String,
    pub app_name: String,
    pub space_title: String,
    pub capability: String,
    pub purpose: String,
    /// `false` = "Allow once" (expires in a minute).
    pub always: bool,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AppMetricDto {
    pub label: String,
    /// 0…1
    pub value: f64,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AppStateDto {
    pub app_id: String,
    pub resource_uri: String,
    pub name: String,
    /// Estado como a interface deve ver agora (vai em `structuredContent`).
    pub view_json: String,
    pub headline: String,
    pub metrics: Vec<AppMetricDto>,
    pub last_action: Option<String>,
    /// Nível da Concessão do mini-app neste Item.
    pub trust: TrustLevelDto,
    /// Widget snapshot JSON (native template for the Home strip and WidgetKit); empty = none.
    pub snapshot_json: String,
}

/// The canonical, content-only projection used by native widgets and shortcuts.
#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct NativeSurfaceAppDto {
    pub app_id: String,
    pub snapshot_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct NativeSurfaceItemDto {
    pub id: String,
    pub space_id: String,
    pub title: String,
    pub version: u32,
    pub app: NativeSurfaceAppDto,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, uniffi::Enum, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub enum AppCallStatus {
    /// Rodou (ou era só leitura).
    Done,
    /// Irreversível ou externo: o app nativo confirma com você antes.
    NeedsConfirmation,
    /// A Concessão não cobre, nem pedindo.
    Denied,
}

#[derive(Debug, Clone, PartialEq, uniffi::Record, serde::Serialize, serde::Deserialize)]
#[cfg_attr(feature = "client-types", derive(ts_rs::TS))]
pub struct AppCallOutcome {
    pub status: AppCallStatus,
    /// `CallToolResult` do MCP em JSON (content + structuredContent + isError).
    pub result_json: String,
    pub message: String,
    pub confirm_title: Option<String>,
    pub confirm_detail: Option<String>,
    pub item: Option<ItemDetail>,
}

//! # Servidor MCP local dos mini-apps embutidos
//!
//! Implementa o lado *servidor* do MCP Apps (SEP-1865, versão `2026-01-26`) para
//! os mini-apps que vêm dentro do app, com estado canônico no núcleo:
//!
//! - **pet**: um bichinho do grupo (o burro do Wabi, em pixel art). Comer, dormir, brincar.
//! - **poll**: enquete que, ao fechar, vira uma linha do plano do Espaço.
//! - **list**: lista compartilhada ("o que levar").
//! - **maptap**: jogo de geografia por turnos (os mesmos 5 lugares para todo mundo).
//! - **recipe**: receita do jantar com porções e modo cozinhar.
//! - **countdown**: contagem regressiva do grupo (viagem, show).
//!
//! As interfaces usam o SDK oficial MCP Apps em recursos HTML íntegros e isolados.
//! Ferramentas e Concessões continuam autoritativas no núcleo.
//!
//! Cada mini-app declara um recurso `ui://builtin/<app>` (`text/html;profile=mcp-app`)
//! e ferramentas com `_meta.ui.resourceUri` e `visibility` (`model` = o agente cria,
//! `app` = a interface chama). O estado compartilhado mora num Item versionado; quem
//! decide se uma chamada pode rodar é o `grants` (no `engine`), não este módulo.

use types::{ActionClass, AppDoc, ItemContent};

use localization as i18n;
use localization::{is_en, t, thousands, tr, ts};
use serde_json::{json, Value};

pub mod binary;
mod catalog;
mod creation;
pub mod deckcraft;
mod defaults;
pub mod excalidraw;
mod execution;
pub mod gridcraft;
pub mod photocraft;
mod projection;
mod schedule;
mod scoped;
pub mod wordcraft;

pub use catalog::{spec, spec_for_start_tool, spec_for_uri, specs};
pub use creation::create;
pub use defaults::{haversine_km, maptap_board, HIKE_TRAILS};
pub use execution::apply;
pub use projection::{headline, last_action, snapshot, view};
pub use schedule::Actor;
pub use scoped::{
    apply_as, create_as, owner_note_for, pin_client, reconcile_concurrent, replayed, view_for,
};

use catalog::tool;
use defaults::{clock, hike_name, maptap_places, recipe_default, st_day};
use projection::pet_mood;

pub const MIME: &str = "text/html;profile=mcp-app";

pub struct ToolSpec {
    pub name: &'static str,
    pub title: &'static str,
    pub description: &'static str,
    pub schema: Value,
    pub visibility: Vec<&'static str>,
    pub action: ActionClass,
    pub read_only: bool,
}

pub struct AppSpec {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub resource_uri: &'static str,
    /// View MCP Apps (HTML) empacotada e verificada pelo host.
    pub html: Option<&'static str>,
    /// Built bundles (tools/miniapp-build) ship a manifest with their sha256.
    pub manifest: Option<&'static str>,
    pub tools: Vec<ToolSpec>,
}

/// Efeito fora do próprio Item (o engine executa, com o mesmo crivo de Concessão).
pub enum Effect {
    AddPlanLine { text: String, cost_cents: i64 },
}

pub struct Applied {
    pub state: Value,
    pub note: String,
    pub text: String,
    pub effect: Option<Effect>,
    /// Novo título do Item (ex.: o grupo renomeou o bichinho).
    pub title: Option<String>,
    /// O agente comenta o evento na conversa (ex.: novo recorde).
    pub comment: Option<String>,
}

impl Applied {
    fn new(state: Value, note: String, text: String) -> Self {
        Applied {
            state,
            note,
            text,
            effect: None,
            title: None,
            comment: None,
        }
    }
}

fn s(v: &Value, k: &str) -> Option<String> {
    v.get(k)
        .and_then(|x| x.as_str())
        .map(|x| x.trim().to_string())
        .filter(|x| !x.is_empty())
}

fn clamp(x: f64) -> f64 {
    x.clamp(0.0, 100.0)
}

fn push_log(state: &mut Value, who: &str, what: &str, now: i64) {
    let log = state["log"]
        .as_array_mut()
        .map(std::mem::take)
        .unwrap_or_default();
    let mut log: Vec<Value> = log;
    log.insert(0, json!({ "who": who, "what": what, "at": now }));
    log.truncate(8);
    state["log"] = Value::Array(log);
}

/// Resultado no formato `CallToolResult` do MCP.
pub fn call_tool_result(text: &str, structured: &Value, is_error: bool) -> Value {
    json!({ "content": [{ "type": "text", "text": text }], "structuredContent": structured, "isError": is_error })
}

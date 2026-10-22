//! Agenda resource and tool schema.

use super::*;

pub(super) fn spec() -> AppSpec {
    use ActionClass::*;
    AppSpec {
            id: "schedule",
            name: ts("Agenda", "Schedule"),
            description: ts(
                "Horários livres, serviços e preços. O cliente marca; o agente confirma o que for especial.",
                "Free slots, services and prices. The client books; the agent confirms anything special.",
            ),
            resource_uri: "ui://builtin/schedule",
            html: Some(include_str!("../../resources/schedule.html")),
            manifest: Some(include_str!("../../resources/schedule.manifest.json")),
            tools: vec![
                tool(
                    "start_schedule",
                    ts("Abrir agenda", "Open schedule"),
                    ts(
                        "Abre uma agenda para qualquer negócio. Preencha nome, serviços, preços e horários a partir do que o dono contou.",
                        "Opens a schedule for any business. Fill in the name, services, prices and hours from what the owner said.",
                    ),
                    json!({
                        "type": "object",
                        "properties": {
                            "business": {"type": "string", "maxLength": 48, "description": "Business name in the owner's words"},
                            "title": {"type": "string", "maxLength": 40},
                            "accentHex": {"type": "string", "pattern": "^#[0-9A-Fa-f]{6}$"},
                            "timezone": {"type": "string", "maxLength": 40, "description": "IANA zone, e.g. America/Sao_Paulo"},
                            "currency": {"type": "string", "pattern": "^[A-Za-z]{3}$"},
                            "slotMinutes": {"type": "integer", "minimum": 5, "maximum": 240},
                            "horizonDays": {"type": "integer", "minimum": 1, "maximum": 365},
                            "services": {
                                "type": "array",
                                "minItems": 1,
                                "maxItems": 12,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "id": {"type": "string", "maxLength": 24},
                                        "name": {"type": "string", "maxLength": 40},
                                        "priceCents": {"type": "integer", "minimum": 0},
                                        "minutes": {"type": "integer", "minimum": 5, "maximum": 480}
                                    },
                                    "required": ["name", "minutes"]
                                }
                            },
                            "weeklyHours": {
                                "type": "object",
                                "description": "Keys \"0\" (Sunday) to \"6\"; each a list of [\"HH:MM\", \"HH:MM\"] windows",
                                "additionalProperties": false,
                                "patternProperties": {"^[0-6]$": {"type": "array", "maxItems": 4, "items": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "string", "pattern": "^([01][0-9]|2[0-3]):[0-5][0-9]$|^24:00$"}}}}
                            }
                        },
                        "required": ["business", "services", "weeklyHours"]
                    }),
                    &["model"],
                    Reversible,
                    false,
                ),
                tool(
                    "schedule_configure",
                    ts("Ajustar agenda", "Configure schedule"),
                    ts(
                        "Dono ou agente: atualiza nome, serviços, horários, fuso ou quem resolve pedidos. Entrada inválida é recusada.",
                        "Owner or agent: updates the name, services, hours, timezone or who resolves requests. Invalid input is refused.",
                    ),
                    json!({
                        "type": "object",
                        "properties": {
                            "business": {"type": "string", "maxLength": 48, "description": "Business name in the owner's words"},
                            "title": {"type": "string", "maxLength": 40},
                            "accentHex": {"type": "string", "pattern": "^#[0-9A-Fa-f]{6}$"},
                            "timezone": {"type": "string", "maxLength": 40, "description": "IANA zone, e.g. America/Sao_Paulo"},
                            "currency": {"type": "string", "pattern": "^[A-Za-z]{3}$"},
                            "slotMinutes": {"type": "integer", "minimum": 5, "maximum": 240},
                            "horizonDays": {"type": "integer", "minimum": 1, "maximum": 365},
                            "services": {
                                "type": "array",
                                "minItems": 1,
                                "maxItems": 12,
                                "items": {
                                    "type": "object",
                                    "properties": {
                                        "id": {"type": "string", "maxLength": 24},
                                        "name": {"type": "string", "maxLength": 40},
                                        "priceCents": {"type": "integer", "minimum": 0},
                                        "minutes": {"type": "integer", "minimum": 5, "maximum": 480}
                                    },
                                    "required": ["name", "minutes"]
                                }
                            },
                            "weeklyHours": {
                                "type": "object",
                                "description": "Keys \"0\" (Sunday) to \"6\"; each a list of [\"HH:MM\", \"HH:MM\"] windows",
                                "additionalProperties": false,
                                "patternProperties": {"^[0-6]$": {"type": "array", "maxItems": 4, "items": {"type": "array", "minItems": 2, "maxItems": 2, "items": {"type": "string", "pattern": "^([01][0-9]|2[0-3]):[0-5][0-9]$|^24:00$"}}}}
                            },
                            "resolvers": {"type": "array", "maxItems": 8, "items": {"type": "string"}, "description": "Member ids who may resolve requests (owner only)"}
                        }
                    }),
                    &["model", "app"],
                    Reversible,
                    false,
                ),
                tool(
                    "schedule_book",
                    ts("Pedir horário", "Request a slot"),
                    ts(
                        "Pede um horário livre. Vira um evento para o agente confirmar ou ajustar.",
                        "Requests a free slot. Becomes an event for the agent to confirm or adjust.",
                    ),
                    json!({
                        "type": "object",
                        "properties": {
                            "serviceId": {"type": "string"},
                            "startMs": {"type": "integer"},
                            "endMs": {"type": "integer", "description": "Optional; must equal startMs + the service length"},
                            "note": {"type": "string", "maxLength": 120},
                            "forId": {"type": "string", "description": "Staff or agent only: the member this booking is for"},
                            "forName": {"type": "string", "maxLength": 48, "description": "Staff or agent only: the client's name"}
                        },
                        "required": ["serviceId", "startMs"]
                    }),
                    &["app"],
                    Reversible,
                    false,
                ),
                tool(
                    "schedule_resolve",
                    ts("Resolver pedido", "Resolve request"),
                    ts(
                        "Dono, agente ou quem resolve: confirma, remarca ou cancela um pedido; num pedido de cancelamento, cancela ou mantém (keep).",
                        "Owner, agent or resolver: confirms, moves or cancels a request; on a cancel request, cancels or keeps it.",
                    ),
                    json!({
                        "type": "object",
                        "properties": {
                            "bookingId": {"type": "string"},
                            "action": {"type": "string", "enum": ["confirm", "adjust", "cancel", "keep"]},
                            "startMs": {"type": "integer"},
                            "endMs": {"type": "integer"},
                            "note": {"type": "string", "maxLength": 120}
                        },
                        "required": ["bookingId", "action"]
                    }),
                    &["model", "app"],
                    Reversible,
                    false,
                ),
                tool(
                    "schedule_cancel",
                    ts("Cancelar horário", "Cancel booking"),
                    ts(
                        "Pede o cancelamento do seu horário. O agente recebe o evento.",
                        "Requests cancelling your booking. The agent gets the event.",
                    ),
                    json!({
                        "type": "object",
                        "properties": {
                            "bookingId": {"type": "string"}
                        },
                        "required": ["bookingId"]
                    }),
                    &["app"],
                    Reversible,
                    false,
                ),
                tool(
                    "schedule_get_state",
                    ts("Ver estado", "Get state"),
                    ts(
                        "Lê serviços, horários e marcações (sem mudar nada).",
                        "Reads services, hours and bookings (changes nothing).",
                    ),
                    empty(),
                    &["app"],
                    Reversible,
                    true,
                ),
            ],
        }
}

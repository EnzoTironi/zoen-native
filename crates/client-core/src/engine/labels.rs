//! Stable domain labels and plan DTO conversion.
use super::{
    money, new_id, t, tr, ts, ActionClass, Capability, EventBody, ItemKind, PlanDoc, PlanDto,
    PlanLine, PlanLineDto, PlanSection, PlanSectionDto, Policy, Reason, TrustLevel, TrustLevelDto,
    R,
};

pub(super) trait LowerFirst {
    fn to_lowercase_first(&self) -> String;
}
impl LowerFirst for String {
    fn to_lowercase_first(&self) -> String {
        let mut c = self.chars();
        match c.next() {
            Some(f) => f.to_lowercase().collect::<String>() + c.as_str(),
            None => String::new(),
        }
    }
}

pub(crate) fn kind_id(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Plan => "plan",
        ItemKind::Task => "task",
        ItemKind::Note => "note",
        ItemKind::App => "app",
        ItemKind::Page => "page",
        ItemKind::File => "file",
    }
}

pub(crate) fn kind_label(k: ItemKind) -> &'static str {
    match k {
        ItemKind::Plan => ts("Plano", "Plan"),
        ItemKind::Task => ts("Tarefa", "Task"),
        ItemKind::Note => ts("Nota", "Note"),
        ItemKind::App => ts("App", "App"),
        ItemKind::Page => ts("Página", "Page"),
        ItemKind::File => ts("Arquivo", "File"),
    }
}

pub(super) fn action_label(a: &ActionClass) -> &'static str {
    match a {
        ActionClass::Reply => ts("Responder", "Reply"),
        ActionClass::Reversible => ts("Editar no Espaço", "Edit in the Space"),
        ActionClass::External => ts("Enviar para fora", "Send outside"),
        ActionClass::Irreversible => ts("Apagar de verdade", "Delete for real"),
        ActionClass::Money { .. } => ts("Pagamento", "Payment"),
        ActionClass::PublicAudience => ts("Publicar", "Publish"),
        ActionClass::ThirdPartyData => ts("Dados de terceiros", "Third-party data"),
    }
}

/// Plain words for a standing decision's kind of action (`grants::standing_key`).
pub(crate) fn standing_label(key: &str) -> String {
    match key {
        "reply" => t("responder", "replies"),
        "reversible" => t("mudanças neste Espaço", "changes in this Space"),
        "external" => t("enviar para fora do Zoen", "sending outside Zoen"),
        "irreversible" => t("apagar de vez", "deleting for good"),
        "money" => t("pagamentos", "payments"),
        "public_audience" => t("publicar para mais gente", "posting to a wider audience"),
        "third_party_data" => t("dados de outras pessoas", "other people's data"),
        other => other.to_string(),
    }
}

pub(crate) fn reason_label(r: &Reason, p: &Policy) -> String {
    match r {
        Reason::TrustTooLow { needed } => tr!(
            "Precisa do nível {}",
            "Needs the {} level",
            trust_label(*needed)
        ),
        Reason::LeavesTheSpace => t(
            "Sai do Espaço ou não tem desfazer: pede antes",
            "Leaves the Space or can’t be undone: asks first",
        ),
        Reason::MoneyAboveCeiling { .. } => tr!(
            "Linha vermelha: dinheiro acima de {} sempre pede",
            "Red line: money above {} always asks",
            money(p.money_ceiling_cents)
        ),
        Reason::NewPublicAudience => t(
            "Linha vermelha: audiência pública nova sempre pede",
            "Red line: a new public audience always asks",
        ),
        Reason::ThirdPartyData => t(
            "Linha vermelha: dados de terceiros sempre pedem",
            "Red line: third-party data always asks",
        ),
        Reason::OverBudget { remaining_cents } => tr!(
            "Orçamento do mês no fim (resta {})",
            "Monthly budget almost gone ({} left)",
            money(*remaining_cents)
        ),
        Reason::StandingDeny => t(
            "Você escolheu sempre negar isso deste agente aqui",
            "You chose to always deny this from this agent here",
        ),
    }
}

pub(crate) fn trust_label(l: TrustLevel) -> &'static str {
    match l {
        TrustLevel::Listen => ts("Ouvir", "Listen"),
        TrustLevel::Suggest => ts("Sugerir", "Suggest"),
        TrustLevel::Act => ts("Agir", "Act"),
        TrustLevel::Autonomous => ts("Autônomo", "Autonomous"),
    }
}

pub(crate) fn trust_dto(l: TrustLevel) -> TrustLevelDto {
    match l {
        TrustLevel::Listen => TrustLevelDto::Listen,
        TrustLevel::Suggest => TrustLevelDto::Suggest,
        TrustLevel::Act => TrustLevelDto::Act,
        TrustLevel::Autonomous => TrustLevelDto::Autonomous,
    }
}

pub(crate) fn trust_from_dto(l: TrustLevelDto) -> TrustLevel {
    match l {
        TrustLevelDto::Listen => TrustLevel::Listen,
        TrustLevelDto::Suggest => TrustLevel::Suggest,
        TrustLevelDto::Act => TrustLevel::Act,
        TrustLevelDto::Autonomous => TrustLevel::Autonomous,
    }
}

pub(crate) fn event_label(b: &EventBody) -> String {
    match b {
        EventBody::SpaceCreated { title, .. } => {
            tr!("Espaço criado: {title}", "Space created: {title}")
        }
        EventBody::MemberAdded { .. } => t("Membro adicionado", "Member added"),
        EventBody::MemberRemoved { .. } => t("Membro removido", "Member removed"),
        EventBody::MessagePosted {
            attaches: Some(_), ..
        } => t("Mensagem com Item", "Message with Item"),
        EventBody::MessagePosted { .. } | EventBody::AppEditRequested { .. } => {
            t("Mensagem", "Message")
        }
        EventBody::AppCurrentShared { .. } => t("App compartilhado", "App shared"),
        EventBody::CallTransportV1 { .. } => t("Conexão da chamada", "Call connection"),
        EventBody::CallV1 { .. } => t("Chamada", "Call"),
        EventBody::ChatPrefsSet { .. } => t("Preferências da conversa", "Chat preferences"),
        EventBody::MessagesDeletedForSelf { .. } => t("Exclusão privada", "Private deletion"),
        EventBody::MessageAnnotated { note, .. } => match note {
            types::Annotation::Edit { .. } => t("Mensagem editada", "Message edited"),
            types::Annotation::Delete => t("Mensagem apagada", "Message deleted"),
            types::Annotation::React { .. } | types::Annotation::Unreact => t("Reação", "Reaction"),
        },
        EventBody::ItemCreated { content, .. } => {
            tr!("Item criado: {}", "Item created: {}", content.title())
        }
        EventBody::ItemVersioned { note, .. } => tr!("Nova versão: {note}", "New version: {note}"),
        EventBody::ItemReverted { to_version, .. } => {
            tr!("Restaurou a v{to_version}", "Restored v{to_version}")
        }
        EventBody::GrantIssued { grant } => match &grant.capability {
            Capability::Trust(l) => tr!(
                "Concessão: confiança {}",
                "Grant: trust {}",
                trust_label(*l)
            ),
            Capability::MonthlyBudget { cents } => tr!(
                "Concessão: orçamento {}/mês",
                "Grant: budget {}/month",
                money(*cents)
            ),
            Capability::Invite { .. } => t("Concessão: link de convite", "Grant: invite link"),
            Capability::Device { capability, .. } => tr!(
                "Concessão: {} para um app",
                "Grant: {} for an app",
                capability
            ),
            Capability::ConsentMode { mode } => tr!(
                "Permissões: {}",
                "Permissions: {}",
                consent_mode_label(*mode)
            ),
            Capability::AutoApproved { capability, .. } => tr!(
                "Aprovado automaticamente: {}",
                "Auto-approved: {}",
                capability
            ),
            Capability::Standing { action, allow } => {
                if *allow {
                    tr!(
                        "Sempre aprovar: {}",
                        "Always approve: {}",
                        standing_label(action)
                    )
                } else {
                    tr!(
                        "Sempre negar: {}",
                        "Always deny: {}",
                        standing_label(action)
                    )
                }
            }
        },
        EventBody::GrantRevoked { .. } => t("Concessão revogada", "Grant revoked"),
        EventBody::RequestOpened { request } => tr!("Pedido: {}", "Request: {}", request.title),
        EventBody::ToolReviewRequested { .. } => {
            t("Nova revisão solicitada", "New review requested")
        }
        EventBody::RequestResolved { approved, .. } => {
            if *approved {
                t("Pedido aprovado", "Request approved")
            } else {
                t("Pedido recusado", "Request declined")
            }
        }
        EventBody::OwnerPolicyOptionsRequested { .. } => {
            t("Consulta de limite de IA", "AI spending policy inquiry")
        }
        EventBody::OwnerPolicyOptionsRefused { .. } => t(
            "Consulta de limite de IA recusada",
            "AI spending policy inquiry refused",
        ),
        EventBody::OwnerPolicyOptionsReported { .. } => {
            t("Opções do limite de IA", "AI spending policy options")
        }
        EventBody::OwnerPolicySubmitted { .. } => {
            t("Limite de IA enviado", "AI spending policy submitted")
        }
        EventBody::OwnerPolicyReported { .. } => {
            t("Resposta ao limite de IA", "AI spending policy response")
        }
        EventBody::UsageRecorded { cents, .. } => {
            tr!("Uso de IA: {}", "AI usage: {}", money(*cents))
        }
        EventBody::BackgroundSet { background } => match &background.media {
            Some(m) => tr!(
                "Fundo: foto {}…",
                "Background: photo {}…",
                &m.sha256[..m.sha256.len().min(12)]
            ),
            None => tr!("Fundo: {}", "Background: {}", background.style),
        },
        EventBody::ProfileKeyShared { shares, .. } => tr!(
            "Chave de perfil compartilhada com {}",
            "Profile key shared with {}",
            shares.len()
        ),
        EventBody::Sealed { kind } => tr!("Cifrado: {kind}", "Encrypted: {kind}"),
        EventBody::SpaceEncrypted => t(
            "Criptografia de ponta a ponta ativada",
            "End-to-end encryption turned on",
        ),
        EventBody::Checkpoint { epoch, .. } => {
            tr!(
                "Ponto de verificação: época {epoch}",
                "Checkpoint: epoch {epoch}"
            )
        }
        EventBody::DeviceJoining { .. } => t(
            "Um aparelho entrou na criptografia",
            "A device joined the encryption",
        ),
        EventBody::Unsupported { kind } => tr!(
            "Evento de uma versão mais nova: {kind}",
            "Event from a newer version: {kind}"
        ),
    }
}

pub(super) fn consent_mode_label(m: types::ConsentMode) -> String {
    match m {
        types::ConsentMode::Ask => t("Perguntar", "Ask"),
        types::ConsentMode::Auto => t("Automático", "Auto"),
        types::ConsentMode::Trusted => t("Confiável", "Trusted"),
    }
}

pub(crate) fn plan_from_dto(p: PlanDto) -> R<PlanDoc> {
    let mut sections = Vec::new();
    for section in p.sections {
        let mut lines = Vec::new();
        for line in section.lines {
            if line.text.trim().is_empty() {
                continue;
            }
            lines.push(PlanLine {
                id: if line.id.is_empty() {
                    new_id("ln")?
                } else {
                    line.id
                },
                text: line.text.trim().to_string(),
                cost_cents: line.cost_cents.max(0),
                done: line.done,
            });
        }
        if !lines.is_empty() {
            sections.push(PlanSection {
                title: section.title,
                lines,
            });
        }
    }
    Ok(PlanDoc {
        title: p.title.trim().to_string(),
        summary: p.summary.trim().to_string(),
        budget_cents: p.budget_cents.filter(|budget| *budget > 0),
        sections,
    })
}

pub(crate) fn plan_to_dto(p: &PlanDoc, total_cents: i64) -> PlanDto {
    PlanDto {
        title: p.title.clone(),
        summary: p.summary.clone(),
        budget_cents: p.budget_cents,
        total_cents,
        sections: p
            .sections
            .iter()
            .map(|s| PlanSectionDto {
                title: s.title.clone(),
                lines: s
                    .lines
                    .iter()
                    .map(|l| PlanLineDto {
                        id: l.id.clone(),
                        text: l.text.clone(),
                        cost_cents: l.cost_cents,
                        done: l.done,
                    })
                    .collect(),
            })
            .collect(),
    }
}

//! # roda-grants
//!
//! O modelo de confiança do Roda em código puro (sem I/O), para rodar igual no
//! aparelho, no servidor e no runtime de agentes. No produto completo as políticas
//! viram Cedar; a regra aqui é a mesma do documento:
//!
//! 1. **Reversível → faz e mostra "Desfazer".**
//! 2. **Irreversível ou externo → pede.**
//! 3. **Linhas vermelhas fixas:** dinheiro acima do teto, nova audiência pública e
//!    dados de terceiros **sempre** pedem, em qualquer nível.
//! 4. Orçamento: alerta aos 80 %, parada no teto.

use roda_types::{ActionClass, TrustLevel};
use serde::{Deserialize, Serialize};

pub mod consent;

/// Por que um pedido foi aberto (ou uma ação bloqueada).
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Reason {
    /// O nível de confiança atual não cobre esta ação.
    TrustTooLow { needed: TrustLevel },
    /// Ação externa ou irreversível.
    LeavesTheSpace,
    /// Linha vermelha: dinheiro acima do teto.
    MoneyAboveCeiling { ceiling_cents: i64 },
    /// Linha vermelha: audiência pública nova.
    NewPublicAudience,
    /// Linha vermelha: dados de terceiros.
    ThirdPartyData,
    /// O orçamento de IA do mês acabou.
    OverBudget { remaining_cents: i64 },
    /// O dono decidiu "sempre negar" este tipo de pedido deste agente aqui.
    StandingDeny,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
pub enum Decision {
    /// Faz agora. `undoable` = mostra "Desfazer".
    Act { undoable: bool },
    /// Vira um pedido em Atividade.
    Request(Reason),
    /// Não pode, nem pedindo (ex.: orçamento esgotado → "Aumentar limite").
    Block(Reason),
}

impl Decision {
    pub fn acts(&self) -> bool {
        matches!(self, Decision::Act { .. })
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Budget {
    pub limit_cents: i64,
    pub spent_cents: i64,
}

impl Budget {
    pub fn remaining_cents(&self) -> i64 {
        self.limit_cents.saturating_sub(self.spent_cents).max(0)
    }

    pub fn fraction(&self) -> f64 {
        if self.limit_cents <= 0 {
            return 1.0;
        }
        (self.spent_cents as f64 / self.limit_cents as f64).clamp(0.0, 1.0)
    }

    /// Alerta aos 80 %.
    pub fn near_limit(&self) -> bool {
        i128::from(self.spent_cents) * 10 >= i128::from(self.limit_cents) * 8
    }

    pub fn can_afford(&self, cents: i64) -> bool {
        cents >= 0
            && self.spent_cents >= 0
            && self
                .spent_cents
                .checked_add(cents)
                .is_some_and(|sum| sum <= self.limit_cents)
    }
}

/// Limites fixos (iguais para todo mundo no v1).
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Policy {
    /// Acima disso, dinheiro sempre pede, mesmo no Autônomo.
    pub money_ceiling_cents: i64,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            money_ceiling_cents: 10_000,
        } // R$ 100
    }
}

/// O avaliador. `ai_cost_cents` é o custo estimado de IA da ação (pré-autorizado
/// contra o orçamento, como o gateway de modelos fará).
pub fn evaluate(
    level: TrustLevel,
    action: &ActionClass,
    ai_cost_cents: i64,
    budget: &Budget,
    policy: &Policy,
) -> Decision {
    // 0. Sem orçamento, nada roda (nem responder): parada no teto.
    if !budget.can_afford(ai_cost_cents) {
        return Decision::Block(Reason::OverBudget {
            remaining_cents: budget.remaining_cents(),
        });
    }

    // 1. Linhas vermelhas: sempre pedem, em qualquer nível.
    match action {
        ActionClass::Money { cents } if *cents > policy.money_ceiling_cents => {
            return Decision::Request(Reason::MoneyAboveCeiling {
                ceiling_cents: policy.money_ceiling_cents,
            })
        }
        ActionClass::PublicAudience => return Decision::Request(Reason::NewPublicAudience),
        ActionClass::ThirdPartyData => return Decision::Request(Reason::ThirdPartyData),
        _ => {}
    }

    // 2. Por nível.
    use ActionClass::*;
    use TrustLevel::*;
    match (level, action) {
        (_, Reply) => Decision::Act { undoable: false },

        (Listen, _) => Decision::Block(Reason::TrustTooLow { needed: Suggest }),

        (Suggest, Reversible) => Decision::Request(Reason::TrustTooLow { needed: Act }),
        (Suggest, _) => Decision::Request(Reason::LeavesTheSpace),

        (Act, Reversible) | (Autonomous, Reversible) => Decision::Act { undoable: true },
        (Act, _) => Decision::Request(Reason::LeavesTheSpace),

        (Autonomous, External) => Decision::Act { undoable: false },
        (Autonomous, Money { .. }) => Decision::Act { undoable: false }, // ≤ teto (linha vermelha já filtrou)
        (Autonomous, Irreversible) => Decision::Request(Reason::LeavesTheSpace),
        (Autonomous, _) => Decision::Request(Reason::LeavesTheSpace),
    }
}

/// The kind of action a standing decision ("always approve/deny") covers. Same agent,
/// same key, same scope → the decision applies without asking again.
pub fn standing_key(action: &ActionClass) -> &'static str {
    match action {
        ActionClass::Reply => "reply",
        ActionClass::Reversible => "reversible",
        ActionClass::External => "external",
        ActionClass::Irreversible => "irreversible",
        ActionClass::Money { .. } => "money",
        ActionClass::PublicAudience => "public_audience",
        ActionClass::ThirdPartyData => "third_party_data",
    }
}

/// "Always approve" never covers a red line or something that can't be undone: those
/// keep asking every time. "Always deny" is allowed for anything.
pub fn standing_allow_permitted(action: &ActionClass, policy: &Policy) -> bool {
    match action {
        ActionClass::Money { cents } => *cents <= policy.money_ceiling_cents,
        ActionClass::PublicAudience | ActionClass::ThirdPartyData | ActionClass::Irreversible => {
            false
        }
        ActionClass::Reply | ActionClass::Reversible | ActionClass::External => true,
    }
}

/// "A confiança cresce com o histórico": depois de `n` aprovações seguidas, sem
/// edição, do mesmo tipo de ação, sugerimos subir o nível.
pub fn suggest_promotion(
    history: &[(ActionKindKey, Outcome)],
    kind: ActionKindKey,
    n: usize,
) -> bool {
    let streak = history
        .iter()
        .rev()
        .filter(|(k, _)| *k == kind)
        .take_while(|(_, o)| *o == Outcome::ApprovedUnchanged)
        .count();
    streak >= n
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ActionKindKey {
    Reversible,
    External,
    Money,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    ApprovedUnchanged,
    ApprovedEdited,
    Denied,
}

/// Uma aprovação vale para o conteúdo exato que foi pedido.
pub fn approval_still_valid(requested_hash: &str, current_hash: &str) -> bool {
    requested_hash == current_hash
}

#[cfg(test)]
mod tests {
    #[test]
    fn negative_and_overflowing_costs_cannot_bypass_the_budget() {
        let budget = super::Budget {
            limit_cents: i64::MAX,
            spent_cents: i64::MAX - 1,
        };
        assert!(budget.can_afford(1));
        assert!(!budget.can_afford(2));
        assert!(!budget.can_afford(-1));
        assert_eq!(budget.remaining_cents(), 1);
        assert!(budget.near_limit());
    }
    use super::*;
    use ActionClass::*;
    use TrustLevel::*;

    const B: Budget = Budget {
        limit_cents: 1_500,
        spent_cents: 0,
    };
    fn p() -> Policy {
        Policy::default()
    }

    #[test]
    fn everyone_can_reply_within_budget() {
        for l in [Listen, Suggest, Act, Autonomous] {
            assert_eq!(
                evaluate(l, &Reply, 5, &B, &p()),
                Decision::Act { undoable: false }
            );
        }
    }

    #[test]
    fn reversible_acts_with_undo_from_act_level() {
        assert_eq!(
            evaluate(Act, &Reversible, 0, &B, &p()),
            Decision::Act { undoable: true }
        );
        assert_eq!(
            evaluate(Autonomous, &Reversible, 0, &B, &p()),
            Decision::Act { undoable: true }
        );
        assert_eq!(
            evaluate(Suggest, &Reversible, 0, &B, &p()),
            Decision::Request(Reason::TrustTooLow { needed: Act })
        );
        assert!(matches!(
            evaluate(Listen, &Reversible, 0, &B, &p()),
            Decision::Block(_)
        ));
    }

    #[test]
    fn external_and_irreversible_ask_below_autonomous() {
        for l in [Suggest, Act] {
            assert_eq!(
                evaluate(l, &External, 0, &B, &p()),
                Decision::Request(Reason::LeavesTheSpace)
            );
            assert_eq!(
                evaluate(l, &Irreversible, 0, &B, &p()),
                Decision::Request(Reason::LeavesTheSpace)
            );
            assert_eq!(
                evaluate(l, &Money { cents: 500 }, 0, &B, &p()),
                Decision::Request(Reason::LeavesTheSpace)
            );
        }
        assert_eq!(
            evaluate(Autonomous, &External, 0, &B, &p()),
            Decision::Act { undoable: false }
        );
        assert_eq!(
            evaluate(Autonomous, &Money { cents: 5_000 }, 0, &B, &p()),
            Decision::Act { undoable: false }
        );
        // Apagar de verdade pede até no Autônomo.
        assert_eq!(
            evaluate(Autonomous, &Irreversible, 0, &B, &p()),
            Decision::Request(Reason::LeavesTheSpace)
        );
    }

    #[test]
    fn red_lines_always_ask_even_autonomous() {
        let ceiling = p().money_ceiling_cents;
        assert_eq!(
            evaluate(Autonomous, &Money { cents: 42_000 }, 0, &B, &p()),
            Decision::Request(Reason::MoneyAboveCeiling {
                ceiling_cents: ceiling
            })
        );
        assert_eq!(
            evaluate(Autonomous, &PublicAudience, 0, &B, &p()),
            Decision::Request(Reason::NewPublicAudience)
        );
        assert_eq!(
            evaluate(Autonomous, &ThirdPartyData, 0, &B, &p()),
            Decision::Request(Reason::ThirdPartyData)
        );
        // Exatamente no teto ainda é "pequeno".
        assert!(evaluate(Autonomous, &Money { cents: ceiling }, 0, &B, &p()).acts());
    }

    #[test]
    fn budget_stops_at_ceiling() {
        let almost = Budget {
            limit_cents: 1_500,
            spent_cents: 1_495,
        };
        assert!(evaluate(Act, &Reply, 5, &almost, &p()).acts());
        assert_eq!(
            evaluate(Act, &Reply, 6, &almost, &p()),
            Decision::Block(Reason::OverBudget { remaining_cents: 5 })
        );
        assert!(almost.near_limit());
        assert!(!Budget {
            limit_cents: 1_500,
            spent_cents: 1_100
        }
        .near_limit());
        assert!(Budget {
            limit_cents: 1_500,
            spent_cents: 1_200
        }
        .near_limit());
        assert_eq!(
            Budget {
                limit_cents: 1_500,
                spent_cents: 2_000
            }
            .remaining_cents(),
            0
        );
        assert_eq!(
            Budget {
                limit_cents: 1_500,
                spent_cents: 750
            }
            .fraction(),
            0.5
        );
    }

    #[test]
    fn promotion_needs_an_unbroken_streak() {
        use ActionKindKey as K;
        use Outcome as O;
        let h = vec![
            (K::Money, O::ApprovedUnchanged),
            (K::Money, O::ApprovedEdited),
            (K::Money, O::ApprovedUnchanged),
            (K::External, O::Denied),
            (K::Money, O::ApprovedUnchanged),
            (K::Money, O::ApprovedUnchanged),
        ];
        assert!(suggest_promotion(&h, K::Money, 3));
        assert!(!suggest_promotion(&h, K::Money, 4));
        assert!(!suggest_promotion(&h, K::External, 1));
    }

    #[test]
    fn approval_is_bound_to_content() {
        assert!(approval_still_valid("abc", "abc"));
        assert!(!approval_still_valid("abc", "abd"));
    }
}

//! Private poll input uses the existing journal; the builtin owns voting state.
use super::*;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PollOption {
    pub id: String,
    pub text: String,
}

pub(super) fn valid_shape(question: &str, options: &[PollOption]) -> bool {
    question.len() <= 300
        && !question.contains('\0')
        && (2..=10).contains(&options.len())
        && options.iter().all(|option| {
            intent_id(&option.id) && option.text.len() <= 200 && !option.text.contains('\0')
        })
        && options
            .iter()
            .map(|option| &option.id)
            .collect::<HashSet<_>>()
            .len()
            == options.len()
}

pub(super) fn arguments(question: &str, options: &[PollOption]) -> R<serde_json::Value> {
    let checked: Vec<&str> = options.iter().map(|option| option.text.trim()).collect();
    if !valid_shape(question, options)
        || question.trim().is_empty()
        || checked.iter().any(|value| value.is_empty())
        || checked.iter().collect::<HashSet<_>>().len() != checked.len()
    {
        return Err(CoreError::Invalid {
            reason: t(
                "Revise a pergunta e as opções.",
                "Review the question and options.",
            ),
        });
    }
    Ok(serde_json::json!({"question": question.trim(), "options": checked}))
}

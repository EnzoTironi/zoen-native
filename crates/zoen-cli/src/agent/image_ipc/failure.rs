//! Closed transport classifications, never subprocess/provider text.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Failure {
    Input,
    Configuration,
    Spawn,
    InputUnavailable,
    InputWrite,
    OutputUnavailable,
    OutputRead,
    OutputLimit,
    Wait,
    NonzeroExit,
    Malformed,
    Timeout,
}
impl Failure {
    pub(super) fn message(self) -> &'static str {
        match self {
            Self::Input => "invalid image command",
            Self::Configuration => "image host configuration unavailable",
            Self::Spawn => "image host could not start",
            Self::InputUnavailable => "image host input unavailable",
            Self::InputWrite => "image host input failed",
            Self::OutputUnavailable => "image host output unavailable",
            Self::OutputRead => "image host output failed",
            Self::OutputLimit => "image host response too large",
            Self::Wait => "image host status unavailable",
            Self::NonzeroExit => {
                "image execution unavailable; original review and financial attempt retained"
            }
            Self::Malformed => "invalid image host response",
            Self::Timeout => "image host response uncertain; recover the original review",
        }
    }
}

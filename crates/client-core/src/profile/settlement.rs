//! Original ordered profile completion predicate, retaining only its first blocker.
use super::Engine;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum ProfileSettlement {
    NotRequired,
    Ready,
    AgreementKeys,
    SealedProfiles,
    KeyShares,
    AgreementPublication,
    ProfileUpload,
    JournalUnavailable,
    PublicationIntent,
    ProfileKeyUnavailable,
}

impl ProfileSettlement {
    pub(crate) fn satisfied(self) -> bool {
        matches!(self, Self::NotRequired | Self::Ready)
    }
}

impl Engine {
    /// Nothing about profiles is waiting on the relay. Observation cannot grant readiness.
    #[cfg(test)]
    pub fn profiles_settled(&self) -> bool {
        self.profile_settlement().satisfied()
    }

    /// Same short-circuit order and reads as the original boolean predicate.
    pub(crate) fn profile_settlement(&self) -> ProfileSettlement {
        let p = &self.net.profiles;
        if !p.supported {
            return ProfileSettlement::NotRequired;
        }
        if !p.need_agreement.is_empty() {
            return ProfileSettlement::AgreementKeys;
        }
        if !p.need_profiles.is_empty() {
            return ProfileSettlement::SealedProfiles;
        }
        if p.shares_dirty {
            return ProfileSettlement::KeyShares;
        }
        if self.agreement_to_publish().is_some() {
            return ProfileSettlement::AgreementPublication;
        }
        if self.upload_due() {
            return ProfileSettlement::ProfileUpload;
        }
        let Ok(journal) = self.own_profile_journal() else {
            return ProfileSettlement::JournalUnavailable;
        };
        if journal.sync.pending.is_some() {
            return ProfileSettlement::PublicationIntent;
        }
        if journal.sync.key_unavailable() {
            return ProfileSettlement::ProfileKeyUnavailable;
        }
        ProfileSettlement::Ready
    }
}

#[cfg(test)]
mod tests;

// Native formatting exports remain included at crate root for UniFFI identity.
/// Formats cents as Brazilian reais ("R$ 1.348", "R$ 12,40").
#[uniffi::export]
pub fn format_brl(cents: i64) -> String {
    engine::brl(cents)
}

/// Formats the existing plan/grant domain's BRL cents for the chosen locale.
#[uniffi::export]
pub fn format_money(cents: i64, locale: String) -> String {
    i18n::money_in(i18n::Lang::from_tag(&locale), cents)
}

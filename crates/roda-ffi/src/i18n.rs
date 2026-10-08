//! Core localization. English is the development language; Portuguese (pt-BR) is the
//! second full translation. Each engine holds its language; `RodaEngine::lock` sets this
//! thread-local before every call, so free functions (mini-apps, money) follow it on
//! whatever thread Swift calls from, and parallel tests stay independent.

use std::cell::Cell;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Pt,
}

impl Lang {
    /// BCP-47 tag from the device ("pt-BR", "pt_PT", "en-US", …). Anything that isn't
    /// Portuguese falls back to English, the development language.
    pub fn from_tag(tag: &str) -> Lang {
        if tag.trim().to_ascii_lowercase().starts_with("pt") {
            Lang::Pt
        } else {
            Lang::En
        }
    }
    pub fn tag(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::Pt => "pt-BR",
        }
    }
}

thread_local! {
    static LANG: Cell<Lang> = const { Cell::new(Lang::En) };
}

pub fn set(lang: Lang) {
    LANG.with(|l| l.set(lang));
}

pub fn lang() -> Lang {
    LANG.with(|l| l.get())
}

pub fn is_en() -> bool {
    lang() == Lang::En
}

/// Pick the string for the current language.
pub fn t(pt: &str, en: &str) -> String {
    if is_en() {
        en.to_string()
    } else {
        pt.to_string()
    }
}

/// `tr!("pt {x}", "en {x}")`: `format!` in the current language (inline captures work).
#[macro_export]
macro_rules! tr {
    ($pt:literal, $en:literal $(,)?) => {
        if $crate::i18n::is_en() { format!($en) } else { format!($pt) }
    };
    ($pt:literal, $en:literal, $($arg:tt)+) => {
        if $crate::i18n::is_en() { format!($en, $($arg)+) } else { format!($pt, $($arg)+) }
    };
}

/// Money in the current language: "$1,348" / "$12.40" (en-US) or "R$ 1.348" / "R$ 12,40" (pt-BR).
pub fn money(cents: i64) -> String {
    money_in(lang(), cents)
}

pub fn money_in(lang: Lang, cents: i64) -> String {
    let neg = cents < 0;
    let c = cents.abs();
    let (units, frac) = (c / 100, c % 100);
    let (thousands, decimal, prefix) = match lang {
        Lang::En => (',', '.', "$"),
        Lang::Pt => ('.', ',', "R$ "),
    };
    let mut digits = units.to_string();
    let mut grouped = String::new();
    while digits.len() > 3 {
        let tail = digits.split_off(digits.len() - 3);
        grouped = format!("{thousands}{tail}{grouped}");
    }
    grouped = format!("{digits}{grouped}");
    let sign = if neg { "−" } else { "" };
    if frac == 0 {
        format!("{sign}{prefix}{grouped}")
    } else {
        format!("{sign}{prefix}{grouped}{decimal}{frac:02}")
    }
}

/// Thousands separator for plain numbers (scores): "4,893" / "4.893".
pub fn thousands(n: i64) -> String {
    let s = n.abs().to_string();
    let sep = if is_en() { ',' } else { '.' };
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(sep);
        }
        out.push(ch);
    }
    if n < 0 {
        format!("-{out}")
    } else {
        out
    }
}

/// Like `t`, for `&'static str` labels.
pub fn ts(pt: &'static str, en: &'static str) -> &'static str {
    if is_en() {
        en
    } else {
        pt
    }
}

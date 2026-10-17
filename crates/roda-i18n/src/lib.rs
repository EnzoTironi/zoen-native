//! Core localization. English is the development language; Portuguese (pt-BR) is the
//! second full translation. Hosts scope this thread-local around domain calls, so
//! native engines and browser workers use one language context and translation
//! implementation. Parallel native tests keep their own thread's context.

use std::{cell::Cell, future::Future, marker::PhantomData};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Lang {
    En,
    Pt,
}

impl Lang {
    /// BCP-47 tag from the device ("pt-BR", "pt_PT", "en-US", …). Anything that isn't
    /// Portuguese falls back to English, the development language.
    pub fn from_tag(tag: &str) -> Lang {
        if tag
            .trim()
            .split(['-', '_'])
            .next()
            .is_some_and(|language| language.eq_ignore_ascii_case("pt"))
        {
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

/// Restores the calling thread's previous language when synchronous work finishes.
/// Deliberately !Send/!Sync: never retain this guard across an await.
pub struct LanguageGuard {
    previous: Lang,
    _thread: PhantomData<*mut ()>,
}

pub fn enter(language: Lang) -> LanguageGuard {
    let previous = lang();
    set(language);
    LanguageGuard {
        previous,
        _thread: PhantomData,
    }
}

impl Drop for LanguageGuard {
    fn drop(&mut self) {
        set(self.previous);
    }
}

pub fn with_lang<T>(language: Lang, work: impl FnOnce() -> T) -> T {
    let _language = enter(language);
    work()
}

/// Applies the immutable session language on every poll, including after a future
/// migrates to another executor thread. Ambient thread state is always restored.
pub async fn scope<F: Future>(language: Lang, future: F) -> F::Output {
    let mut future = std::pin::pin!(future);
    std::future::poll_fn(|cx| with_lang(language, || future.as_mut().poll(cx))).await
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
        if $crate::is_en() { format!($en) } else { format!($pt) }
    };
    ($pt:literal, $en:literal, $($arg:tt)+) => {
        if $crate::is_en() { format!($en, $($arg)+) } else { format!($pt, $($arg)+) }
    };
}

/// Formats the existing plan/grant domain's BRL cents. Language only selects separators;
/// it never changes the currency, amount or financial policy.
pub fn money(cents: i64) -> String {
    money_in(lang(), cents)
}

pub fn money_in(lang: Lang, cents: i64) -> String {
    let neg = cents < 0;
    let c = cents.unsigned_abs();
    let (units, frac) = (c / 100, c % 100);
    let (thousands, decimal, prefix) = match lang {
        Lang::En => (',', '.', "R$ "),
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

#[cfg(test)]
mod scope_tests {
    use super::*;
    use std::{
        pin::Pin,
        sync::Arc,
        task::{Context, Poll, Wake, Waker},
    };

    struct Noop;
    impl Wake for Noop {
        fn wake(self: Arc<Self>) {}
    }
    fn poll<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
        let waker = Waker::from(Arc::new(Noop));
        future.poll(&mut Context::from_waker(&waker))
    }

    #[test]
    fn tag_selection_checks_the_complete_language_subtag() {
        for tag in ["pt", "pt-BR", "pt_PT", " PT-br "] {
            assert_eq!(Lang::from_tag(tag), Lang::Pt);
        }
        for tag in ["", "ptx", "en-US", "fr", "x-pt"] {
            assert_eq!(Lang::from_tag(tag), Lang::En);
        }
    }

    #[test]
    fn synchronous_scopes_restore_after_nesting_and_unwinding() {
        set(Lang::En);
        with_lang(Lang::Pt, || {
            assert_eq!(lang(), Lang::Pt);
            with_lang(Lang::En, || assert_eq!(lang(), Lang::En));
            assert_eq!(lang(), Lang::Pt);
        });
        assert_eq!(lang(), Lang::En);
        let _ = std::panic::catch_unwind(|| with_lang(Lang::Pt, || panic!("fixture")));
        assert_eq!(lang(), Lang::En);
    }

    async fn two_polls(expected: Lang) -> Lang {
        assert_eq!(lang(), expected); // also covers work before the first await/decode
        let mut first = true;
        std::future::poll_fn(|cx| {
            assert_eq!(lang(), expected);
            if std::mem::take(&mut first) {
                cx.waker().wake_by_ref();
                Poll::Pending
            } else {
                Poll::Ready(())
            }
        })
        .await;
        assert_eq!(lang(), expected);
        lang()
    }

    #[test]
    fn interleaved_sessions_restore_after_every_pending_and_ready_poll() {
        set(Lang::En);
        let mut portuguese = Box::pin(scope(Lang::Pt, two_polls(Lang::Pt)));
        let mut english = Box::pin(scope(Lang::En, two_polls(Lang::En)));
        assert!(poll(portuguese.as_mut()).is_pending());
        assert_eq!(lang(), Lang::En);
        set(Lang::Pt);
        assert!(poll(english.as_mut()).is_pending());
        assert_eq!(lang(), Lang::Pt);
        assert_eq!(poll(portuguese.as_mut()), Poll::Ready(Lang::Pt));
        assert_eq!(lang(), Lang::Pt);
        assert_eq!(poll(english.as_mut()), Poll::Ready(Lang::En));
        assert_eq!(lang(), Lang::Pt);
    }

    #[test]
    fn suspended_future_resumes_on_another_thread_in_its_session_language() {
        set(Lang::En);
        let mut future = Box::pin(scope(Lang::Pt, two_polls(Lang::Pt)));
        assert!(poll(future.as_mut()).is_pending());
        assert_eq!(lang(), Lang::En);
        std::thread::spawn(move || {
            set(Lang::En);
            assert_eq!(poll(future.as_mut()), Poll::Ready(Lang::Pt));
            assert_eq!(lang(), Lang::En);
        })
        .join()
        .unwrap();
        assert_eq!(lang(), Lang::En);
    }

    #[test]
    fn cancellation_does_not_leave_a_session_language_in_the_executor() {
        set(Lang::En);
        let mut future = Box::pin(scope(Lang::Pt, two_polls(Lang::Pt)));
        assert!(poll(future.as_mut()).is_pending());
        drop(future);
        assert_eq!(lang(), Lang::En);
    }
}

#[cfg(test)]
mod money_currency_tests {
    use super::{money_in, Lang};

    #[test]
    fn known_brl_domain_never_changes_currency_with_language() {
        assert_eq!(money_in(Lang::En, 12_345), "R$ 123.45");
        assert_eq!(money_in(Lang::Pt, 12_345), "R$ 123,45");
        assert_eq!(
            money_in(Lang::En, i64::MIN),
            "−R$ 92,233,720,368,547,758.08"
        );
        assert_eq!(money_in(Lang::Pt, i64::MAX), "R$ 92.233.720.368.547.758,07");
    }
}

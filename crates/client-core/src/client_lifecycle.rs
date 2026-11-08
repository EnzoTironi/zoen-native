//! Client construction keeps all runtime controls on the same native owner.
use super::*;

#[uniffi::export]
impl Client {
    /// Opens (or creates) the database at `path` (":memory:" in tests). `locale` is the
    /// device language (BCP-47, e.g. "en-US", "pt-BR"): agent output, demo data and money
    /// formatting follow it. Anything that isn't Portuguese falls back to English.
    #[uniffi::constructor]
    pub fn open(path: String, locale: String) -> Result<Arc<Self>, CoreError> {
        let lang = i18n::Lang::from_tag(&locale);
        let _language = i18n::enter(lang);
        Ok(Arc::new(Self {
            inner: Arc::new(Mutex::new(Engine::open(&path)?)),
            lang,
            net: Mutex::new(None),
            history_receive: link_api::HistoryControl::default(),
            link_pickup: link_pickup::PickupControl::default(),
            backup_write: tokio::sync::Mutex::new(()),
        }))
    }

    /// Language the core is speaking ("en" or "pt-BR").
    pub fn language(&self) -> String {
        self.lang.tag().to_string()
    }
}

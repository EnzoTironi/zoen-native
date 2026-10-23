//! Versioned public delivery, separate from private Items and the relay router.
//! Mount management only on an authenticated API origin and public_router only
//! on a separate, cookieless origin. Neither router imports the other.

mod configuration;
mod http;
mod inspection;
mod persistence;
mod storage;

use std::sync::Arc;

use object_store::ObjectStore;
use protocol::sites::{Configuration, Receipt, SignedMutation};
use sqlx::PgPool;

pub(crate) use configuration::announcement;
pub use configuration::configure;
pub use http::{management_router, public_router};

#[derive(Clone)]
pub struct Service {
    pool: PgPool,
    store: Arc<dyn ObjectStore>,
    configuration: Configuration,
    publication_kib: Arc<crate::limits::Limiter>,
    in_flight: Arc<tokio::sync::Semaphore>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Invalid,
    Unauthorized,
    Conflict,
    Unavailable,
    Missing,
    Limited,
}

impl Service {
    /// `store` must use the publication namespace; private encrypted blobs are
    /// never inferred from a caller-supplied object key.
    pub fn new(
        pool: PgPool,
        store: Arc<dyn ObjectStore>,
        configuration: Configuration,
    ) -> Result<Self, Error> {
        configuration::validate(&configuration)?;
        Ok(Self {
            pool,
            store,
            configuration,
            publication_kib: Arc::new(crate::limits::Limiter::new(
                "sites_owner_kib",
                crate::limits::Quota::per_minute(8192, 16384),
            )),
            in_flight: Arc::new(tokio::sync::Semaphore::new(16)),
        })
    }

    /// Current-device authorization and head/receipt updates share one SQL
    /// transaction. The exact selected bytes are stored before making them
    /// visible. Lost ACKs recover the original receipt, including after newer
    /// publications; they never roll the public head backwards.
    pub async fn apply(
        &self,
        signed: &SignedMutation,
        html: Option<&[u8]>,
        now_ms: i64,
    ) -> Result<Receipt, Error> {
        use relay_admission::sites as policy;
        let _permit = self.in_flight.try_acquire().map_err(|_| Error::Limited)?;
        policy::authorize(signed, &self.configuration.audience, now_ms).map_err(
            |error| match error {
                policy::Failure::Signature | policy::Failure::Stale => Error::Unauthorized,
                _ => Error::Invalid,
            },
        )?;
        if signed.mutation.public_origin != self.configuration.public_origin {
            return Err(Error::Invalid);
        }
        let key = policy::verify_content(signed, html).map_err(|_| Error::Invalid)?;
        self.persist(signed, key.as_deref(), html).await
    }
}

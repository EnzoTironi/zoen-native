//! Optional publication delivery process. Uses the existing directory database
//! and object_store adapter; never serves public HTML from the authenticated API.
use std::{net::SocketAddr, path::PathBuf};

use zoen_relay::sites::{management_router, public_router, Service};

fn required(name: &str) -> anyhow::Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("set {name}"))
}

fn origin(value: &str) -> anyhow::Result<reqwest::Url> {
    let url = reqwest::Url::parse(value)?;
    anyhow::ensure!(
        matches!(url.scheme(), "https" | "http")
            && url.host_str().is_some()
            && url.username().is_empty()
            && url.password().is_none()
            && url.query().is_none()
            && url.fragment().is_none()
            && url.path() == "/",
        "Sites origins must be http(s) origins without credentials or paths"
    );
    Ok(url)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let management_origin = origin(&required("ZOEN_SITES_MANAGEMENT_ORIGIN")?)?;
    let public_origin = origin(&required("ZOEN_SITES_PUBLIC_ORIGIN")?)?;
    anyhow::ensure!(
        management_origin.host_str() != public_origin.host_str(),
        "public Sites must use a different hostname from the authenticated API"
    );
    let management_bind: SocketAddr = required("ZOEN_SITES_MANAGEMENT_BIND")?.parse()?;
    let public_bind: SocketAddr = required("ZOEN_SITES_PUBLIC_BIND")?.parse()?;
    anyhow::ensure!(
        management_bind != public_bind,
        "Sites listeners must be separate"
    );
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(20)
        .connect(&required("DATABASE_URL")?)
        .await?;
    sqlx::migrate!("./migrations").run(&pool).await?;
    let dir = PathBuf::from(required("ZOEN_SITES_BLOB_DIR")?);
    let (store, _) = zoen_relay::blobs::store_from_env(&dir)?;
    let service = Service::new(pool, store, required("ZOEN_RELAY_NAME")?)
        .map_err(|_| anyhow::anyhow!("invalid Sites audience"))?;
    let management = tokio::net::TcpListener::bind(management_bind).await?;
    let public = tokio::net::TcpListener::bind(public_bind).await?;
    tokio::try_join!(
        async { axum::serve(management, management_router(service.clone())).await },
        async { axum::serve(public, public_router(service.clone())).await },
    )?;
    Ok(())
}

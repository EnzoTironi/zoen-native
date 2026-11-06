//! Automatic wake/reconciliation loop; bus payloads are never execution input.
use crate::{config::Config, file_vault::FileVault, protocol, runtime_host_config::text};
use futures_util::StreamExt;
use std::time::Duration;
use zoen_runtime::{ReplyRuntimeHost, ReplyWorker, ReplyWorkerEvent, RuntimeError};

pub(super) async fn run(
    host: &ReplyRuntimeHost,
    config: &Config,
    vault: &FileVault,
) -> Result<(), String> {
    let mut hints = if let Some(key) = &config.worker_bus_key {
        let url = text(vault, key)?;
        match async_nats::ConnectOptions::new()
            .name("zoen-native-reply-worker")
            .subscription_capacity(1)
            .connection_timeout(Duration::from_secs(5))
            .connect(url)
            .await
        {
            Ok(client) => match client
                .subscribe(host.wake_subject().map_err(|e| e.to_string())?)
                .await
            {
                Ok(subscription) => {
                    client
                        .flush()
                        .await
                        .map_err(|_| "worker hint subscription unavailable")?;
                    Some(subscription)
                }
                Err(_) => None,
            },
            Err(_) => None,
        }
    } else {
        None
    };
    let mut shutdown = Shutdown::new()?;
    let mut worker = ReplyWorker::default();
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    crate::emit(
        &serde_json::json!({"status":"worker_ready","notifications":hints.is_some(),"subject":host.wake_subject().map_err(|e|e.to_string())?}),
    )?;
    loop {
        match Box::pin(worker.tick(host)).await {
            Ok(events) => {
                for event in events {
                    let value = match event {
                        ReplyWorkerEvent::Progress(progress) => protocol::progress(*progress),
                        ReplyWorkerEvent::Deferred { run, error } => {
                            serde_json::json!({"status":"deferred","run":run,"reason":error.to_string()})
                        }
                        ReplyWorkerEvent::ConversationDeferred { space, error } => {
                            serde_json::json!({"status":"conversation_deferred","space":space,"reason":error.to_string()})
                        }
                    };
                    crate::emit(&value)?;
                }
            }
            Err(RuntimeError::Unavailable) => {}
            Err(error) => return Err(error.to_string()),
        }
        // Coalesce bursts and bound reconciliation even under hostile hints.
        tokio::time::sleep(Duration::from_millis(250)).await;
        tokio::select! {
            result=shutdown.wait()=>{ result?; return Ok(()) },
            _=interval.tick()=>{},
            _=async { match hints.as_mut() {
                Some(subscription)=>{if subscription.next().await.is_none(){ hints=None; }},
                None=>std::future::pending::<()>().await,
            }}=>{},
        }
    }
}

struct Shutdown {
    #[cfg(unix)]
    interrupt: tokio::signal::unix::Signal,
    #[cfg(unix)]
    terminate: tokio::signal::unix::Signal,
}
impl Shutdown {
    fn new() -> Result<Self, String> {
        Ok(Self {
            #[cfg(unix)]
            interrupt: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::interrupt())
                .map_err(|_| "worker signal unavailable")?,
            #[cfg(unix)]
            terminate: tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
                .map_err(|_| "worker signal unavailable")?,
        })
    }
    async fn wait(&mut self) -> Result<(), String> {
        #[cfg(unix)]
        {
            tokio::select! { _=self.interrupt.recv()=>{}, _=self.terminate.recv()=>{} }
            Ok(())
        }
        #[cfg(not(unix))]
        {
            tokio::signal::ctrl_c()
                .await
                .map_err(|_| "worker signal unavailable".into())
        }
    }
}

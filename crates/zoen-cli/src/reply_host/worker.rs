//! Automatic wake/reconciliation loop; bus payloads are never execution input.
use crate::{config::Config, file_vault::FileVault, protocol, runtime_host_config::text};
use futures_util::StreamExt;
use std::time::Duration;
use zoen_runtime::{
    ReplyRuntimeHost, ReplySource, ReplyWorker, ReplyWorkerEvent, RuntimeError, SelectedReplyWorker,
};

pub(super) async fn run(
    host: &ReplyRuntimeHost,
    config: &Config,
    vault: &FileVault,
    supervised: bool,
    source: Option<ReplySource>,
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
    let controls = crate::control::Server::bind(&config.home)?;
    let mut shutdown = Shutdown::new()?;
    let mut parent = Box::pin(parent_lifetime(supervised)?);
    let mut worker = Worker::new(source)?;
    let mut interval = tokio::time::interval(Duration::from_secs(2));
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    crate::emit(
        &serde_json::json!({"status":"worker_ready","notifications":hints.is_some(),"subject":host.wake_subject().map_err(|e|e.to_string())?}),
    )?;
    loop {
        match Box::pin(worker.tick(host)).await {
            Ok((events, complete)) => {
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
                if complete {
                    crate::emit(&serde_json::json!({"status":"source_completed"}))?;
                    return Ok(());
                }
            }
            Err(RuntimeError::Unavailable) => {}
            Err(error) => return Err(error.to_string()),
        }
        // Coalesce bursts and bound reconciliation even under hostile hints.
        tokio::time::sleep(Duration::from_millis(250)).await;
        tokio::select! {
            _=&mut parent=>{ return Ok(()) },
            result=shutdown.wait()=>{ result?; return Ok(()) },
            _=interval.tick()=>{},
            request=controls.accept()=>{
                if crate::control::Server::respond(host, request?).await {
                    worker.reconcile_soon();
                }
            },
            _=async { match hints.as_mut() {
                Some(subscription)=>{if subscription.next().await.is_none(){ hints=None; }},
                None=>std::future::pending::<()>().await,
            }}=>{},
        }
    }
}

enum Worker {
    All(ReplyWorker),
    Source(SelectedReplyWorker),
}
impl Worker {
    fn new(source: Option<ReplySource>) -> Result<Self, String> {
        match source {
            Some(source) => SelectedReplyWorker::new(source)
                .map(Self::Source)
                .map_err(|e| e.to_string()),
            None => Ok(Self::All(ReplyWorker::default())),
        }
    }
    async fn tick(
        &mut self,
        host: &ReplyRuntimeHost,
    ) -> Result<(Vec<ReplyWorkerEvent>, bool), RuntimeError> {
        match self {
            Self::All(worker) => worker.tick(host).await.map(|events| (events, false)),
            Self::Source(worker) => worker.tick(host).await,
        }
    }
    fn reconcile_soon(&mut self) {
        match self {
            Self::All(worker) => worker.reconcile_soon(),
            Self::Source(worker) => worker.reconcile_soon(),
        }
    }
}

/// A pipe held only by the supervisor ends even if it is killed without running
/// destructors. EOF requests graceful shutdown between durable worker ticks.
fn parent_lifetime(supervised: bool) -> Result<impl std::future::Future<Output = ()>, String> {
    let (send, receive) = tokio::sync::oneshot::channel();
    if supervised {
        std::thread::Builder::new()
            .name("reply-parent".into())
            .spawn(move || {
                use std::io::Read;
                let mut byte = [0];
                // Any input, EOF or pipe error ends this lifetime; the pipe is never
                // a command transport and cannot grant work or change authority.
                let _ = std::io::stdin().read(&mut byte);
                let _ = send.send(());
            })
            .map_err(|_| "parent lifetime unavailable")?;
    }
    Ok(async move {
        if supervised {
            let _ = receive.await;
        } else {
            std::future::pending::<()>().await;
        }
    })
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

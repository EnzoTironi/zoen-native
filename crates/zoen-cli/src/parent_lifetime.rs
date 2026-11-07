//! EOF on a supervisor-owned pipe retires a companion even after abrupt death.
pub(crate) fn watch(supervised: bool) -> Result<impl std::future::Future<Output = ()>, String> {
    let (send, receive) = tokio::sync::oneshot::channel();
    if supervised {
        std::thread::Builder::new()
            .name("zoen-parent".into())
            .spawn(move || {
                use std::io::Read;
                let mut byte = [0];
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

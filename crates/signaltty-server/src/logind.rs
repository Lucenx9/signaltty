//! Best-effort early host shutdown notification. The fd delays shutdown only
//! up to logind's own policy; it does not protect against power loss.
use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use zbus::zvariant::OwnedFd;

use crate::router::Ctx;

pub(crate) struct Monitor(tokio::task::JoinHandle<()>);

impl Drop for Monitor {
    fn drop(&mut self) {
        self.0.abort();
    }
}

const MIN_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(60);

pub(crate) fn start(ctx: Arc<Ctx>) -> Monitor {
    Monitor(tokio::spawn(async move {
        let mut backoff = MIN_BACKOFF;
        loop {
            match prepare().await {
                Ok(inhibitor) => {
                    tracing::info!(
                        "host shutdown requested; writing snapshot before pane termination"
                    );
                    if let Err(e) = crate::persist::save(&ctx.store, &ctx.ptys.terms(), &ctx.config)
                    {
                        tracing::warn!("host shutdown snapshot failed: {e}");
                    }
                    drop(inhibitor);
                    ctx.shutdown.notify_waiters();
                    break;
                }
                Err(e) => {
                    tracing::debug!("logind shutdown notification unavailable: {e}");
                    tokio::time::sleep(backoff).await;
                    backoff = (backoff * 2).min(MAX_BACKOFF);
                }
            }
        }
    }))
}

async fn prepare() -> zbus::Result<OwnedFd> {
    let connection = zbus::connection::Builder::system()?
        .method_timeout(Duration::from_secs(5))
        .build()
        .await?;
    let manager = zbus::proxy::Builder::<zbus::Proxy<'_>>::new(&connection)
        .destination("org.freedesktop.login1")?
        .path("/org/freedesktop/login1")?
        .interface("org.freedesktop.login1.Manager")?
        .cache_properties(zbus::proxy::CacheProperties::No)
        .build()
        .await?;
    let mut owners = manager.receive_owner_changed().await?;
    let mut signals = manager.receive_signal("PrepareForShutdown").await?;
    let inhibitor: OwnedFd = manager
        .call(
            "Inhibit",
            &(
                "shutdown",
                "signaltty",
                "Save workspace and agent resume metadata",
                "delay",
            ),
        )
        .await?;
    let mut preparing: bool = manager.get_property("PreparingForShutdown").await?;
    loop {
        if preparing {
            return Ok(inhibitor);
        }
        tokio::select! {
            _ = owners.next() => return Err(zbus::Error::Failure("logind owner changed".into())),
            signal = signals.next() => {
                let signal = signal.ok_or_else(|| zbus::Error::Failure("logind disconnected".into()))?;
                preparing = signal.body().deserialize()?;
            }
        }
    }
}

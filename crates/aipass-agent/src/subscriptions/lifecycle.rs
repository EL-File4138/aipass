//! Bound account setup and connection establishment, never generation reads.
use super::*;

pub(super) fn transport(proxy: Option<&UpstreamProxyConfig>) -> Result<Client> {
    let mut builder = Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never())
        .connect_timeout(Duration::from_secs(15));
    if let Some(proxy) = proxy {
        builder = aipass_proxy::apply_upstream_proxy(builder, proxy)?;
    }
    builder
        .build()
        .map_err(|_| "cannot create subscription transport".into())
}

#[cfg(test)]
mod tests;

/// Cancellation waits for an in-flight one-use rotation and its durable ACK.
/// The setup deadline stops applying once the generation dispatch stage starts.
pub(super) async fn run_cancellable(
    operation: impl std::future::Future<Output = Result<()>>,
    mut cancellation: watch::Receiver<bool>,
    mut refreshing: watch::Receiver<bool>,
    mut generation: watch::Receiver<bool>,
) -> Result<()> {
    tokio::pin!(operation);
    let deadline = tokio::time::sleep(Duration::from_secs(1200));
    tokio::pin!(deadline);
    let mut canceled = *cancellation.borrow();
    let mut generating = *generation.borrow();
    let mut refresh_open = true;
    let mut generation_open = true;
    loop {
        generating |= *generation.borrow();
        if canceled && !*refreshing.borrow() {
            return Err("subscription operation cancelled".into());
        }
        tokio::select! {
            result = &mut operation => return result,
            _ = &mut deadline, if !generating && !*refreshing.borrow() => {
                // Polling the operation may enter dispatch or rotation in this
                // same select. Recheck before dropping a submitted operation.
                generating |= *generation.borrow();
                if !generating && !*refreshing.borrow() {
                    return Err("subscription operation timed out".into());
                }
            },
            changed = cancellation.changed(), if !canceled => {
                canceled = changed.is_err() || *cancellation.borrow();
            },
            changed = refreshing.changed(), if refresh_open => { refresh_open = changed.is_ok(); },
            changed = generation.changed(), if generation_open => {
                generation_open = changed.is_ok();
                generating |= *generation.borrow();
            },
        }
    }
}

//! Linux Secret Service stores. Searches never unlock a collection or display a prompt.
use anyhow::{bail, Result};
use secret_service::{EncryptionType, Item, SecretService};
use std::{collections::HashMap, future::Future, time::Duration};
use zeroize::Zeroizing;
const LIMIT: usize = 4 * 1024 * 1024;
fn unavailable(_: impl std::fmt::Debug) -> anyhow::Error {
    anyhow::anyhow!("system credential store unavailable; unlock the Linux keyring and retry")
}
fn bounded<T>(operation: impl Future<Output = Result<T>>) -> Result<T> {
    async_io::block_on(futures_lite::future::or(operation, async {
        async_io::Timer::after(Duration::from_secs(3)).await;
        bail!("system credential store timed out")
    }))
}
async fn find<'a>(
    ss: &'a SecretService<'_>,
    service: &str,
    account: &str,
    keytar: bool,
) -> Result<Option<Item<'a>>> {
    let field = if keytar { "account" } else { "username" };
    let mut attrs = HashMap::from([("service", service), (field, account)]);
    if !keytar {
        attrs.insert("target", "default");
    }
    let search = ss.search_items(attrs).await.map_err(unavailable)?;
    let mut items: Vec<_> = search.unlocked.into_iter().chain(search.locked).collect();
    if !keytar && items.is_empty() {
        // keyring-rs also supports legacy entries without target, in the default collection only.
        let search = ss
            .search_items(HashMap::from([("service", service), ("username", account)]))
            .await
            .map_err(unavailable)?;
        let legacy: Vec<_> = search.unlocked.into_iter().chain(search.locked).collect();
        if !legacy.is_empty() {
            let default = ss.get_default_collection().await.map_err(unavailable)?;
            let prefix = format!("{}/", default.collection_path.as_str());
            for item in legacy {
                if item.item_path.as_str().starts_with(&prefix)
                    && !item
                        .get_attributes()
                        .await
                        .map_err(unavailable)?
                        .contains_key("target")
                {
                    items.push(item);
                }
            }
        }
    }
    if items.len() > 1 {
        bail!("system credential store has ambiguous accounts");
    }
    if let Some(item) = items.pop() {
        if item.is_locked().await.map_err(unavailable)? {
            bail!("system credential store is locked; unlock the Linux keyring and retry");
        }
        Ok(Some(item))
    } else {
        Ok(None)
    }
}
fn read_item(service: &str, account: &str, keytar: bool) -> Result<Option<Zeroizing<Vec<u8>>>> {
    bounded(async {
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(unavailable)?;
        let Some(item) = find(&ss, service, account, keytar).await? else {
            return Ok(None);
        };
        let secret = Zeroizing::new(item.get_secret().await.map_err(unavailable)?);
        if secret.len() > LIMIT {
            bail!("invalid native credential data");
        }
        Ok(Some(secret))
    })
}
pub(super) fn read(service: &str, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
    read_item(service, account, false)
}
/// libsecret/keytar uses account; Rust keyring uses username and a target attribute.
pub fn read_keytar(service: &str, account: &str) -> Result<Option<Zeroizing<Vec<u8>>>> {
    read_item(service, account, true)
}
pub(super) fn write(service: &str, account: &str, bytes: Option<&[u8]>) -> Result<()> {
    if bytes.is_some_and(|b| b.len() > LIMIT) {
        bail!("invalid native credential data");
    }
    bounded(async {
        let ss = SecretService::connect(EncryptionType::Dh)
            .await
            .map_err(unavailable)?;
        match (find(&ss, service, account, false).await?, bytes) {
            (Some(item), Some(bytes)) => item
                .set_secret(bytes, "text/plain")
                .await
                .map_err(unavailable)?,
            (Some(item), None) => item.delete().await.map_err(unavailable)?,
            (None, Some(bytes)) => {
                let collection = ss.get_default_collection().await.map_err(unavailable)?;
                if collection.is_locked().await.map_err(unavailable)? {
                    bail!("system credential store is locked");
                }
                collection
                    .create_item(
                        "AIPass native CLI credential",
                        HashMap::from([
                            ("service", service),
                            ("username", account),
                            ("target", "default"),
                            ("application", "rust-keyring-client"),
                        ]),
                        bytes,
                        true,
                        "text/plain",
                    )
                    .await
                    .map_err(unavailable)?;
            }
            (None, None) => {}
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests;

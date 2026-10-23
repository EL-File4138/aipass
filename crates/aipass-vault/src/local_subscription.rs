//! Device-bound subscription grants are deliberately outside snapshot/export allowlists.
use super::*;
use std::io::Read;
use zeroize::Zeroizing;

const LIMIT: usize = 8 * 1024 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LocalGrant {
    version: u16,
    key: Ciphertext,
    payload: Ciphertext,
}

impl Vault {
    fn subscription_local_context(
        &self,
        generation: Uuid,
        device: &str,
    ) -> Result<String, VaultError> {
        if device.is_empty() || device.len() > 128 {
            return Err(VaultError::InvalidSecretValue);
        }
        // A copied vault on this same device must not become a second refresh
        // owner. Directory aliases share an owner; moving a vault needs login.
        let root = fs::canonicalize(&self.root)?;
        Ok(format!(
            "subscription-local-v1:{generation}:{device}:{root:?}"
        ))
    }

    pub fn read_local_subscription(
        &self,
        generation: Uuid,
        device: &str,
    ) -> Result<Option<SecretString>, VaultError> {
        self.ensure_sync_ready()?;
        let path = self
            .root
            .join("local-subscriptions")
            .join(format!("{generation}.aiplocal"));
        let file = match fs::File::open(path) {
            Ok(file) => file,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.into()),
        };
        let mut bytes = Vec::new();
        file.take((LIMIT * 2) as u64 + 1).read_to_end(&mut bytes)?;
        if bytes.len() > LIMIT * 2 {
            return Err(VaultError::InvalidSecretValue);
        }
        let grant: LocalGrant = serde_json::from_slice(&bytes)?;
        if grant.version != 1 {
            return Err(VaultError::UnsupportedVersion);
        }
        let purpose = self.subscription_local_context(generation, device)?;
        let key = Zeroizing::new(self.decrypt_local_state(&purpose, &grant.key)?);
        let key: &[u8; KEY_LEN] = key
            .as_slice()
            .try_into()
            .map_err(|_| VaultError::InvalidSecretValue)?;
        let plaintext = Zeroizing::new(decrypt_bytes(key, purpose.as_bytes(), &grant.payload)?);
        let text = std::str::from_utf8(&plaintext).map_err(|_| VaultError::InvalidSecretValue)?;
        Ok(Some(SecretString::new(text)))
    }

    pub fn write_local_subscription(
        &self,
        generation: Uuid,
        device: &str,
        value: &SecretString,
    ) -> Result<(), VaultError> {
        self.ensure_sync_ready()?;
        if value.expose().len() > LIMIT {
            return Err(VaultError::InvalidSecretValue);
        }
        let purpose = self.subscription_local_context(generation, device)?;
        let dek = generate_record_dek();
        let grant = LocalGrant {
            version: 1,
            key: self.encrypt_local_state(&purpose, dek.as_bytes())?,
            payload: encrypt_bytes(
                dek.as_bytes(),
                purpose.as_bytes(),
                value.expose().as_bytes(),
            )?,
        };
        let dir = self.root.join("local-subscriptions");
        fs::create_dir_all(&dir)?;
        if !fs::symlink_metadata(&dir)?.is_dir() {
            return Err(VaultError::InvalidSecretValue);
        }
        atomic_write_bytes(
            dir.join(format!("{generation}.aiplocal")),
            &serde_json::to_vec(&grant)?,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests;

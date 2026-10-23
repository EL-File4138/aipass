//! Atomic subscription bindings and Agent-only encrypted runtime data.
use super::*;

impl Vault {
    /// Caller verifies vendor ownership. This CAS changes all binding fields
    /// together and removes v1 so older Agents fail closed on the new scope.
    pub fn migrate_subscription_scope(
        &self,
        id: Uuid,
        expected_marker: &str,
        expected_bundle: &SecretString,
        identity: &str,
        marker: &str,
        bundle: &SecretString,
    ) -> Result<(), VaultError> {
        let mut record = self.decrypt_provider_path(&self.record_path(id))?;
        let secret = record
            .entry
            .secret_refs
            .first_mut()
            .ok_or(VaultError::RecordNotFound)?;
        if record.entry.provider_id.as_deref() != Some("devin")
            || record.secrets.get(&secret.id).map(String::as_str) != Some(expected_marker)
            || record
                .runtime_extensions
                .get("community_account_v1")
                .map(String::as_str)
                != Some(expected_bundle.expose())
            || record
                .runtime_extensions
                .contains_key("community_account_v2")
        {
            return Err(VaultError::InvalidSecretValue);
        }
        if let Some(mut old) = record.secrets.insert(secret.id.clone(), marker.into()) {
            old.zeroize();
        }
        secret.masked = mask_secret(marker);
        secret.fingerprint = hmac_fingerprint(&self.index_key, marker);
        record.entry.account_identity = Some(identity.into());
        record.entry.subscription = None;
        record.entry.quota = None;
        for key in ["community_account_v1", "community_usage_v1"] {
            if let Some(mut old) = record.runtime_extensions.remove(key) {
                old.zeroize();
            }
        }
        record
            .runtime_extensions
            .insert("community_account_v2".into(), bundle.expose().into());
        record.entry.updated_at = OffsetDateTime::now_utc();
        self.write_provider_record(id, &record)?;
        self.audit("provider.scope.migrate", Some(id), None)
    }
    /// Switch an existing subscription to a CLI-owned reference in one record
    /// write, retaining its stable entry/secret IDs and configured routes.
    pub fn bind_cli_subscription(
        &self,
        id: Uuid,
        provider: &str,
        identity: &str,
        marker: &str,
        extension_key: &str,
        reference: &SecretString,
    ) -> Result<(), VaultError> {
        let mut record = self.decrypt_provider_path(&self.record_path(id))?;
        let secret = record
            .entry
            .secret_refs
            .first_mut()
            .ok_or(VaultError::RecordNotFound)?;
        if let Some(mut previous) = record.secrets.insert(secret.id.clone(), marker.to_owned()) {
            previous.zeroize();
        }
        secret.masked = mask_secret(marker);
        secret.fingerprint = hmac_fingerprint(&self.index_key, marker);
        record.entry.provider_id = Some(provider.to_owned());
        record.entry.credential_kind = CredentialKind::OAuth;
        record.entry.account_identity = Some(identity.to_owned());
        if provider == "devin" || extension_key == "community_account_v3" {
            record.entry.subscription = None;
            record.entry.quota = None;
            if let Some(mut old) = record.runtime_extensions.remove("community_usage_v1") {
                old.zeroize();
            }
        }
        record.entry.supports_websockets = Some(false);
        for key in [
            "claude_native",
            "copilot_auth_v1",
            "community_account_v1",
            "community_account_v2",
            "community_account_v3",
            extension_key,
        ] {
            if let Some(mut previous) = record.runtime_extensions.remove(key) {
                previous.zeroize();
            }
        }
        record
            .runtime_extensions
            .insert(extension_key.to_owned(), reference.expose().to_owned());
        record.entry.updated_at = OffsetDateTime::now_utc();
        self.write_provider_record(id, &record)?;
        self.audit("provider.cli.bind", Some(id), None)
    }

    pub fn provider_runtime_extension(
        &self,
        id: Uuid,
        key: &str,
    ) -> Result<Option<SecretString>, VaultError> {
        let mut record = self.decrypt_provider_path(&self.record_path(id))?;
        Ok(record.runtime_extensions.remove(key).map(SecretString::new))
    }
    pub fn set_provider_runtime_extension(
        &self,
        id: Uuid,
        key: &str,
        value: Option<&SecretString>,
    ) -> Result<(), VaultError> {
        let mut record = self.decrypt_provider_path(&self.record_path(id))?;
        if let Some(mut old) = record.runtime_extensions.remove(key) {
            old.zeroize();
        }
        if let Some(value) = value {
            record
                .runtime_extensions
                .insert(key.to_owned(), value.expose().to_owned());
        }
        record.entry.updated_at = OffsetDateTime::now_utc();
        self.write_provider_record(id, &record)?;
        self.audit("provider.runtime.update", Some(id), None)?;
        Ok(())
    }
}

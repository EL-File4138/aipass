use super::*;
use core_foundation::{
    base::{CFType, TCFType},
    boolean::CFBoolean,
    date::CFDate,
    dictionary::CFDictionary,
    string::{CFString, CFStringRef},
};
use security_framework_sys::{item::*, keychain_item::SecItemCopyMatching};

// This SDK constant is not exported by security-framework-sys 2.17.
unsafe extern "C" {
    static kSecUseAuthenticationUIFail: CFStringRef;
}
pub(super) struct Keychain {
    pub(super) service: String,
    pub(super) account: String,
}
impl Source for Keychain {
    fn version(&self) -> Result<Option<(u64, u64)>> {
        // Return attributes only and explicitly prohibit OS authentication UI.
        // "Unavailable" is distinct from absence; never fall back on an ACL error.
        unsafe {
            let key = |v| CFString::wrap_under_get_rule(v).into_CFType();
            let query = CFDictionary::from_CFType_pairs(&[
                (key(kSecClass), key(kSecClassGenericPassword)),
                (
                    key(kSecAttrService),
                    CFString::new(&self.service).into_CFType(),
                ),
                (
                    key(kSecAttrAccount),
                    CFString::new(&self.account).into_CFType(),
                ),
                (
                    key(kSecReturnAttributes),
                    CFBoolean::true_value().into_CFType(),
                ),
                (key(kSecReturnData), CFBoolean::false_value().into_CFType()),
                (
                    key(kSecUseAuthenticationUI),
                    key(kSecUseAuthenticationUIFail),
                ),
            ]);
            let mut result = std::ptr::null();
            let status = SecItemCopyMatching(query.as_concrete_TypeRef(), &mut result);
            if status == -25300 {
                return Ok(None);
            }
            if status != 0 {
                bail!("system credential attributes unavailable; unlock Keychain and retry");
            }
            if result.is_null() {
                bail!("system credential attributes missing");
            }
            let value = CFType::wrap_under_create_rule(result);
            let dictionary = value
                .downcast::<CFDictionary>()
                .context("invalid credential attributes")?;
            let date = |name: &str| -> Result<u64> {
                let raw = *dictionary
                    .find(CFString::new(name).as_CFTypeRef())
                    .context("credential modification date missing")?;
                let date = CFType::wrap_under_get_rule(raw)
                    .downcast::<CFDate>()
                    .context("invalid credential modification date")?;
                Ok(date.abs_time().to_bits())
            };
            Ok(Some((date("cdat")?, date("mdat")?)))
        }
    }
    fn read(&self) -> Result<Secret> {
        super::super::keychain_read_direct(&self.service, &self.account)
    }
}

//! Narrow HKCU Run access. No elevation, shell, environment expansion or secrets.
use anyhow::{ensure, Result};
use std::{io, ptr};
use windows_sys::Win32::{
    Foundation::{ERROR_FILE_NOT_FOUND, ERROR_NO_MORE_ITEMS},
    System::Registry::*,
};

pub(super) const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}

fn check(status: u32) -> Result<()> {
    if status != 0 {
        return Err(io::Error::from_raw_os_error(status as i32).into());
    }
    Ok(())
}

pub(super) struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        unsafe { RegCloseKey(self.0) };
    }
}

impl Key {
    pub(super) fn open(path: &str, create: bool) -> Result<Option<Self>> {
        Self::open_access(path, create, KEY_QUERY_VALUE | KEY_SET_VALUE)
    }

    pub(super) fn read_only(path: &str) -> Result<Option<Self>> {
        Self::open_access(path, false, KEY_QUERY_VALUE)
    }

    fn open_access(path: &str, create: bool, access: u32) -> Result<Option<Self>> {
        let mut key = ptr::null_mut();
        let path = wide(path);
        let status = if create {
            unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    path.as_ptr(),
                    0,
                    ptr::null(),
                    0,
                    access,
                    ptr::null(),
                    &mut key,
                    ptr::null_mut(),
                )
            }
        } else {
            unsafe { RegOpenKeyExW(HKEY_CURRENT_USER, path.as_ptr(), 0, access, &mut key) }
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(status)?;
        Ok(Some(Self(key)))
    }

    pub(super) fn read(&self, name: &str) -> Result<Option<String>> {
        // Startup commands are bounded, but allow reading old longer entries
        // for cleanup. REG_SZ specifically prevents accidental % expansion.
        let mut buffer = vec![0_u16; 32_768];
        let mut bytes = (buffer.len() * 2) as u32;
        let status = unsafe {
            RegGetValueW(
                self.0,
                ptr::null(),
                wide(name).as_ptr(),
                RRF_RT_REG_SZ | RRF_NOEXPAND,
                ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut bytes,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        check(status)?;
        ensure!(
            bytes >= 2 && bytes.is_multiple_of(2),
            "invalid login registry string"
        );
        buffer.truncate(bytes as usize / 2);
        ensure!(
            buffer.pop() == Some(0) && !buffer.contains(&0),
            "invalid login registry string"
        );
        Ok(Some(String::from_utf16(&buffer)?))
    }

    pub(super) fn write(&self, name: &str, value: &str) -> Result<()> {
        if self.read(name)?.as_deref() == Some(value) {
            return Ok(());
        }
        let value = wide(value);
        check(unsafe {
            RegSetValueExW(
                self.0,
                wide(name).as_ptr(),
                0,
                REG_SZ,
                value.as_ptr().cast(),
                (value.len() * 2) as u32,
            )
        })
    }

    pub(super) fn remove(&self, name: &str) -> Result<()> {
        let status = unsafe { RegDeleteValueW(self.0, wide(name).as_ptr()) };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(());
        }
        check(status)
    }

    pub(super) fn names(&self) -> Result<Vec<String>> {
        let mut names = Vec::new();
        for index in 0.. {
            let mut buffer = vec![0_u16; 16_384];
            let mut len = buffer.len() as u32;
            let status = unsafe {
                RegEnumValueW(
                    self.0,
                    index,
                    buffer.as_mut_ptr(),
                    &mut len,
                    ptr::null(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };
            if status == ERROR_NO_MORE_ITEMS {
                break;
            }
            check(status)?;
            names.push(String::from_utf16(&buffer[..len as usize])?);
        }
        Ok(names)
    }
}

pub(super) fn ensure_login(name: &str, value: &str) -> Result<()> {
    if let Some(key) = Key::read_only(RUN_KEY)? {
        if key.read(name)?.as_deref() == Some(value) {
            return Ok(());
        }
    }
    Key::open(RUN_KEY, true)?
        .ok_or_else(|| anyhow::anyhow!("cannot open login registry"))?
        .write(name, value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_round_trip_is_idempotent_and_preserves_other_entries() {
        // Never register runnable entries in the real Run key during unit tests.
        let path = format!(r"Software\AIPass\Tests\{}", uuid::Uuid::new_v4());
        let key = Key::open(&path, true).unwrap().unwrap();
        key.write("other", "keep").unwrap();
        let value = r#""C:\中文 空格\app.exe" --login-start --vault "C:\%HOME% & vault""#;
        key.write("login", value).unwrap();
        key.write("login", value).unwrap();
        assert_eq!(key.read("login").unwrap().as_deref(), Some(value));
        assert_eq!(key.names().unwrap().len(), 2);
        key.remove("login").unwrap();
        key.remove("login").unwrap();
        assert_eq!(key.read("login").unwrap(), None);
        assert_eq!(key.read("other").unwrap().as_deref(), Some("keep"));
        drop(key);
        check(unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, wide(&path).as_ptr()) }).unwrap();
    }
}

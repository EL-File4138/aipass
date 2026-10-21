//! Verify the pipe object's owner before sending a token or deep link. A DACL on
//! our listener alone cannot protect a client from a foreign preclaimed name.
use anyhow::{ensure, Result};
use interprocess::{local_socket::Stream, os::windows::security_descriptor::SecurityDescriptor};
use std::{
    ffi::c_void,
    os::windows::io::{AsHandle, AsRawHandle, FromRawHandle, OwnedHandle},
    ptr::null_mut,
};
use windows_sys::Win32::{
    Foundation::{LocalFree, HANDLE},
    Security::{
        Authorization::{ConvertSidToStringSidW, GetSecurityInfo, SE_KERNEL_OBJECT},
        GetTokenInformation, TokenUser, OWNER_SECURITY_INFORMATION, TOKEN_QUERY, TOKEN_USER,
    },
    System::Threading::{GetCurrentProcess, OpenProcessToken},
};

pub(super) fn claim_listener(name: &str) -> Result<std::fs::File> {
    use sha2::{Digest, Sha256};
    use std::os::windows::fs::OpenOptionsExt;
    let profile = directories::BaseDirs::new()
        .ok_or_else(|| anyhow::anyhow!("Windows profile is unavailable"))?;
    // A fixed profile-local root also excludes competing RDP/service sessions
    // and callers with different runtime-directory overrides. The inherited
    // profile ACL protects it from foreign users; a crash releases the handle.
    let root = profile.data_local_dir().join("AIPass/instance-locks");
    std::fs::create_dir_all(&root)?;
    let path = root.join(format!("{:x}.lock", Sha256::digest(name.as_bytes())));
    std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .share_mode(0)
        .open(path)
        .map_err(|error| {
            if error.raw_os_error() == Some(32) {
                std::io::Error::new(
                    std::io::ErrorKind::AddrInUse,
                    "AIPass listener already has an owner",
                )
                .into()
            } else {
                error.into()
            }
        })
}

struct LocalMemory(*mut c_void);
impl Drop for LocalMemory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                LocalFree(self.0);
            }
        }
    }
}

fn sid_string(sid: *mut c_void) -> Result<String> {
    ensure!(!sid.is_null(), "Windows security identifier is missing");
    let mut text = null_mut();
    ensure!(
        unsafe { ConvertSidToStringSidW(sid, &mut text) } != 0,
        "could not read Windows security identifier"
    );
    let _memory = LocalMemory(text.cast());
    Ok(unsafe { widestring::U16CStr::from_ptr_str(text) }.to_string_lossy())
}

fn current_user_sid() -> Result<String> {
    let mut token: HANDLE = null_mut();
    ensure!(
        unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) } != 0,
        "could not open Windows user token"
    );
    let token = unsafe { OwnedHandle::from_raw_handle(token) };
    let mut length = 0;
    unsafe {
        GetTokenInformation(token.as_raw_handle(), TokenUser, null_mut(), 0, &mut length);
    }
    ensure!(
        length > 0 && length <= 64 * 1024,
        "invalid Windows user token length"
    );
    // TOKEN_USER contains pointers; retain native alignment and its backing SID.
    let mut storage = vec![0_usize; (length as usize).div_ceil(std::mem::size_of::<usize>())];
    ensure!(
        unsafe {
            GetTokenInformation(
                token.as_raw_handle(),
                TokenUser,
                storage.as_mut_ptr().cast(),
                length,
                &mut length,
            )
        } != 0,
        "could not read Windows user token"
    );
    let user = unsafe { &*storage.as_ptr().cast::<TOKEN_USER>() };
    sid_string(user.User.Sid)
}

pub(super) fn security_descriptor() -> Result<SecurityDescriptor> {
    let sid = current_user_sid()?;
    let text = widestring::U16CString::from_str(format!("O:{sid}D:P(D;;GA;;;NU)(A;;GA;;;{sid})"))?;
    Ok(SecurityDescriptor::deserialize(&text)?)
}

pub(super) fn verify_owner(stream: &Stream) -> Result<()> {
    let Stream::NamedPipe(pipe) = stream;
    let mut owner = null_mut();
    let mut descriptor = null_mut();
    let status = unsafe {
        GetSecurityInfo(
            pipe.as_handle().as_raw_handle(),
            SE_KERNEL_OBJECT,
            OWNER_SECURITY_INFORMATION,
            &mut owner,
            null_mut(),
            null_mut(),
            null_mut(),
            &mut descriptor,
        )
    };
    let _memory = LocalMemory(descriptor);
    if status != 0 {
        return Err(std::io::Error::from_raw_os_error(status as i32).into());
    }
    ensure!(
        sid_string(owner)? == current_user_sid()?,
        "local pipe belongs to another Windows user"
    );
    Ok(())
}

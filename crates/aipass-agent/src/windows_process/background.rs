//! Companions must not inherit the CLI/native host's output pipes.
//! Stdio::null alone still permits inheritance of unrelated open handles.
use std::{
    cmp::Ordering,
    ffi::{OsStr, OsString},
    io,
    os::windows::{
        ffi::OsStrExt,
        io::{FromRawHandle, OwnedHandle},
    },
    process::Command,
    ptr,
};
use windows_sys::Win32::{
    Globalization::CompareStringOrdinal,
    System::Threading::{
        CreateProcessW, CREATE_UNICODE_ENVIRONMENT, PROCESS_INFORMATION, STARTF_USESTDHANDLES,
        STARTUPINFOW,
    },
};

fn wide(value: &OsStr) -> io::Result<Vec<u16>> {
    let mut value: Vec<_> = value.encode_wide().collect();
    if value.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "NUL in companion launch value",
        ));
    }
    value.push(0);
    Ok(value)
}

fn compare(a: &OsStr, b: &OsStr) -> Ordering {
    let a: Vec<_> = a.encode_wide().collect();
    let b: Vec<_> = b.encode_wide().collect();
    match unsafe { CompareStringOrdinal(a.as_ptr(), a.len() as i32, b.as_ptr(), b.len() as i32, 1) }
    {
        1 => Ordering::Less,
        2 => Ordering::Equal,
        3 => Ordering::Greater,
        _ => a.cmp(&b),
    }
}

fn quote(value: &OsStr, result: &mut Vec<u16>) {
    result.push(b'"' as u16);
    let mut slashes = 0;
    for ch in value.encode_wide() {
        if ch == b'\\' as u16 {
            slashes += 1;
            continue;
        }
        result.extend(std::iter::repeat_n(
            b'\\' as u16,
            if ch == b'"' as u16 {
                slashes * 2 + 1
            } else {
                slashes
            },
        ));
        slashes = 0;
        result.push(ch);
    }
    result.extend(std::iter::repeat_n(b'\\' as u16, slashes * 2));
    result.push(b'"' as u16);
}

fn environment(command: &Command) -> io::Result<zeroize::Zeroizing<Vec<u16>>> {
    let mut values: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter(|(key, _)| {
            // CLI input is not configuration for a long-lived companion.
            !["AIPASS_MASTER_PASSWORD", "AIPASS_INPUT_API_KEY"]
                .iter()
                .any(|secret| compare(key, OsStr::new(secret)) == Ordering::Equal)
        })
        .collect();
    for (key, value) in command.get_envs() {
        values.retain(|(old, _)| compare(old, key) != Ordering::Equal);
        if let Some(value) = value {
            values.push((key.to_owned(), value.to_owned()));
        }
    }
    values.sort_by(|a, b| compare(&a.0, &b.0));
    let mut block = zeroize::Zeroizing::new(Vec::new());
    for (key, value) in values {
        let mut line = key;
        line.push("=");
        line.push(value);
        block.extend(wide(&line)?);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

/// For owned Agent/desktop companions with inherited base environment plus
/// explicit overrides. Deliberately inherits no handles and opens no console.
pub(crate) fn spawn(command: &Command) -> io::Result<u32> {
    // lpApplicationName does not search PATH, unlike std::process::Command.
    // Retain that discovery behavior for a desktop available only on PATH.
    let program = std::path::Path::new(command.get_program());
    let resolved = if program.is_file() {
        std::fs::canonicalize(program)?
    } else {
        std::env::var_os("PATH")
            .and_then(|path| {
                std::env::split_paths(&path)
                    .map(|dir| dir.join(program))
                    .find(|path| path.is_file())
            })
            .ok_or_else(|| {
                io::Error::new(io::ErrorKind::NotFound, "companion executable not found")
            })?
    };
    let application = wide(resolved.as_os_str())?;
    let mut arguments = Vec::new();
    for arg in std::iter::once(command.get_program()).chain(command.get_args()) {
        if !arguments.is_empty() {
            arguments.push(b' ' as u16);
        }
        quote(arg, &mut arguments);
    }
    if arguments.len() >= 32_767 || arguments.contains(&0) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid companion command line",
        ));
    }
    arguments.push(0);
    let environment = environment(command)?;
    let directory = command
        .get_current_dir()
        .map(|path| wide(path.as_os_str()))
        .transpose()?;
    let startup = STARTUPINFOW {
        cb: std::mem::size_of::<STARTUPINFOW>() as u32,
        dwFlags: STARTF_USESTDHANDLES,
        ..Default::default()
    };
    let mut process = PROCESS_INFORMATION::default();
    // Explicit null stdio and bInheritHandles=FALSE also exclude inheritable
    // handles installed by a calling runtime such as Node or a browser.
    if unsafe {
        CreateProcessW(
            application.as_ptr(),
            arguments.as_mut_ptr(),
            ptr::null(),
            ptr::null(),
            0,
            super::CREATE_NO_WINDOW | CREATE_UNICODE_ENVIRONMENT,
            environment.as_ptr().cast(),
            directory
                .as_ref()
                .map_or(ptr::null(), |value| value.as_ptr()),
            &startup,
            &mut process,
        )
    } == 0
    {
        return Err(io::Error::last_os_error());
    }
    let _process = unsafe { OwnedHandle::from_raw_handle(process.hProcess) };
    let _thread = unsafe { OwnedHandle::from_raw_handle(process.hThread) };
    Ok(process.dwProcessId)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_round_trip_through_the_windows_argument_parser() {
        use windows_sys::Win32::{Foundation::LocalFree, UI::Shell::CommandLineToArgvW};
        let values = [
            OsStr::new("fixture.exe"),
            OsStr::new("中文 & %PATH% \\\"trail\\"),
        ];
        let mut command = Vec::new();
        for value in values {
            if !command.is_empty() {
                command.push(32);
            }
            quote(value, &mut command);
        }
        command.push(0);
        let mut count = 0;
        let args = unsafe { CommandLineToArgvW(command.as_ptr(), &mut count) };
        assert!(!args.is_null());
        assert_eq!(count, 2);
        for (index, expected) in values.iter().enumerate() {
            let actual = unsafe { widestring::U16CStr::from_ptr_str(*args.add(index)) };
            assert_eq!(
                actual.as_slice(),
                expected.encode_wide().collect::<Vec<_>>()
            );
        }
        unsafe {
            LocalFree(args.cast());
        }
        assert_eq!(
            compare(OsStr::new("aipass_Ä"), OsStr::new("AIPASS_ä")),
            Ordering::Equal
        );
        let mut command = Command::new("fixture.exe");
        command.env("AIPASS_BG_FIXTURE", "中文 & %PATH%");
        let block = environment(&command).unwrap();
        assert!(String::from_utf16_lossy(&block).contains("AIPASS_BG_FIXTURE=中文 & %PATH%\0"));
        assert!(wide(OsStr::new("bad\0path")).is_err());
    }
}

//! Run entries are CreateProcess command lines, never CMD/PowerShell scripts.
use anyhow::{ensure, Result};

pub(super) fn command(binary: &str, vault: &str) -> Result<String> {
    ensure!(
        !binary.contains(['\0', '"']) && !vault.contains(['\0', '"']),
        "invalid Windows login path"
    );
    let command = format!("{} --login-start --vault {}", quote(binary), quote(vault));
    // Documented Run-key limit, measured in native UTF-16 code units.
    ensure!(command.encode_utf16().count() <= 260, "Windows login command exceeds the 260-character Run-key limit; choose shorter installation/vault paths");
    Ok(command)
}

pub(super) fn desktop_command(binary: &str, vault: &str) -> Result<String> {
    // The NSIS updater replays the previous process's argv, possibly through
    // Explorer without its environment. A new version must then reveal main.
    let value = format!(
        "{} --login-version {}",
        command(binary, vault)?,
        quote(env!("CARGO_PKG_VERSION"))
    );
    ensure!(value.encode_utf16().count() <= 260, "Windows login command exceeds the 260-character Run-key limit; choose shorter installation/vault paths");
    Ok(value)
}

fn quote(value: &str) -> String {
    // Paths cannot contain quotes. Only a trailing backslash needs doubling
    // so the closing quote is not consumed by CommandLineToArgvW/CRT parsing.
    let trailing = value.chars().rev().take_while(|ch| *ch == '\\').count();
    format!("\"{value}{}\"", "\\".repeat(trailing))
}

#[cfg(windows)]
pub(super) fn vault_for_command(binary: &str, value: &str) -> Option<String> {
    use windows_sys::Win32::{Foundation::LocalFree, UI::Shell::CommandLineToArgvW};
    let value = widestring::U16CString::from_str(value).ok()?;
    let mut count = 0;
    let argv = unsafe { CommandLineToArgvW(value.as_ptr(), &mut count) };
    if argv.is_null() {
        return None;
    }
    let args: Option<Vec<_>> = (0..count)
        .map(|index| unsafe {
            widestring::U16CStr::from_ptr_str(*argv.add(index as usize))
                .to_string()
                .ok()
        })
        .collect();
    unsafe {
        LocalFree(argv.cast());
    }
    let args = args?;
    if !matches!(args.len(), 4 | 6)
        || args[0] != binary
        || args[1] != "--login-start"
        || args[2] != "--vault"
        || args.len() == 6 && (args[4] != "--login-version" || args[5].is_empty())
    {
        return None;
    }
    Some(args[3].clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_command_keeps_unicode_metacharacters_and_trailing_slash_literal() {
        assert_eq!(
            command(r"C:\中文 & %HOME%\aipass.exe", "D:\\vault space\\").unwrap(),
            "\"C:\\中文 & %HOME%\\aipass.exe\" --login-start --vault \"D:\\vault space\\\\\""
        );
    }

    #[test]
    fn rejects_invalid_and_overlong_commands_before_registration() {
        assert!(command("app\0.exe", "vault").is_err());
        assert!(command("app.exe", "vault\" --flag").is_err());
        assert!(command("app.exe", &"😀".repeat(130)).is_err());
        let value = desktop_command("app.exe", "vault").unwrap();
        assert!(value.ends_with(&format!(
            "--login-version \"{}\"",
            env!("CARGO_PKG_VERSION")
        )));
    }

    #[cfg(windows)]
    #[test]
    fn uninstall_only_decodes_commands_owned_by_the_exact_executable() {
        let value = command(r"C:\中文\app.exe", "C:\\vault space\\").unwrap();
        assert_eq!(
            vault_for_command(r"C:\中文\app.exe", &value).as_deref(),
            Some("C:\\vault space\\")
        );
        assert!(vault_for_command(r"C:\other\app.exe", &value).is_none());
        assert!(vault_for_command(r"C:\中文\app.exe", &(value + " --extra")).is_none());
        let versioned = desktop_command(r"C:\中文\app.exe", "C:\\vault space\\").unwrap();
        assert_eq!(
            vault_for_command(r"C:\中文\app.exe", &versioned).as_deref(),
            Some("C:\\vault space\\")
        );
    }
}

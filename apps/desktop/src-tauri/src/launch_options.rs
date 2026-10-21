//! Narrow desktop login entry point, separate from deep-link arguments.
use anyhow::{bail, Result};
use std::{ffi::OsString, path::PathBuf};

pub(super) fn login_vault(args: &[OsString]) -> Result<Option<PathBuf>> {
    if args.first().is_none_or(|arg| arg != "--login-start") {
        return Ok(None);
    }
    if !matches!(args.len(), 3 | 5)
        || args[1] != "--vault"
        || !PathBuf::from(&args[2]).is_absolute()
    {
        bail!("invalid login-start arguments");
    }
    if args.len() == 5
        && (args[3] != "--login-version"
            || args[4]
                .to_str()
                .is_none_or(|value| semver::Version::parse(value).is_err()))
    {
        bail!("invalid login version");
    }
    Ok(Some(PathBuf::from(&args[2])))
}

fn login_target(args: &[OsString]) -> &'static str {
    if args.get(4).and_then(|value| value.to_str()) == Some(env!("CARGO_PKG_VERSION")) {
        "tray"
    } else {
        "main"
    }
}

pub(super) fn initialize() -> Result<bool> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(windows)]
    if args.len() == 1 && args[0] == "--remove-login-start" {
        aipass_agent::autostart::uninstall_windows_tray_entries(&std::env::current_exe()?)?;
        return Ok(false);
    }
    if let Some(vault) = login_vault(&args)? {
        // Called first in run(), before logging, IPC or Tauri starts threads.
        if std::env::var_os("AIPASS_WINDOW_TARGET").is_none() {
            std::env::set_var("AIPASS_WINDOW_TARGET", login_target(&args));
        }
        std::env::set_var("AIPASS_VAULT_DIR", vault);
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_entry_requires_exact_arguments_and_an_absolute_vault() {
        let root = tempfile::tempdir().unwrap();
        let vault = root.path().join("中文 %HOME% & space");
        let args = vec![
            "--login-start".into(),
            "--vault".into(),
            vault.clone().into_os_string(),
        ];
        assert_eq!(login_vault(&args).unwrap(), Some(vault));
        let mut extra = args.clone();
        extra.push("aipass://launch/main".into());
        assert!(login_vault(&extra).is_err());
        assert!(
            login_vault(&["--login-start".into(), "--vault".into(), "relative".into()]).is_err()
        );
        assert_eq!(login_vault(&["aipass://launch/main".into()]).unwrap(), None);
    }

    #[test]
    fn replayed_login_arguments_reveal_a_new_version_without_inherited_environment() {
        let root = tempfile::tempdir().unwrap();
        let mut args = vec![
            "--login-start".into(),
            "--vault".into(),
            root.path().as_os_str().to_owned(),
            "--login-version".into(),
            env!("CARGO_PKG_VERSION").into(),
        ];
        assert_eq!(login_vault(&args).unwrap(), Some(root.path().to_path_buf()));
        assert_eq!(login_target(&args), "tray");
        args[4] = "0.0.1".into();
        assert_eq!(login_target(&args), "main");
        assert_eq!(login_target(&args[..3]), "main");
        args[4] = "invalid-version".into();
        assert!(login_vault(&args).is_err());
    }
}

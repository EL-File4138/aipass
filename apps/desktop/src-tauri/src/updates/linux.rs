//! Linux installation policy. Never collect sudo passwords or retry after cancellation.
use std::{
    fs,
    io::Write,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
use tauri::{
    utils::{config::BundleType, platform::bundle_type},
    Manager,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Kind {
    AppImage,
    Deb,
    Rpm,
}

pub(super) fn kind() -> Result<Kind, String> {
    match bundle_type() {
        Some(BundleType::AppImage) => Ok(Kind::AppImage),
        Some(BundleType::Deb) => Ok(Kind::Deb),
        Some(BundleType::Rpm) => Ok(Kind::Rpm),
        _ => Err("update_installation_unsupported".into()),
    }
}

pub(super) fn target() -> Result<String, String> {
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        _ => return Err("update_installation_unsupported".into()),
    };
    let format = match kind()? {
        Kind::AppImage => "appimage",
        Kind::Deb => "deb",
        Kind::Rpm => "rpm",
    };
    // A generic Linux entry might point to another package format. Never use it.
    Ok(format!("linux-{arch}-{format}"))
}

pub(super) enum Plan {
    AppImage,
    System {
        kind: Kind,
        package: tempfile::NamedTempFile,
    },
}

pub(super) fn prepare(app: &tauri::AppHandle, bytes: &[u8]) -> Result<Plan, String> {
    let kind = kind()?;
    validate_format(kind, bytes)?;
    if kind == Kind::AppImage {
        let image = app
            .env()
            .appimage
            .map(PathBuf::from)
            .ok_or("update_installation_unsupported")?;
        writable_appimage(&image)?;
        return Ok(Plan::AppImage);
    }
    let manager = manager(kind);
    if !Path::new(manager).is_file() || !Path::new("/usr/bin/pkexec").is_file() {
        return Err("update_authorization_unavailable".into());
    }
    let mut package = tempfile::Builder::new()
        .prefix("aipass-verified-update-")
        .suffix(if kind == Kind::Deb { ".deb" } else { ".rpm" })
        .tempfile()
        .map_err(super::cache_io_error)?;
    package.write_all(bytes).map_err(super::cache_io_error)?;
    package
        .as_file()
        .sync_all()
        .map_err(super::cache_io_error)?;
    Ok(Plan::System { kind, package })
}

fn validate_format(kind: Kind, bytes: &[u8]) -> Result<(), String> {
    let valid = match kind {
        Kind::AppImage => compatible_appimage(bytes),
        Kind::Deb => bytes.starts_with(b"!<arch>\n"),
        Kind::Rpm => bytes.starts_with(&[0xed, 0xab, 0xee, 0xdb]),
    };
    if valid {
        Ok(())
    } else {
        Err("update_package_invalid".into())
    }
}

fn compatible_appimage(bytes: &[u8]) -> bool {
    let machine = match std::env::consts::ARCH {
        "aarch64" => [183, 0],
        "x86_64" => [62, 0],
        _ => return false,
    };
    bytes.len() >= 64
        && bytes.starts_with(b"\x7fELF\x02\x01\x01")
        && bytes.get(8..11) == Some(b"AI\x02")
        && matches!(bytes.get(16..18), Some([2 | 3, 0]))
        && bytes.get(18..20) == Some(machine.as_slice())
}

fn writable_appimage(image: &Path) -> Result<(), String> {
    let image = image
        .canonicalize()
        .map_err(|_| "update_install_permission_denied")?;
    let metadata = fs::metadata(&image).map_err(|_| "update_install_permission_denied")?;
    if !metadata.is_file() || metadata.uid() != rustix::process::geteuid().as_raw() {
        return Err("update_install_permission_denied".into());
    }
    let parent = image.parent().ok_or("update_install_permission_denied")?;
    // Test real directory access (ACLs and read-only mounts), not only mode bits.
    tempfile::Builder::new()
        .prefix(".aipass-update-probe-")
        .tempfile_in(parent)
        .map_err(|_| "update_install_permission_denied")?;
    Ok(())
}

fn manager(kind: Kind) -> &'static str {
    if kind == Kind::Deb {
        "/usr/bin/dpkg"
    } else {
        "/usr/bin/rpm"
    }
}

pub(super) fn install(
    plan: Plan,
    update: &tauri_plugin_updater::Update,
    bytes: &[u8],
) -> Result<(), String> {
    match plan {
        Plan::AppImage => update.install(bytes).map_err(|error| match error {
            tauri_plugin_updater::Error::Io(ref e) if super::permission_error(e) => {
                "update_install_permission_denied".into()
            }
            other => super::updater_error(other),
        }),
        Plan::System { kind, package } => {
            // pkexec owns authorization. AIPass never reads or retains a password.
            let status = Command::new("/usr/bin/pkexec")
                .args([manager(kind), if kind == Kind::Deb { "-i" } else { "-U" }])
                .arg(package.path())
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .map_err(|_| "update_authorization_unavailable")?;
            authorization_result(status.code())
        }
    }
}

fn authorization_result(code: Option<i32>) -> Result<(), String> {
    match code {
        Some(0) => Ok(()),
        Some(126) => Err("update_install_cancelled".into()),
        Some(127) => Err("update_install_permission_denied".into()),
        _ => Err("update_install_failed".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn package_formats_and_authorization_outcomes_fail_closed() {
        let mut image = [0; 64];
        image[..11].copy_from_slice(b"\x7fELF\x02\x01\x01\x00AI\x02");
        image[16] = 2;
        image[18] = if cfg!(target_arch = "aarch64") {
            183
        } else {
            62
        };
        assert!(compatible_appimage(&image));
        let mut other_arch = image;
        other_arch[18] = if image[18] == 183 { 62 } else { 183 };
        assert!(!compatible_appimage(&other_arch));
        assert!(!compatible_appimage(&image[..20]));
        for (kind, data) in [
            (Kind::Deb, b"!<arch>\n".as_slice()),
            (Kind::Rpm, &[0xed, 0xab, 0xee, 0xdb]),
            (Kind::AppImage, image.as_slice()),
        ] {
            assert!(validate_format(kind, data).is_ok());
            for other in [Kind::Deb, Kind::Rpm, Kind::AppImage]
                .into_iter()
                .filter(|other| *other != kind)
            {
                assert!(validate_format(other, data).is_err());
            }
        }
        assert_eq!(
            authorization_result(Some(126)),
            Err("update_install_cancelled".into())
        );
        assert_eq!(
            authorization_result(Some(127)),
            Err("update_install_permission_denied".into())
        );
        assert!(authorization_result(Some(1)).is_err());
        assert!(authorization_result(None).is_err());
        assert!(authorization_result(Some(0)).is_ok());
    }
    #[test]
    fn appimage_permissions_are_checked_before_stopping_the_runtime() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let image = dir.path().join("AIPass.AppImage");
        fs::write(&image, b"fixture").unwrap();
        assert!(writable_appimage(&image).is_ok());
        if !rustix::process::geteuid().is_root() {
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o555)).unwrap();
            assert_eq!(
                writable_appimage(&image),
                Err("update_install_permission_denied".into())
            );
            fs::set_permissions(dir.path(), fs::Permissions::from_mode(0o700)).unwrap();
        }
        assert!(writable_appimage(&dir.path().join("absent")).is_err());
    }
}

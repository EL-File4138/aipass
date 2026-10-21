//! Windows npm shims own a node.exe child; killing only cmd.exe leaves work alive.
use std::os::windows::process::CommandExt;
use std::{
    path::PathBuf,
    process::{Command, Stdio},
};

pub(crate) const CREATE_NO_WINDOW: u32 = 0x0800_0000;
mod background;
pub(crate) use background::spawn;

/// Retain the authenticated pipe peer's process handle before requesting exit.
/// Holding the handle prevents PID reuse; waiting never terminates a process.
pub(crate) fn peer_process(
    stream: &interprocess::local_socket::Stream,
) -> anyhow::Result<Option<std::os::windows::io::OwnedHandle>> {
    use interprocess::local_socket::traits::StreamCommon as _;
    use std::os::windows::io::FromRawHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_SYNCHRONIZE};
    let pid = stream
        .peer_creds()?
        .pid()
        .ok_or_else(|| anyhow::anyhow!("Agent peer PID is missing"))?;
    // In-process test servers and SCM control callbacks stop their own server
    // thread; waiting for our own process here would deadlock that shutdown.
    if pid == std::process::id() {
        return Ok(None);
    }
    let handle = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
    anyhow::ensure!(
        !handle.is_null(),
        "cannot retain the Agent process for shutdown"
    );
    Ok(Some(unsafe {
        std::os::windows::io::OwnedHandle::from_raw_handle(handle)
    }))
}

pub(crate) fn wait_for_exit(process: &std::os::windows::io::OwnedHandle) -> anyhow::Result<()> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::{Foundation::WAIT_OBJECT_0, System::Threading::WaitForSingleObject};
    anyhow::ensure!(
        unsafe { WaitForSingleObject(process.as_raw_handle(), 8000) } == WAIT_OBJECT_0,
        "Agent process did not exit within eight seconds"
    );
    Ok(())
}

/// Call while retaining the live Child handle and after try_wait. Windows keeps
/// that PID reserved until the handle is released, including if the child exits.
pub(crate) fn terminate_tree(pid: u32) {
    let tool = std::env::var_os("SystemRoot")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
        .join("System32/taskkill.exe");
    let _ = Command::new(tool)
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    #[test]
    #[ignore = "subprocess fixture invoked only by tree termination test"]
    fn child_fixture() {
        let root = PathBuf::from(std::env::var_os("AIPASS_TREE_FIXTURE").unwrap());
        if std::env::var("AIPASS_TREE_ROLE").as_deref() == Ok("parent") {
            let mut child = Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "windows_process::tests::child_fixture",
                    "--ignored",
                ])
                .env("AIPASS_TREE_ROLE", "child")
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap();
            std::fs::write(root.join("pid"), child.id().to_string()).unwrap();
            let _ = child.wait();
        } else {
            std::fs::write(root.join("ready"), b"ready").unwrap();
        }
        std::thread::sleep(Duration::from_secs(30));
    }

    #[test]
    fn termination_reaps_a_vendor_shims_descendants() {
        use windows_sys::Win32::{
            Foundation::{CloseHandle, WAIT_OBJECT_0},
            System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE},
        };
        let root = tempfile::tempdir().unwrap();
        let mut parent = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "windows_process::tests::child_fixture",
                "--ignored",
            ])
            .env("AIPASS_TREE_FIXTURE", root.path())
            .env("AIPASS_TREE_ROLE", "parent")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(10);
        while (!root.path().join("ready").exists() || !root.path().join("pid").exists())
            && Instant::now() < deadline
        {
            std::thread::sleep(Duration::from_millis(20));
        }
        if !root.path().join("ready").exists() {
            terminate_tree(parent.id());
            let _ = parent.wait();
            panic!("process tree fixture did not start");
        }
        let pid: u32 = std::fs::read_to_string(root.path().join("pid"))
            .unwrap()
            .parse()
            .unwrap();
        // Retain the descendant's actual process handle, rather than polling a
        // PID that Windows might later reuse for an unrelated process.
        let descendant = unsafe { OpenProcess(PROCESS_SYNCHRONIZE, 0, pid) };
        assert!(!descendant.is_null());
        terminate_tree(parent.id());
        parent.wait().unwrap();
        let result = unsafe { WaitForSingleObject(descendant, 5000) };
        unsafe {
            CloseHandle(descendant);
        }
        assert_eq!(
            result, WAIT_OBJECT_0,
            "vendor descendant survived cancellation"
        );
    }
}

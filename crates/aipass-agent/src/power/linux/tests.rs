use super::*;
use std::io::{BufRead, BufReader};
use std::process::{Child, Command, Stdio};

struct PrivateBus {
    _root: tempfile::TempDir,
    child: Child,
    address: String,
}

impl PrivateBus {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let mut child = Command::new("dbus-daemon")
            .args(["--session", "--nofork", "--print-address=1"])
            .arg(format!(
                "--address=unix:path={}",
                root.path().join("bus").display()
            ))
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let mut address = String::new();
        BufReader::new(child.stdout.take().unwrap())
            .read_line(&mut address)
            .unwrap();
        Self {
            _root: root,
            child,
            address: address.trim().into(),
        }
    }

    async fn connect(&self) -> Connection {
        zbus::connection::Builder::address(self.address.as_str())
            .unwrap()
            .build()
            .await
            .unwrap()
    }
}

impl Drop for PrivateBus {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

fn signal(interface: &str, member: &str, active: bool) -> Message {
    Message::signal("/org/freedesktop/ScreenSaver", interface, member)
        .unwrap()
        .build(&active)
        .unwrap()
}

#[test]
fn only_active_lock_and_sleep_signals_trigger_their_policy() {
    for name in SCREENSAVERS {
        assert_eq!(
            reason(&signal(name, "ActiveChanged", true), Bus::Session),
            Some(LockReason::ScreenLock)
        );
        assert_eq!(
            reason(&signal(name, "ActiveChanged", false), Bus::Session),
            None
        );
    }
    assert_eq!(
        reason(
            &signal("org.freedesktop.login1.Manager", "PrepareForSleep", true),
            Bus::System
        ),
        Some(LockReason::SystemSleep)
    );
    assert_eq!(
        reason(
            &signal("org.freedesktop.login1.Manager", "PrepareForSleep", false),
            Bus::System
        ),
        None
    );
    assert_eq!(
        reason(
            &signal("org.example.Unrelated", "ActiveChanged", true),
            Bus::Session
        ),
        None
    );
    let mut policy = SessionPolicy::default();
    assert!(policy_allows(&policy, &LockReason::ScreenLock));
    assert!(policy_allows(&policy, &LockReason::SystemSleep));
    policy.lock_on_screen_lock = false;
    assert!(!policy_allows(&policy, &LockReason::ScreenLock));
    assert!(policy_allows(&policy, &LockReason::SystemSleep));
    policy.lock_on_sleep = false;
    assert!(!policy_allows(&policy, &LockReason::SystemSleep));
}

#[tokio::test]
async fn session_bus_filters_impostor_senders_and_delivers_real_signals() {
    let bus = PrivateBus::new();
    let monitor = bus.connect().await;
    let service = bus.connect().await;
    service
        .request_name("org.freedesktop.ScreenSaver")
        .await
        .unwrap();
    let mut stream = signals(&monitor, Bus::Session).await.unwrap();
    let impostor = bus.connect().await;
    impostor
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/ScreenSaver",
            "org.freedesktop.ScreenSaver",
            "ActiveChanged",
            &true,
        )
        .await
        .unwrap();
    assert!(
        tokio::time::timeout(Duration::from_millis(100), stream.next())
            .await
            .is_err()
    );
    service
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/ScreenSaver",
            "org.freedesktop.ScreenSaver",
            "ActiveChanged",
            &false,
        )
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(reason(&message, Bus::Session), None);
    service
        .emit_signal(
            None::<&str>,
            "/org/freedesktop/ScreenSaver",
            "org.freedesktop.ScreenSaver",
            "ActiveChanged",
            &true,
        )
        .await
        .unwrap();
    let message = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert_eq!(reason(&message, Bus::Session), Some(LockReason::ScreenLock));
    drop(bus);
    let next = tokio::time::timeout(Duration::from_secs(2), stream.next())
        .await
        .unwrap();
    assert!(
        !matches!(next, Some(Ok(_))),
        "desktop bus loss must end monitoring"
    );
}

struct LoginSession {
    uid: u32,
}
#[zbus::interface(name = "org.freedesktop.login1.Session")]
impl LoginSession {
    #[zbus(property)]
    fn user(&self) -> (u32, zbus::zvariant::OwnedObjectPath) {
        (
            self.uid,
            "/org/freedesktop/login1/user/test".try_into().unwrap(),
        )
    }
}

#[tokio::test]
async fn logind_lock_only_applies_to_sessions_owned_by_this_user() {
    let bus = PrivateBus::new();
    let uid = rustix::process::getuid().as_raw();
    let _service = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.freedesktop.login1")
        .unwrap()
        .serve_at("/org/freedesktop/login1/session/ours", LoginSession { uid })
        .unwrap()
        .serve_at(
            "/org/freedesktop/login1/session/other",
            LoginSession {
                uid: uid.wrapping_add(1),
            },
        )
        .unwrap()
        .build()
        .await
        .unwrap();
    let monitor = bus.connect().await;
    for (path, expected) in [("ours", true), ("other", false)] {
        let message = Message::signal(
            format!("/org/freedesktop/login1/session/{path}"),
            "org.freedesktop.login1.Session",
            "Lock",
        )
        .unwrap()
        .build(&())
        .unwrap();
        assert_eq!(reason(&message, Bus::System), Some(LockReason::ScreenLock));
        assert_eq!(own_session(&monitor, &message).await, expected);
    }
}

struct ScreenSaver;
#[zbus::interface(name = "org.freedesktop.ScreenSaver")]
impl ScreenSaver {
    fn get_active(&self) -> bool {
        true
    }
    fn get_session_idle_time(&self) -> u32 {
        42
    }
}

#[tokio::test]
async fn desktop_idle_time_is_converted_from_seconds_to_milliseconds() {
    let bus = PrivateBus::new();
    let _service = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.freedesktop.ScreenSaver")
        .unwrap()
        .serve_at("/org/freedesktop/ScreenSaver", ScreenSaver)
        .unwrap()
        .build()
        .await
        .unwrap();
    assert_eq!(idle_millis(&bus.connect().await).await, Some(42_000));
    assert!(screen_active(&bus.connect().await).await);
}

struct PowerManager(std::sync::Arc<std::sync::Mutex<Option<std::os::unix::net::UnixStream>>>);
#[zbus::interface(name = "org.freedesktop.login1.Manager")]
impl PowerManager {
    fn inhibit(
        &self,
        what: &str,
        _who: &str,
        _why: &str,
        mode: &str,
    ) -> zbus::fdo::Result<zbus::zvariant::OwnedFd> {
        if what != "sleep" || mode != "delay" {
            return Err(zbus::fdo::Error::InvalidArgs(
                "expected sleep delay inhibitor".into(),
            ));
        }
        let (fd, peer) = std::os::unix::net::UnixStream::pair().unwrap();
        *self.0.lock().unwrap() = Some(peer);
        Ok(std::os::fd::OwnedFd::from(fd).into())
    }
}

#[tokio::test]
async fn delay_inhibitor_holds_and_releases_the_logind_descriptor() {
    use std::io::Read;
    let bus = PrivateBus::new();
    let peer = std::sync::Arc::new(std::sync::Mutex::new(None));
    let _service = zbus::connection::Builder::address(bus.address.as_str())
        .unwrap()
        .name("org.freedesktop.login1")
        .unwrap()
        .serve_at("/org/freedesktop/login1", PowerManager(peer.clone()))
        .unwrap()
        .build()
        .await
        .unwrap();
    let connection = bus.connect().await;
    let inhibitor = sleep_inhibitor(&connection)
        .await
        .expect("must acquire delay inhibitor");
    let mut peer = peer.lock().unwrap().take().unwrap();
    peer.set_nonblocking(true).unwrap();
    assert_eq!(
        peer.read(&mut [0]).unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    drop(inhibitor);
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            match peer.read(&mut [0]) {
                Ok(0) => break,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    tokio::time::sleep(Duration::from_millis(10)).await
                }
                other => panic!("unexpected inhibitor descriptor state: {other:?}"),
            }
        }
    })
    .await
    .unwrap();
}

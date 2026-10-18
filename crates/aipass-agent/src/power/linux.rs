use crate::session::{current_policy, lock_session, AgentState};
use aipass_agent_protocol::{LockReason, SessionPolicy};
use futures_util::{stream::SelectAll, StreamExt};
use std::collections::HashMap;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, LazyLock,
};
use std::time::{Duration, Instant};
use zbus::{Connection, Message, MessageStream};

const SCREENSAVERS: &[&str] = &[
    "org.freedesktop.ScreenSaver",
    "org.gnome.ScreenSaver",
    "org.xfce.ScreenSaver",
    "org.mate.ScreenSaver",
    "org.cinnamon.ScreenSaver",
];
static START: LazyLock<Instant> = LazyLock::new(Instant::now);
static IDLE_MILLIS: AtomicU64 = AtomicU64::new(u64::MAX);
static IDLE_UPDATED: AtomicU64 = AtomicU64::new(0);

#[derive(Clone, Copy, PartialEq)]
enum Bus {
    Session,
    System,
}

pub fn spawn_power_watcher(state: Arc<AgentState>) {
    std::thread::spawn(move || {
        let Ok(runtime) = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        else {
            return;
        };
        runtime.block_on(async {
            tokio::join!(watch(&state, Bus::Session), watch(&state, Bus::System));
        });
    });
}

pub fn system_idle_seconds() -> Option<f64> {
    let updated = IDLE_UPDATED.load(Ordering::Relaxed);
    let millis = IDLE_MILLIS.load(Ordering::Relaxed);
    (updated > 0
        && START.elapsed().as_secs().saturating_sub(updated - 1) <= 15
        && millis != u64::MAX)
        .then_some(millis as f64 / 1000.0)
}

fn policy_allows(policy: &SessionPolicy, reason: &LockReason) -> bool {
    match reason {
        LockReason::ScreenLock => policy.lock_on_screen_lock,
        LockReason::SystemSleep => policy.lock_on_sleep,
        _ => false,
    }
}

fn apply(state: &Arc<AgentState>, reason: LockReason) {
    if !state.shutdown.load(Ordering::Relaxed)
        && current_policy(state)
            .map(|p| policy_allows(&p, &reason))
            .unwrap_or(true)
    {
        lock_session(state, reason);
    }
}

async fn watch(state: &Arc<AgentState>, bus: Bus) {
    while !state.shutdown.load(Ordering::Relaxed) {
        let connect = async {
            match bus {
                Bus::Session => Connection::session().await,
                Bus::System => Connection::system().await,
            }
        };
        if let Ok(Ok(connection)) = tokio::time::timeout(Duration::from_secs(2), connect).await {
            if let Ok(mut streams) = signals(&connection, bus).await {
                crate::logging::write_component_log(
                    crate::logging::AGENT_LOG,
                    "INFO",
                    if bus == Bus::Session {
                        "Linux session lock monitor connected"
                    } else {
                        "Linux logind power monitor connected"
                    },
                );
                let mut idle_tick = tokio::time::interval(Duration::from_secs(5));
                let mut sleeping = false;
                let mut inhibitor = None;
                loop {
                    tokio::select! {
                        item = streams.next() => {
                            let Some(Ok(message)) = item else { break; };
                            let sleep = sleep_transition(&message, bus);
                            if let Some(reason) = reason(&message, bus) {
                                if bus == Bus::Session || reason == LockReason::SystemSleep || own_session(&connection, &message).await {
                                    apply(state, reason);
                                }
                            }
                            if let Some(active) = sleep {
                                sleeping = active;
                                // logind waits for this descriptor to close.
                                // Release it only after the vault is locked.
                                if active { inhibitor = None; }
                            }
                        }
                        _ = idle_tick.tick() => {
                            if state.shutdown.load(Ordering::Relaxed) { return; }
                            if bus == Bus::Session {
                                if screen_active(&connection).await { apply(state, LockReason::ScreenLock); }
                                let idle = idle_millis(&connection).await;
                                IDLE_MILLIS.store(idle.unwrap_or(u64::MAX), Ordering::Relaxed);
                                IDLE_UPDATED.store(START.elapsed().as_secs() + 1, Ordering::Relaxed);
                            } else if !sleeping && inhibitor.is_none() {
                                inhibitor = sleep_inhibitor(&connection).await;
                            }
                        }
                    }
                }
                if bus == Bus::Session {
                    IDLE_MILLIS.store(u64::MAX, Ordering::Relaxed);
                    // Losing the desktop bus ends our ability to observe its
                    // lock state. Treat logout/bus loss as a screen lock.
                    apply(state, LockReason::ScreenLock);
                }
            }
        }
        for _ in 0..5 {
            if state.shutdown.load(Ordering::Relaxed) {
                return;
            }
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }
}

fn sleep_transition(message: &Message, bus: Bus) -> Option<bool> {
    let header = message.header();
    (bus == Bus::System
        && header.interface().map(|v| v.as_str()) == Some("org.freedesktop.login1.Manager")
        && header.member().map(|v| v.as_str()) == Some("PrepareForSleep"))
    .then(|| message.body().deserialize::<bool>().ok())
    .flatten()
}

async fn sleep_inhibitor(connection: &Connection) -> Option<zbus::zvariant::OwnedFd> {
    let call = connection.call_method(
        Some("org.freedesktop.login1"),
        "/org/freedesktop/login1",
        Some("org.freedesktop.login1.Manager"),
        "Inhibit",
        &("sleep", "AIPass", "Lock vault before sleep", "delay"),
    );
    tokio::time::timeout(Duration::from_secs(1), call)
        .await
        .ok()?
        .ok()?
        .body()
        .deserialize()
        .ok()
}

async fn screen_active(connection: &Connection) -> bool {
    for name in SCREENSAVERS {
        let path = format!("/{}", name.replace('.', "/"));
        let call =
            connection.call_method(Some(*name), path.as_str(), Some(*name), "GetActive", &());
        if let Ok(Ok(reply)) = tokio::time::timeout(Duration::from_millis(300), call).await {
            if reply.body().deserialize::<bool>().ok() == Some(true) {
                return true;
            }
        }
    }
    false
}

async fn signals(connection: &Connection, bus: Bus) -> zbus::Result<SelectAll<MessageStream>> {
    let rules = if bus == Bus::Session {
        SCREENSAVERS
            .iter()
            .map(|name| {
                format!("type='signal',sender='{name}',interface='{name}',member='ActiveChanged'")
            })
            .collect::<Vec<_>>()
    } else {
        vec![
            "type='signal',sender='org.freedesktop.login1',interface='org.freedesktop.login1.Manager',member='PrepareForSleep'".into(),
            "type='signal',sender='org.freedesktop.login1',interface='org.freedesktop.login1.Session',member='Lock'".into(),
            "type='signal',sender='org.freedesktop.login1',interface='org.freedesktop.DBus.Properties',member='PropertiesChanged',arg0='org.freedesktop.login1.Session'".into(),
        ]
    };
    let mut streams = SelectAll::new();
    for rule in rules {
        streams.push(MessageStream::for_match_rule(rule.as_str(), connection, Some(16)).await?);
    }
    Ok(streams)
}

fn reason(message: &Message, bus: Bus) -> Option<LockReason> {
    let header = message.header();
    let interface = header.interface()?.as_str();
    let member = header.member()?.as_str();
    if bus == Bus::Session {
        return (SCREENSAVERS.contains(&interface)
            && member == "ActiveChanged"
            && message.body().deserialize::<bool>().ok() == Some(true))
        .then_some(LockReason::ScreenLock);
    }
    match (interface, member) {
        ("org.freedesktop.login1.Manager", "PrepareForSleep") => {
            (message.body().deserialize::<bool>().ok() == Some(true))
                .then_some(LockReason::SystemSleep)
        }
        ("org.freedesktop.login1.Session", "Lock") => Some(LockReason::ScreenLock),
        ("org.freedesktop.DBus.Properties", "PropertiesChanged") => {
            let (name, changed, _): (
                String,
                HashMap<String, zbus::zvariant::OwnedValue>,
                Vec<String>,
            ) = message.body().deserialize().ok()?;
            (name == "org.freedesktop.login1.Session"
                && changed
                    .get("LockedHint")
                    .and_then(|value| bool::try_from(value).ok())
                    == Some(true))
            .then_some(LockReason::ScreenLock)
        }
        _ => None,
    }
}

async fn own_session(connection: &Connection, message: &Message) -> bool {
    let header = message.header();
    let Some(path) = header.path() else {
        return false;
    };
    let request = async {
        let proxy = zbus::Proxy::new(
            connection,
            "org.freedesktop.login1",
            path.as_str(),
            "org.freedesktop.login1.Session",
        )
        .await?;
        proxy
            .get_property::<(u32, zbus::zvariant::OwnedObjectPath)>("User")
            .await
    };
    matches!(tokio::time::timeout(Duration::from_secs(1), request).await,
        Ok(Ok((uid, _))) if uid == rustix::process::getuid().as_raw())
}

async fn idle_millis(connection: &Connection) -> Option<u64> {
    let mut sources = vec![(
        "org.gnome.Mutter.IdleMonitor",
        "/org/gnome/Mutter/IdleMonitor/Core".to_string(),
        "GetIdletime",
        true,
    )];
    sources.extend(SCREENSAVERS.iter().map(|name| {
        (
            *name,
            format!("/{}", name.replace('.', "/")),
            "GetSessionIdleTime",
            false,
        )
    }));
    for (name, path, method, milliseconds) in sources {
        let call = connection.call_method(Some(name), path.as_str(), Some(name), method, &());
        if let Ok(Ok(reply)) = tokio::time::timeout(Duration::from_millis(300), call).await {
            let value = reply
                .body()
                .deserialize::<u64>()
                .ok()
                .or_else(|| reply.body().deserialize::<u32>().ok().map(u64::from));
            if let Some(value) = value {
                return if milliseconds {
                    Some(value)
                } else {
                    value.checked_mul(1000)
                };
            }
        }
    }
    None
}

#[cfg(test)]
mod tests;

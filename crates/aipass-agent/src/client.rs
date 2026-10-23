use crate::ipc;
use crate::launcher;
use crate::paths::{canonical_vault_dir, default_vault_dir, namespace_for_vault_dir};
#[cfg(target_os = "windows")]
use crate::windows_service;
#[cfg(not(windows))]
use aipass_agent_protocol::{read_frame, write_frame};
use aipass_agent_protocol::{
    AgentErrorCode, AgentRequest, AgentResponse, AuthenticatedAgentRequest, SessionStatus,
    AGENT_PROTOCOL_VERSION,
};
use anyhow::Result;
use serde::de::DeserializeOwned;
use std::io::ErrorKind;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

const AGENT_READY_TIMEOUT: Duration = Duration::from_secs(15);
/// The first sync after launch can legitimately outlast the boot timeout;
/// readiness waits for it, but only within this extra bounded grace window.
const INITIAL_SYNC_READY_GRACE: Duration = Duration::from_secs(120);
const AGENT_READY_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(target_os = "macos")]
const AUTOSTART_RECOVERY_GRACE: Duration = Duration::from_secs(3);
/// A send should never take as long as the work behind a response.
const REQUEST_SEND_TIMEOUT: Duration = Duration::from_secs(30);

// Desktop setup warms the resident agent while the first frontend request may
// also call ensure_running. Serialize those paths so they cannot both trigger
// LaunchAgent repair and restart the supervisor underneath each other.
static AGENT_START_LOCK: Mutex<()> = Mutex::new(());

fn apply_request_timeouts(
    stream: &interprocess::local_socket::Stream,
    response_timeout: Duration,
) -> std::result::Result<(), AgentCommandError> {
    use interprocess::local_socket::traits::Stream as _;
    #[cfg(windows)]
    let result = {
        let _ = response_timeout;
        stream.set_nonblocking(true)
    };
    #[cfg(not(windows))]
    let result = stream
        .set_send_timeout(Some(REQUEST_SEND_TIMEOUT))
        .and_then(|()| stream.set_recv_timeout(Some(response_timeout)));
    result.map_err(|err| AgentCommandError {
        code: Some(AgentErrorCode::Internal),
        message: format!("failed to set agent request timeout: {err}"),
    })
}

/// A timed-out agent is reported as unavailable rather than an internal fault,
/// so callers retry or fall back to their "agent not reachable" path.
fn timeout_aware_error(err: anyhow::Error) -> AgentCommandError {
    let timed_out = err
        .downcast_ref::<std::io::Error>()
        .is_some_and(|err| matches!(err.kind(), ErrorKind::TimedOut | ErrorKind::WouldBlock));
    if timed_out {
        return AgentCommandError {
            code: Some(AgentErrorCode::ServiceUnavailable),
            message: format!("agent did not respond in time: {err}"),
        };
    }
    AgentCommandError::internal(err)
}

#[derive(Clone, Debug)]
pub struct AgentClientConfig {
    pub vault_dir: PathBuf,
    pub namespace: String,
}

impl AgentClientConfig {
    pub fn for_vault(vault_dir: PathBuf) -> Result<Self> {
        let vault_dir = canonical_vault_dir(vault_dir)?;
        let namespace = namespace_for_vault_dir(&vault_dir)?;
        Ok(Self {
            vault_dir,
            namespace,
        })
    }

    pub fn default_vault() -> Result<Self> {
        Self::for_vault(default_vault_dir()?)
    }
}

#[derive(Clone, Debug)]
pub struct AgentClient {
    pub config: AgentClientConfig,
}

#[derive(Debug)]
pub struct AgentCommandError {
    pub code: Option<AgentErrorCode>,
    pub message: String,
}

impl std::fmt::Display for AgentCommandError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for AgentCommandError {}

impl AgentClient {
    pub fn new(config: AgentClientConfig) -> Self {
        Self { config }
    }

    pub fn for_vault(vault_dir: PathBuf) -> Result<Self> {
        Ok(Self::new(AgentClientConfig::for_vault(vault_dir)?))
    }

    pub fn default_vault() -> Result<Self> {
        Ok(Self::new(AgentClientConfig::default_vault()?))
    }

    pub fn request_raw(
        &self,
        request: &AgentRequest,
    ) -> std::result::Result<AgentResponse, AgentCommandError> {
        let request_id = crate::logging::current_request_id().unwrap_or_else(uuid::Uuid::new_v4);
        let _scope = crate::logging::RequestScope::new(request_id);
        let started = Instant::now();
        let event = request.event_name();
        if !request.is_background_poll() {
            crate::logging::write_component_log(
                crate::logging::CLIENT_LOG,
                "INFO",
                &format!("event={event} outcome=sending"),
            );
        }
        let result = self.send_request(request, request_id);
        let (outcome, code) = match &result {
            Ok(response) if response.ok => ("received", None),
            Ok(response) => ("rejected", response.code.clone()),
            Err(err) => ("transport_failed", err.code.clone()),
        };
        if !request.is_background_poll() || code.is_some() {
            crate::logging::write_component_log(
                crate::logging::CLIENT_LOG,
                if code.is_some() { "WARN" } else { "INFO" },
                &format!(
                    "event={event} outcome={outcome} code={code:?} elapsed_ms={}",
                    started.elapsed().as_millis()
                ),
            );
        }
        result
    }

    fn send_request(
        &self,
        request: &AgentRequest,
        request_id: uuid::Uuid,
    ) -> std::result::Result<AgentResponse, AgentCommandError> {
        self.send_versioned_request(request, request_id, AGENT_PROTOCOL_VERSION)
    }

    // The only caller using a legacy version is the authenticated shutdown
    // below. Never retry a provider mutation against an older schema.
    fn send_versioned_request(
        &self,
        request: &AgentRequest,
        request_id: uuid::Uuid,
        protocol_version: u32,
    ) -> std::result::Result<AgentResponse, AgentCommandError> {
        let mut stream = ipc::connect(&self.config.vault_dir).map_err(|err| AgentCommandError {
            code: Some(AgentErrorCode::ServiceUnavailable),
            message: err.to_string(),
        })?;
        let auth_token =
            ipc::read_auth_token(&self.config.vault_dir).map_err(|err| AgentCommandError {
                code: Some(AgentErrorCode::ServiceUnavailable),
                message: err.to_string(),
            })?;
        // Without a deadline a wedged agent hangs the caller forever — the tray
        // polls on a timer and the desktop issues these from UI commands, so a
        // stuck read is never recovered from.
        apply_request_timeouts(&stream, request.response_timeout())?;
        let payload = AuthenticatedAgentRequest {
            protocol_version,
            auth_token,
            request_id: Some(request_id),
            request: request.clone(),
        };
        #[cfg(windows)]
        {
            // Retain the peer on this transaction. A separate connect-and-drop
            // probe can race named-pipe accept and instance recycling.
            let process = if matches!(request, AgentRequest::AgentShutdown) {
                crate::windows_process::peer_process(&stream)
                    .map_err(AgentCommandError::internal)?
            } else {
                None
            };
            ipc::framing::write_frame_with_deadline(&mut stream, &payload, REQUEST_SEND_TIMEOUT)
                .map_err(timeout_aware_error)?;
            let response: AgentResponse =
                ipc::framing::read_frame_with_deadline(&mut stream, request.response_timeout())
                    .map_err(timeout_aware_error)?;
            drop(stream);
            if response.ok {
                if let Some(process) = process {
                    crate::windows_process::wait_for_exit(&process)
                        .map_err(AgentCommandError::internal)?;
                }
            }
            Ok(response)
        }
        #[cfg(not(windows))]
        {
            write_frame(&mut stream, &payload).map_err(timeout_aware_error)?;
            read_frame(&mut stream).map_err(timeout_aware_error)
        }
    }

    pub fn request<T: DeserializeOwned>(
        &self,
        request: &AgentRequest,
    ) -> std::result::Result<T, AgentCommandError> {
        let response = self.request_raw(request)?;
        decode_response(response)
    }

    pub fn ensure_running(&self) -> Result<()> {
        self.ensure_running_with_mode(AgentStartupMode::Autostart)
    }

    pub fn ensure_running_for_app(&self) -> Result<()> {
        self.ensure_running_with_mode(AgentStartupMode::Direct {
            suppress_desktop_tray: false,
        })
    }

    pub fn ensure_running_for_desktop_companion(&self) -> Result<()> {
        self.ensure_running_with_mode(AgentStartupMode::Direct {
            suppress_desktop_tray: true,
        })
    }

    fn ensure_running_with_mode(&self, mode: AgentStartupMode) -> Result<()> {
        let _startup_guard = AGENT_START_LOCK
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let initial_response = self.request_raw(&AgentRequest::SessionStatus);
        if let Ok(response) = &initial_response {
            anyhow::ensure!(
                response.protocol_version <= AGENT_PROTOCOL_VERSION,
                "a newer AIPass agent is running; update this client instead of replacing it"
            );
        }
        let startup_status = match initial_response {
            Ok(response) => self.compatible_startup_status(response),
            Err(error) => Err(error.into()),
        };
        let initial_connection_error = match startup_status {
            Ok(status) if !status.initial_sync_pending => return Ok(()),
            Ok(_) => {
                // The agent is already up but still running its first
                // sync: wait for readiness without launching a second
                // instance underneath it.
                return self.wait_for_ready(None, &[], "agent initial sync pending", false);
            }
            Err(err) => err.to_string(),
        };
        #[cfg(target_os = "windows")]
        let (launched_binary, binary_candidates) = if mode.install_autostart() {
            let candidates = launcher::agent_binary_candidates();
            if windows_service::start_service(&self.config.vault_dir).is_ok() {
                (None, candidates)
            } else {
                // A standalone CLI install has no SCM registration. Normal user
                // operations must still start the per-user Agent without elevation.
                let launch = launcher::launch_agent(
                    &self.config.vault_dir,
                    &self.config.namespace,
                    &initial_connection_error,
                    mode.launch_options(),
                )?;
                (Some(launch.binary), launch.candidates)
            }
        } else {
            let launch = launcher::launch_agent(
                &self.config.vault_dir,
                &self.config.namespace,
                &initial_connection_error,
                mode.launch_options(),
            )?;
            (Some(launch.binary), launch.candidates)
        };
        #[cfg(not(target_os = "windows"))]
        let (launched_binary, binary_candidates) = match launcher::agent_binary_path() {
            Ok(agent_binary) => {
                let candidates = launcher::agent_binary_candidates();
                if mode.install_autostart() {
                    #[cfg(target_os = "macos")]
                    let install_result =
                        crate::autostart::ensure_autostart(&agent_binary, &self.config.vault_dir);
                    #[cfg(not(target_os = "macos"))]
                    let install_result =
                        crate::autostart::install_autostart(&agent_binary, &self.config.vault_dir);
                    match install_result {
                        Ok(_) => (Some(agent_binary), candidates),
                        Err(_) => {
                            let launch = launcher::launch_agent(
                                &self.config.vault_dir,
                                &self.config.namespace,
                                &initial_connection_error,
                                mode.launch_options(),
                            )?;
                            (Some(launch.binary), launch.candidates)
                        }
                    }
                } else {
                    let launch = launcher::launch_agent(
                        &self.config.vault_dir,
                        &self.config.namespace,
                        &initial_connection_error,
                        mode.launch_options(),
                    )?;
                    (Some(launch.binary), launch.candidates)
                }
            }
            Err(_) => {
                let launch = launcher::launch_agent(
                    &self.config.vault_dir,
                    &self.config.namespace,
                    &initial_connection_error,
                    mode.launch_options(),
                )?;
                (Some(launch.binary), launch.candidates)
            }
        };
        self.wait_for_ready(
            launched_binary,
            &binary_candidates,
            &initial_connection_error,
            mode.install_autostart(),
        )
    }

    fn compatible_startup_status(&self, response: AgentResponse) -> Result<SessionStatus> {
        if response.protocol_version >= AGENT_PROTOCOL_VERSION {
            return decode_response(response).map_err(Into::into);
        }
        // Confirm a replacement exists before retiring the per-vault resident.
        // This also handles directly launched agents that launchd does not own.
        launcher::agent_binary_path()?;
        let previous_version = response.protocol_version;
        self.shutdown_older_agent(previous_version)?;
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            match self.request_raw(&AgentRequest::SessionStatus) {
                Ok(response) if response.protocol_version == AGENT_PROTOCOL_VERSION => {
                    return decode_response(response).map_err(Into::into);
                }
                Err(_) => anyhow::bail!("older agent stopped; starting compatible agent"),
                _ if Instant::now() >= deadline => {
                    anyhow::bail!("older agent did not stop after authenticated shutdown");
                }
                _ => thread::sleep(AGENT_READY_POLL_INTERVAL),
            }
        }
    }

    pub(crate) fn shutdown_older_agent(&self, version: u32) -> Result<()> {
        anyhow::ensure!(
            version > 0 && version < AGENT_PROTOCOL_VERSION,
            "only older agents can be replaced"
        );
        crate::logging::write_component_log(
            crate::logging::CLIENT_LOG, "INFO",
            &format!("event=agent.compatibility outcome=replacing protocol={version} required={AGENT_PROTOCOL_VERSION}"),
        );
        let response = self.send_versioned_request(
            &AgentRequest::AgentShutdown,
            uuid::Uuid::new_v4(),
            version,
        )?;
        anyhow::ensure!(
            response.ok && response.protocol_version == version,
            "older agent rejected shutdown"
        );
        Ok(())
    }

    fn wait_for_ready(
        &self,
        launched_binary: Option<PathBuf>,
        binary_candidates: &[PathBuf],
        initial_connection_error: &str,
        repair_autostart: bool,
    ) -> Result<()> {
        #[cfg(not(target_os = "macos"))]
        let _ = repair_autostart;
        let started = Instant::now();
        let mut deadline = ready_deadline(started, false);
        #[cfg(target_os = "macos")]
        let mut force_repair_at =
            repair_autostart.then(|| Instant::now() + AUTOSTART_RECOVERY_GRACE);
        let last_connection_error = loop {
            match self.request::<SessionStatus>(&AgentRequest::SessionStatus) {
                Ok(status) if status.initial_sync_pending => {
                    // A pending first sync is not readiness, but it does earn
                    // a bounded extension over the plain boot timeout.
                    deadline = ready_deadline(started, true);
                    if Instant::now() >= deadline {
                        break "agent initial sync did not finish in time".to_string();
                    }
                }
                Ok(_) => return Ok(()),
                Err(err) => {
                    let message = err.to_string();
                    #[cfg(target_os = "macos")]
                    if force_repair_at.is_some_and(|repair_at| Instant::now() >= repair_at) {
                        force_repair_at = None;
                        if let Some(agent_binary) = launched_binary.as_deref() {
                            let _ = crate::autostart::install_autostart(
                                agent_binary,
                                &self.config.vault_dir,
                            );
                        }
                    }
                    if Instant::now() >= deadline {
                        break message;
                    }
                }
            }
            thread::sleep(AGENT_READY_POLL_INTERVAL);
        };
        Err(anyhow::anyhow!(launcher::agent_ready_timeout_message(
            &self.config.vault_dir,
            &self.config.namespace,
            launched_binary.as_deref(),
            binary_candidates,
            initial_connection_error,
            Some(&last_connection_error),
        )))
    }

    pub fn shutdown(&self) -> Result<()> {
        self.request::<serde_json::Value>(&AgentRequest::AgentShutdown)
            .map(|_| ())
            .map_err(anyhow::Error::from)
    }
}

#[derive(Clone, Copy, Debug)]
enum AgentStartupMode {
    Autostart,
    Direct { suppress_desktop_tray: bool },
}

impl AgentStartupMode {
    fn install_autostart(self) -> bool {
        matches!(self, Self::Autostart)
    }

    fn launch_options(self) -> launcher::AgentLaunchOptions {
        launcher::AgentLaunchOptions {
            suppress_desktop_tray: matches!(
                self,
                Self::Autostart
                    | Self::Direct {
                        suppress_desktop_tray: true
                    }
            ),
        }
    }
}

/// How long to keep waiting for agent readiness based on the latest status
/// poll: a pending first sync extends the base boot deadline by a bounded
/// grace window; once the sync settles (or never started) the base timeout
/// applies again.
fn ready_deadline(started: Instant, initial_sync_pending: bool) -> Instant {
    let base = started + AGENT_READY_TIMEOUT;
    if initial_sync_pending {
        base + INITIAL_SYNC_READY_GRACE
    } else {
        base
    }
}

impl AgentCommandError {
    fn internal(err: impl Into<anyhow::Error>) -> Self {
        Self {
            code: Some(AgentErrorCode::Internal),
            message: err.into().to_string(),
        }
    }

    pub fn is_locked(&self) -> bool {
        matches!(self.code, Some(AgentErrorCode::Locked))
    }
}
fn decode_response<T: DeserializeOwned>(
    response: AgentResponse,
) -> std::result::Result<T, AgentCommandError> {
    if response.protocol_version != AGENT_PROTOCOL_VERSION {
        return Err(AgentCommandError {
            code: Some(AgentErrorCode::ServiceUnavailable),
            message: format!(
                "agent protocol mismatch: running {}, required {}; restart with the matching AIPass agent",
                response.protocol_version, AGENT_PROTOCOL_VERSION
            ),
        });
    }
    if !response.ok {
        return Err(AgentCommandError {
            code: response.code,
            message: response
                .message
                .unwrap_or_else(|| "agent request failed".to_string()),
        });
    }
    serde_json::from_value(response.data).map_err(AgentCommandError::internal)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn windows_shutdown_uses_one_authenticated_connection() {
        use interprocess::local_socket::traits::Listener as _;
        let root = tempfile::tempdir().unwrap();
        let client = AgentClient::for_vault(root.path().join("vault")).unwrap();
        let token = ipc::load_or_create_auth_token(&client.config.vault_dir).unwrap();
        let listener = ipc::listen(&client.config.vault_dir).unwrap();
        let server = thread::spawn(move || {
            let mut stream = listener.accept().unwrap();
            let payload: AuthenticatedAgentRequest =
                ipc::framing::read_frame_with_deadline(&mut stream, Duration::from_secs(5))
                    .unwrap();
            assert_eq!(payload.auth_token.expose(), token.expose());
            assert!(matches!(payload.request, AgentRequest::AgentShutdown));
            ipc::framing::write_frame_with_deadline(
                &mut stream,
                &AgentResponse::empty(),
                Duration::from_secs(5),
            )
            .unwrap();
            thread::sleep(Duration::from_millis(100));
        });
        client.shutdown().unwrap();
        server.join().unwrap();
        ipc::clear_auth_token(&client.config.vault_dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_named_pipe_client_authenticates_large_responses() {
        use interprocess::local_socket::traits::Listener as _;
        let dir = tempfile::tempdir().unwrap();
        let client = AgentClient::for_vault(dir.path().join("vault")).unwrap();
        let token = ipc::load_or_create_auth_token(&client.config.vault_dir).unwrap();
        let listener = ipc::listen(&client.config.vault_dir).unwrap();
        let server = thread::spawn(move || {
            let mut stream = listener.accept().unwrap();
            let request: AuthenticatedAgentRequest =
                ipc::framing::read_frame_with_deadline(&mut stream, Duration::from_secs(5))
                    .unwrap();
            assert_eq!(request.auth_token.expose(), token.expose());
            assert!(matches!(request.request, AgentRequest::SessionStatus));
            ipc::framing::write_frame_with_deadline(
                &mut stream,
                &AgentResponse::success(
                    serde_json::json!({"marker":"windows", "large": "x".repeat(512 * 1024)}),
                ),
                Duration::from_secs(5),
            )
            .unwrap();
            // Keep the server alive until the client reads the response.
            std::thread::sleep(Duration::from_millis(100));
        });
        let response = client
            .request::<serde_json::Value>(&AgentRequest::SessionStatus)
            .unwrap();
        assert_eq!(response["marker"], "windows");
        assert_eq!(response["large"].as_str().unwrap().len(), 512 * 1024);
        server.join().unwrap();
        ipc::clear_auth_token(&client.config.vault_dir).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn windows_silent_named_pipe_times_out() {
        use interprocess::local_socket::traits::{Listener as _, Stream as _};
        // Interprocess closes written pipe handles on a background flush pool.
        // Use a fresh namespace so this timeout check does not race that cleanup.
        let dir = tempfile::tempdir().unwrap();
        let vault_dir = dir.path().join("vault");
        let listener = ipc::listen(&vault_dir).unwrap();
        let mut stream = ipc::connect(&vault_dir).unwrap();
        let _held = listener.accept().unwrap();
        stream.set_nonblocking(true).unwrap();
        let error = ipc::framing::read_frame_with_deadline::<AgentResponse>(
            &mut stream,
            Duration::from_millis(50),
        )
        .unwrap_err();
        assert_eq!(
            timeout_aware_error(error).code,
            Some(AgentErrorCode::ServiceUnavailable)
        );
    }

    /// A socket that accepts a connection and then never answers is exactly
    /// the wedged-agent case: without a receive timeout the caller blocks
    /// forever, which used to freeze the tray and every desktop command.
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_silent_agent_times_out_instead_of_hanging() {
        use interprocess::local_socket::{prelude::*, GenericFilePath, ListenerOptions};

        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("silent.sock");
        let name = path
            .clone()
            .to_fs_name::<GenericFilePath>()
            .expect("socket name");
        let listener = ListenerOptions::new()
            .name(name)
            .create_sync()
            .expect("listener");
        // Accept and hold the connection open without ever replying.
        let accepted = thread::spawn(move || {
            let _held = listener.accept();
            thread::sleep(Duration::from_secs(30));
        });

        let connect_name = path.to_fs_name::<GenericFilePath>().expect("connect name");
        let stream = interprocess::local_socket::Stream::connect(connect_name).expect("connect");
        apply_request_timeouts(&stream, Duration::from_millis(250)).expect("set timeouts");

        let started = Instant::now();
        let result = read_frame::<AgentResponse>(&stream);
        let elapsed = started.elapsed();

        assert!(result.is_err(), "expected the silent agent to time out");
        assert!(elapsed < Duration::from_secs(5), "elapsed {elapsed:?}");
        // The caller sees an unavailable agent, not an internal fault, so it
        // falls back to its "agent not reachable" handling.
        let err = timeout_aware_error(result.unwrap_err());
        assert_eq!(err.code, Some(AgentErrorCode::ServiceUnavailable));

        drop(stream);
        drop(accepted);
    }

    #[test]
    fn resident_agent_startup_suppresses_tray_launch() {
        let companion = AgentStartupMode::Direct {
            suppress_desktop_tray: true,
        }
        .launch_options();
        let app = AgentStartupMode::Direct {
            suppress_desktop_tray: false,
        }
        .launch_options();
        let autostart = AgentStartupMode::Autostart.launch_options();

        assert!(companion.suppress_desktop_tray);
        assert!(!app.suppress_desktop_tray);
        assert!(autostart.suppress_desktop_tray);
    }

    #[test]
    fn successful_legacy_response_is_not_readiness() {
        let mut response = AgentResponse::success(serde_json::json!({
            "exists": true, "locked": false,
            "policy": {"idleLockMinutes": 60, "lockOnSleep": true, "lockOnScreenLock": true}
        }));
        response.protocol_version = AGENT_PROTOCOL_VERSION - 1;
        let error = decode_response::<SessionStatus>(response).unwrap_err();
        assert_eq!(error.code, Some(AgentErrorCode::ServiceUnavailable));
        assert!(error.message.contains("protocol mismatch"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn retiring_an_older_agent_uses_only_authenticated_legacy_shutdown() {
        use interprocess::local_socket::traits::Listener as _;
        let dir = tempfile::tempdir().unwrap();
        let client = AgentClient::for_vault(dir.path().join("vault")).unwrap();
        let token = ipc::load_or_create_auth_token(&client.config.vault_dir).unwrap();
        let listener = ipc::listen(&client.config.vault_dir).unwrap();
        let version = AGENT_PROTOCOL_VERSION - 1;
        let server = thread::spawn(move || {
            let mut stream = listener.accept().unwrap();
            let request: AuthenticatedAgentRequest = read_frame(&mut stream).unwrap();
            assert_eq!(request.protocol_version, version);
            assert!(request.auth_token == token);
            assert!(matches!(request.request, AgentRequest::AgentShutdown));
            let mut response = AgentResponse::empty();
            response.protocol_version = version;
            write_frame(&mut stream, &response).unwrap();
        });
        crate::logging::with_test_log_dir(&dir.path().join("logs"), || {
            client.shutdown_older_agent(version).unwrap();
        });
        server.join().unwrap();
        ipc::clear_auth_token(&client.config.vault_dir).unwrap();
        assert!(client.shutdown_older_agent(AGENT_PROTOCOL_VERSION).is_err());
        assert!(client
            .shutdown_older_agent(AGENT_PROTOCOL_VERSION + 1)
            .is_err());
    }

    #[test]
    fn failed_status_response_is_not_treated_as_ready() {
        let response = AgentResponse::error(
            AgentErrorCode::ValidationFailed,
            "unsupported agent protocol version",
        );

        let error = decode_response::<SessionStatus>(response).unwrap_err();

        assert_eq!(error.code, Some(AgentErrorCode::ValidationFailed));
    }

    #[test]
    fn initial_sync_pending_extends_the_ready_deadline_within_a_cap() {
        let started = Instant::now();

        assert_eq!(
            ready_deadline(started, false),
            started + AGENT_READY_TIMEOUT
        );
        assert_eq!(
            ready_deadline(started, true),
            started + AGENT_READY_TIMEOUT + INITIAL_SYNC_READY_GRACE
        );
        // The grace window stays bounded: pending sync can never push
        // readiness beyond 15s + 120s from the wait start.
        assert!(INITIAL_SYNC_READY_GRACE <= Duration::from_secs(120));
    }

    #[test]
    fn legacy_status_without_initial_sync_field_decodes_as_ready() {
        let status: SessionStatus = serde_json::from_value(serde_json::json!({
            "exists": true,
            "locked": false,
            "policy": {
                "idleLockMinutes": 60,
                "lockOnSleep": true,
                "lockOnScreenLock": true
            }
        }))
        .expect("decode legacy status");

        assert!(!status.initial_sync_pending);
    }
}

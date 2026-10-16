//! Process-local Cursor Runs. No tool result is uploaded twice or reconstructed
//! after an ambiguous submission. The Agent owns every retained connection.
use super::*;
use aipass_agent_protocol::SensitiveString;
use aipass_proxy::ResolvedTarget;
use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, Mutex, OnceLock},
    time::Instant,
};
mod connection;

type Id = uuid::Uuid;
#[derive(Default)]
struct Pool {
    entries: HashMap<Id, Arc<Entry>>,
    calls: HashMap<String, (Id, String, Instant)>,
}
struct Entry {
    owner: Id,
    binding: String,
    session: String,
    context: [u8; 32],
    outbound: Option<UpstreamProxyConfig>,
    commands: mpsc::Sender<Turn>,
    abort: watch::Sender<bool>,
    state: Mutex<State>,
}
struct State {
    busy: bool,
    closed: bool,
    parked: bool,
    history: [u8; 32],
    history_len: usize,
    text: String,
    pending: Vec<codec::PendingCall>,
    touched: Instant,
}
struct Turn {
    body: Value,
    output: mpsc::Sender<Result<Event>>,
}
enum Event {
    Data(Vec<u8>),
    End,
    Rejected(u16, String),
}
struct Lease {
    entry: Arc<Entry>,
    armed: bool,
}
impl Drop for Lease {
    fn drop(&mut self) {
        if self.armed {
            self.entry.abort.send_replace(true);
        }
    }
}
fn pool() -> &'static Mutex<Pool> {
    static POOL: OnceLock<Mutex<Pool>> = OnceLock::new();
    POOL.get_or_init(Mutex::default)
}
impl Pool {
    fn retain_targets(&mut self, targets: &[&ResolvedTarget]) {
        self.entries.retain(|_, entry| {
            let keep = targets.iter().any(|target| {
                target.config.id == entry.owner
                    && target.upstream_kind == aipass_proxy::UpstreamKind::CommunitySubscription
                    && format!("{:x}", Sha256::digest(target.api_key.as_bytes())) == entry.binding
                    && target.upstream_proxy == entry.outbound
            });
            if !keep {
                entry.abort.send_replace(true);
            }
            keep
        });
        // Call bindings remain tombstones: missing Runs must reject old result
        // uploads, rather than reconstructing them under a different transport.
    }
}
pub(crate) fn retain_targets(targets: &[&ResolvedTarget]) {
    if let Ok(mut pool) = pool().lock() {
        pool.retain_targets(targets);
    }
}
fn digest(value: &Value) -> [u8; 32] {
    Sha256::digest(value.to_string().as_bytes()).into()
}
fn context(body: &Value, model: &str) -> [u8; 32] {
    let mut value = body.clone();
    if let Some(object) = value.as_object_mut() {
        for field in ["messages", "stream", "metadata"] {
            object.remove(field);
        }
    }
    value["model"] = json!(model);
    digest(&value)
}
pub(crate) fn revoke_owners(owners: &HashSet<Id>) {
    if let Ok(mut pool) = pool().lock() {
        pool.entries.retain(|_, entry| {
            if owners.contains(&entry.owner) {
                entry.abort.send_replace(true);
                false
            } else {
                true
            }
        });
        pool.calls
            .retain(|_, (owner, _, _)| !owners.contains(owner));
    }
}
pub(crate) fn history_owner(body: &Value, targets: &[ResolvedTarget]) -> Result<Option<Id>> {
    let pool = pool()
        .lock()
        .map_err(|_| "Cursor session coordinator unavailable")?;
    let mut owner = None;
    for id in aipass_proxy::subscription_history_call_ids(body)
        .into_iter()
        .filter(|id| id.starts_with("call_aipass_cursor_"))
    {
        let Some((id, binding, when)) = pool.calls.get(id) else {
            return Err("Cursor session history expired; start a new conversation".into());
        };
        if when.elapsed() > Duration::from_secs(86400)
            || !targets.iter().any(|target| {
                target.config.id == *id
                    && format!("{:x}", Sha256::digest(target.api_key.as_bytes())) == *binding
            })
        {
            return Err("Cursor session account changed; start a new conversation".into());
        }
        if owner.is_some_and(|old| old != *id) {
            return Err("Cursor history mixes accounts".into());
        }
        owner = Some(*id);
    }
    Ok(owner)
}
fn validate(state: &State, body: &Value) -> Result<()> {
    let messages = body["messages"]
        .as_array()
        .ok_or("Cursor messages must be an array")?;
    if messages.len() != state.history_len + 1 + state.pending.len()
        || digest(&json!(&messages[..state.history_len])) != state.history
    {
        return Err("Cursor native history changed; restart the conversation".into());
    }
    let assistant = &messages[state.history_len];
    let calls = assistant["tool_calls"]
        .as_array()
        .ok_or("Cursor pending assistant calls missing")?;
    if assistant["role"] != "assistant"
        || aipass_proxy_conversion::providers::connect::text(&assistant["content"]) != state.text
        || calls.len() != state.pending.len()
    {
        return Err("Cursor pending assistant turn changed".into());
    }
    let mut seen = HashSet::new();
    for call in calls {
        let id = call["id"].as_str().ok_or("Cursor call id missing")?;
        let expected = state
            .pending
            .iter()
            .find(|c| c.id == id)
            .ok_or("unknown Cursor call")?;
        let args: Value = serde_json::from_str(
            call["function"]["arguments"]
                .as_str()
                .ok_or("Cursor call arguments missing")?,
        )
        .map_err(|_| "invalid Cursor call arguments")?;
        if !seen.insert(id)
            || call["type"] != "function"
            || call["function"]["name"] != expected.name
            || args != expected.arguments
        {
            return Err("Cursor pending call changed".into());
        }
    }
    let results = &messages[state.history_len + 1..];
    let mut seen = HashSet::new();
    for result in results {
        let id = result["tool_call_id"]
            .as_str()
            .ok_or("Cursor result id missing")?;
        if result["role"] != "tool" || !seen.insert(id) || !state.pending.iter().any(|c| c.id == id)
        {
            return Err("return exactly one result per pending Cursor call".into());
        }
        if !(result["content"].is_string()
            || result["content"].as_array().is_some_and(|blocks| {
                blocks
                    .iter()
                    .all(|b| b["type"] == "text" && b["text"].is_string())
            }))
        {
            return Err("Cursor native continuation requires text tool results".into());
        }
    }
    Ok(())
}
fn entry(c: &Context, model: &str, body: &Value) -> Result<Arc<Entry>> {
    let owner = c.history_owner.ok_or("Cursor account binding missing")?;
    if c.session_binding.is_empty() {
        return Err("Cursor credential binding missing".into());
    }
    let signature = context(body, model);
    let mut pool = pool()
        .lock()
        .map_err(|_| "Cursor session coordinator unavailable")?;
    pool.calls
        .retain(|_, (_, _, when)| when.elapsed() < Duration::from_secs(86400));
    if pool.calls.len() > 4096 {
        return Err("Cursor session history capacity reached; restart the proxy".into());
    }
    let result_id = body["messages"]
        .as_array()
        .and_then(|messages| messages.last())
        .filter(|m| m["role"] == "tool")
        .and_then(|m| m["tool_call_id"].as_str());
    let existing = pool
        .entries
        .values()
        .find(|entry| {
            entry.owner == owner
                && result_id.map_or_else(
                    || entry.session == c.session,
                    |id| {
                        entry
                            .state
                            .lock()
                            .is_ok_and(|state| state.pending.iter().any(|call| call.id == id))
                    },
                )
        })
        .cloned();
    if let Some(entry) = existing {
        let mut state = entry
            .state
            .lock()
            .map_err(|_| "Cursor session unavailable")?;
        if !state.closed {
            if state.busy {
                return Err("Cursor conversation already has an active turn".into());
            }
            if !state.parked
                || entry.binding != c.session_binding
                || entry.context != signature
                || entry.outbound != c.outbound
            {
                return Err("Cursor account or context changed; restart the conversation".into());
            }
            validate(&state, body)?;
            state.busy = true;
            drop(state);
            return Ok(entry);
        }
    }
    if body["messages"]
        .as_array()
        .and_then(|m| m.last())
        .is_some_and(|m| {
            m["role"] == "tool"
                && m["tool_call_id"]
                    .as_str()
                    .is_some_and(|id| id.starts_with("call_aipass_cursor_"))
        })
    {
        return Err("Cursor native session expired; restart the conversation".into());
    }
    pool.entries
        .retain(|_, entry| entry.state.lock().is_ok_and(|s| !s.closed));
    if pool.entries.len() >= 32 || pool.entries.values().filter(|e| e.owner == owner).count() >= 8 {
        return Err("Cursor native session capacity reached".into());
    }
    let messages = body["messages"]
        .as_array()
        .ok_or("Cursor messages missing")?;
    let (commands, receiver) = mpsc::channel(1);
    let (abort, cancellation) = watch::channel(false);
    let entry = Arc::new(Entry {
        owner,
        binding: c.session_binding.clone(),
        session: c.session.clone(),
        context: signature,
        outbound: c.outbound.clone(),
        commands,
        abort,
        state: Mutex::new(State {
            busy: true,
            closed: false,
            parked: false,
            history: digest(&json!(messages)),
            history_len: messages.len(),
            text: String::new(),
            pending: vec![],
            touched: Instant::now(),
        }),
    });
    let token = SensitiveString::new(c.token()?);
    let outbound = c.outbound.clone();
    let worker = entry.clone();
    let model = model.to_owned();
    std::thread::Builder::new()
        .name("aipass-cursor-run".into())
        .spawn(move || {
            connection::start(worker, receiver, cancellation, token, outbound, None, model)
        })
        .map_err(|_| "Cursor executor unavailable")?;
    pool.entries.insert(Id::new_v4(), entry.clone());
    Ok(entry)
}
pub(super) async fn generate(c: &mut Context, m: &Value, body: Value) -> Result<()> {
    let raw = m["cursor_models"].as_array().cloned().unwrap_or_default();
    let model = codec::model_id(
        &raw,
        s(&body, "model"),
        s(&body, "reasoning_effort"),
        matches!(s(&body, "service_tier"), "priority" | "fast"),
    );
    let entry = match entry(c, &model, &body) {
        Ok(entry) => entry,
        Err(message) => {
            c.headers(
                409,
                json!([
                    ["content-type", "application/json"],
                    ["x-aipass-local-rejection", "1"]
                ]),
            )
            .await?;
            c.data(
                json!({"error":{"type":"invalid_request_error","message":message}})
                    .to_string()
                    .as_bytes(),
            )
            .await?;
            return c.emit(json!({"type":"end"})).await;
        }
    };
    let mut lease = Lease {
        entry: entry.clone(),
        armed: true,
    };
    let (output, mut events) = mpsc::channel(8);
    entry
        .commands
        .send(Turn { body, output })
        .await
        .map_err(|_| "Cursor session closed")?;
    let mut sent = false;
    while let Some(event) = events.recv().await {
        match event? {
            Event::Data(chunk) => {
                if !sent {
                    c.headers(
                        200,
                        json!([
                            ["content-type", "text/event-stream"],
                            ["x-aipass-usage-scope", "native-run"]
                        ]),
                    )
                    .await?;
                    sent = true;
                }
                c.data(&chunk).await?;
            }
            Event::Rejected(status, message) if !sent => return error(c, status, &message).await,
            Event::Rejected(_, _) => {
                return Err("Cursor stream failed after generation started".into())
            }
            Event::End => {
                c.emit(json!({"type":"end"})).await?;
                lease.armed = false;
                return Ok(());
            }
        }
    }
    Err("Cursor native connection ended before completion".into())
}

#[cfg(test)]
mod tests;

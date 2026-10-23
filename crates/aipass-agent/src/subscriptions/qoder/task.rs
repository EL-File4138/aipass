//! Bounded, credential-free task bookkeeping for one user prompt's tool loop.
use super::*;
use std::{
    collections::BTreeMap,
    sync::{Mutex, OnceLock},
};
const CAPACITY: usize = 512;
const IDLE_MS: u64 = 2 * 60 * 60 * 1000;
type Key = [u8; 32];

#[derive(Clone)]
pub(super) struct Task {
    key: Key,
    id: String,
    session: String,
    begin: u64,
    used: u64,
    answered: bool,
}
#[derive(Default)]
struct Tasks(BTreeMap<Key, Task>);
fn session_seed(session: &str, chat: &Value) -> Value {
    if !session.is_empty() {
        return json!(["named", session]);
    }
    for field in ["prompt_cache_key", "user"] {
        if let Some(value) = chat[field].as_str().filter(|s| !s.trim().is_empty()) {
            return json!([field, value]);
        }
    }
    let first = chat["messages"]
        .as_array()
        .and_then(|messages| messages.iter().find(|m| m["role"] == "user"));
    match first {
        Some(first) => json!(["opening", &first["content"]]),
        // Without any conversation anchor, do not merge unrelated tool-only calls.
        None => json!(["unbound", uuid::Uuid::new_v4().to_string()]),
    }
}
impl Tasks {
    fn get(&mut self, binding: &str, session: &str, chat: &Value, at: u64) -> Task {
        self.0
            .retain(|_, task| at.saturating_sub(task.used) <= IDLE_MS);
        let session_hash = Sha256::digest(
            serde_json::to_vec(&("aipass:qoder:session", binding, session_seed(session, chat)))
                .unwrap(),
        );
        let session = uuid::Uuid::from_bytes(session_hash[..16].try_into().unwrap()).to_string();
        let last = chat["messages"].as_array().and_then(|messages| {
            messages
                .iter()
                .enumerate()
                .rev()
                .find(|(_, m)| m["role"] == "user")
        });
        let key: Key = Sha256::digest(
            serde_json::to_vec(&(session.as_str(), last.map(|(i, m)| (i, &m["content"])))).unwrap(),
        )
        .into();
        let task = self.0.entry(key).or_insert_with(|| Task {
            key,
            id: uuid::Uuid::new_v4().to_string(),
            session,
            begin: at,
            used: at,
            answered: false,
        });
        task.used = at;
        let task = task.clone();
        while self.0.len() > CAPACITY {
            let oldest = *self
                .0
                .iter()
                .filter(|(k, _)| **k != key)
                .min_by_key(|(_, t)| t.used)
                .unwrap()
                .0;
            self.0.remove(&oldest);
        }
        task
    }
    fn answered(&mut self, task: &Task) {
        if let Some(current) = self.0.get_mut(&task.key).filter(|t| t.id == task.id) {
            current.answered = true;
        }
    }
}
fn tasks() -> &'static Mutex<Tasks> {
    static TASKS: OnceLock<Mutex<Tasks>> = OnceLock::new();
    TASKS.get_or_init(Default::default)
}
impl Task {
    pub(super) fn for_request(c: &Context, chat: &Value) -> Result<Self> {
        let binding = format!("{}:{}", c.provider, c.session_binding);
        Ok(tasks()
            .lock()
            .map_err(|_| "Qoder task state unavailable")?
            .get(&binding, &c.session, chat, now()))
    }
    pub(super) fn request(&self, chat: &Value, config: &Value) -> Result<Value> {
        let mut body = aipass_proxy_conversion::providers::qoder::request(
            chat,
            config,
            &self.session,
            self.begin,
        )?;
        body["request_set_id"] = json!(self.id);
        body["business"]["id"] = json!(self.id);
        body["business"]["stage"] = json!(if self.answered { "processing" } else { "start" });
        Ok(body)
    }
    pub(super) fn answered(&self) {
        if let Ok(mut tasks) = tasks().lock() {
            tasks.answered(self);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_tool_loop_keeps_its_task_but_each_request_gets_its_own_id() {
        let mut tasks = Tasks::default();
        let chat = json!({"messages":[{"role":"system","content":"one"},{"role":"user","content":"work"}]});
        let first = tasks.get("account-a", "session-a", &chat, 100);
        let a = first.request(&chat, &json!({})).unwrap();
        let retried = tasks
            .get("account-a", "session-a", &chat, 200)
            .request(&chat, &json!({}))
            .unwrap();
        assert_eq!(a["request_set_id"], retried["request_set_id"]);
        assert_eq!(retried["business"]["stage"], "start");
        tasks.answered(&first);
        let mut follow = chat.clone();
        follow["messages"][0]["content"] = json!("changed instructions");
        follow["messages"].as_array_mut().unwrap().extend([json!({"role":"assistant","tool_calls":[{"id":"call","type":"function","function":{"name":"read","arguments":"{}"}}]}),json!({"role":"tool","tool_call_id":"call","content":"result"})]);
        let b = tasks
            .get("account-a", "session-a", &follow, 300)
            .request(&follow, &json!({}))
            .unwrap();
        for key in ["session_id", "request_set_id"] {
            assert_eq!(a[key], b[key]);
        }
        assert_eq!(a["business"]["begin_at"], b["business"]["begin_at"]);
        assert_eq!(b["business"]["stage"], "processing");
        assert_ne!(a["request_id"], b["request_id"]);
        assert_eq!(b["request_id"], b["chat_record_id"]);
        for (binding, session) in [("account-b", "session-a"), ("account-a", "session-b")] {
            assert_ne!(first.id, tasks.get(binding, session, &follow, 400).id);
        }
        follow["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"user","content":"work"}));
        assert_ne!(
            first.id,
            tasks.get("account-a", "session-a", &follow, 500).id
        );
    }
    #[test]
    fn task_cache_expires_and_late_answers_cannot_mark_replacements() {
        let mut tasks = Tasks::default();
        let chat = json!({"messages":[{"role":"user","content":"work"}]});
        let old = tasks.get("a", "s", &chat, 0);
        let new = tasks.get("a", "s", &chat, IDLE_MS + 1);
        assert_ne!(old.id, new.id);
        tasks.answered(&old);
        assert!(!tasks.get("a", "s", &chat, IDLE_MS + 2).answered);
        for i in 0..CAPACITY + 1 {
            tasks.get("a", &i.to_string(), &chat, IDLE_MS + 3 + i as u64);
        }
        assert_eq!(tasks.0.len(), CAPACITY);
        assert!(!tasks.0.contains_key(&old.key));
    }
    #[test]
    fn anonymous_fallback_uses_the_opening_and_named_sessions_take_precedence() {
        let mut tasks = Tasks::default();
        let mut chat = json!({"messages":[{"role":"user","content":"first"}]});
        let initial = tasks.get("a", "", &chat, 1);
        chat["messages"]
            .as_array_mut()
            .unwrap()
            .push(json!({"role":"assistant","content":"answer"}));
        assert_eq!(initial.id, tasks.get("a", "", &chat, 2).id);
        chat["messages"][0]["content"] = json!("another opening");
        assert_ne!(initial.session, tasks.get("a", "", &chat, 3).session);
        let named = tasks.get("a", "stable-native-session", &chat, 4);
        chat["prompt_cache_key"] = json!("changed");
        assert_eq!(
            named.session,
            tasks.get("a", "stable-native-session", &chat, 5).session
        );
        let unbound =
            json!({"messages":[{"role":"tool","tool_call_id":"call","content":"result"}]});
        assert_ne!(
            tasks.get("a", "", &unbound, 6).session,
            tasks.get("a", "", &unbound, 7).session
        );
    }
}

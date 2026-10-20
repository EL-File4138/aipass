//! Bounded, memory-only history for search rounds hidden from clients.
use super::*;
use zeroize::Zeroizing;
const LIMIT: usize = 8 * 1024 * 1024;
struct Entry {
    context: [u8; 32],
    route: Uuid,
    target: Uuid,
    public: Zeroizing<Vec<u8>>,
    private: Zeroizing<Vec<u8>>,
    at: Instant,
}
#[derive(Default)]
pub(crate) struct History {
    entries: VecDeque<Entry>,
    bytes: usize,
}
fn field(protocol: ProxyProtocol) -> &'static str {
    if protocol == ProxyProtocol::OpenAiResponses {
        "input"
    } else {
        "messages"
    }
}
fn context(body: &Value) -> [u8; 32] {
    use sha2::{Digest, Sha256};
    Sha256::digest(
        json!([
            body["model"],
            body["system"],
            body["instructions"],
            body["reasoning"],
            body["thinking"],
            body["output_config"]
        ])
        .to_string()
        .as_bytes(),
    )
    .into()
}
fn messages(protocol: ProxyProtocol, body: &Value) -> Option<Vec<Value>> {
    let value = &body[field(protocol)];
    if let Some(s) = value.as_str() {
        Some(vec![json!({"role":"user","content":s})])
    } else {
        value.as_array().cloned()
    }
}
fn append_reply(protocol: ProxyProtocol, messages: &mut Vec<Value>, response: &Value) {
    match protocol {
        ProxyProtocol::AnthropicMessages => {
            messages.push(json!({"role":"assistant","content":response["content"]}))
        }
        ProxyProtocol::OpenAiChatCompletions => {
            messages.push(response["choices"][0]["message"].clone())
        }
        ProxyProtocol::OpenAiResponses => {
            messages.extend(response["output"].as_array().into_iter().flatten().cloned())
        }
    }
}
impl History {
    pub(crate) fn retain(&mut self, valid: &HashSet<(Uuid, Uuid)>) {
        self.entries
            .retain(|e| valid.contains(&(e.route, e.target)));
        self.recount();
    }
    fn recount(&mut self) {
        self.bytes = self
            .entries
            .iter()
            .map(|e| e.public.len() + e.private.len())
            .sum();
    }
    pub(super) fn restore(
        &mut self,
        route: Uuid,
        protocol: ProxyProtocol,
        original: &Value,
        body: &mut Value,
    ) -> Option<Uuid> {
        self.entries
            .retain(|e| e.at.elapsed() < Duration::from_secs(3600));
        self.recount();
        let incoming = messages(protocol, body)?;
        for e in self
            .entries
            .iter()
            .rev()
            .filter(|e| e.route == route && e.context == context(original))
        {
            let public: Vec<Value> = serde_json::from_slice(&e.public).ok()?;
            if incoming.starts_with(&public) {
                let mut private: Vec<Value> = serde_json::from_slice(&e.private).ok()?;
                private.extend_from_slice(&incoming[public.len()..]);
                body[field(protocol)] = json!(private);
                return Some(e.target);
            }
        }
        None
    }
    pub(super) fn remember(
        &mut self,
        route: Uuid,
        target: Uuid,
        protocol: ProxyProtocol,
        original: &Value,
        private: &Value,
        response: &Value,
    ) {
        let (Some(mut public), Some(mut private)) =
            (messages(protocol, original), messages(protocol, private))
        else {
            return;
        };
        append_reply(protocol, &mut public, response);
        append_reply(protocol, &mut private, response);
        let (Ok(public), Ok(private)) = (serde_json::to_vec(&public), serde_json::to_vec(&private))
        else {
            return;
        };
        let size = public.len() + private.len();
        if size > LIMIT {
            return;
        }
        while self.bytes + size > LIMIT || self.entries.len() >= 64 {
            if let Some(e) = self.entries.pop_front() {
                self.bytes -= e.public.len() + e.private.len();
            } else {
                break;
            }
        }
        self.bytes += size;
        self.entries.push_back(Entry {
            context: context(original),
            route,
            target,
            public: Zeroizing::new(public),
            private: Zeroizing::new(private),
            at: Instant::now(),
        });
    }
}

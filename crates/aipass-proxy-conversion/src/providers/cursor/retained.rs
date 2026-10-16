use super::*;

#[derive(Clone, Debug)]
pub struct PendingCall {
    pub id: String,
    pub name: String,
    pub arguments: Value,
    pub(super) request_id: u64,
    pub(super) exec: String,
}
impl CursorStream {
    pub fn new_retained(model: &str, id: &str, run: Run) -> Self {
        Self {
            retained: true,
            ..Self::new(model, id, run)
        }
    }
    pub fn parked(&self) -> bool {
        self.parked
    }
    pub fn pending(&self) -> &[PendingCall] {
        &self.pending
    }
    /// Validate the complete batch before constructing a single upload frame.
    /// The caller still controls execution; only its results are serialized.
    pub fn resume(&mut self, results: &[Value], id: &str) -> Result<Vec<Vec<u8>>, String> {
        if !self.parked || self.ended {
            return Err("Cursor session is not waiting for tool results".into());
        }
        if results.len() != self.pending.len() {
            return Err("return exactly one result per Cursor call".into());
        }
        let mut seen = BTreeSet::new();
        let mut frames = Vec::new();
        for result in results {
            let id = result["tool_call_id"]
                .as_str()
                .ok_or("Cursor result id missing")?;
            if result["role"] != "tool" || !seen.insert(id) {
                return Err("invalid or duplicate Cursor tool result".into());
            }
            let call = self
                .pending
                .iter()
                .find(|call| call.id == id)
                .ok_or("unknown Cursor tool result")?;
            let content = match &result["content"] {
                Value::String(text) => vec![json!({"type":"text","text":text})],
                Value::Array(blocks)
                    if blocks
                        .iter()
                        .all(|block| block["type"] == "text" && block["text"].is_string()) =>
                {
                    blocks.clone()
                }
                _ => return Err("Cursor native continuation requires text tool results".into()),
            };
            let mut encoded = Vec::new();
            for block in content {
                encoded.extend(bytes(
                    1,
                    &bytes(1, &string(1, block["text"].as_str().unwrap())),
                ));
            }
            if result["is_error"] == true {
                encoded.extend(uint(2, 1));
            }
            // ExecClientMessage.mcpResult -> McpToolResult.success.
            let reply = bytes(
                2,
                &[
                    uint(1, call.request_id),
                    string(15, &call.exec),
                    bytes(11, &bytes(1, &encoded)),
                ]
                .concat(),
            );
            frames.push(frame(0, &reply));
            frames.push(frame(0, &bytes(5, &bytes(1, &uint(1, call.request_id)))));
        }
        self.chat = ChatStream::new(&self.chat.model, id);
        self.listed = 0;
        self.answered = 0;
        self.pending.clear();
        self.parked = false;
        Ok(frames)
    }
}

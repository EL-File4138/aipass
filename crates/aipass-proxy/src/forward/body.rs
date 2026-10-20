use super::*;

pub(crate) enum ReplayableRequestBody {
    Memory(Bytes),
    File { file: std::fs::File, len: u64 },
}

#[derive(Default, Deserialize)]
pub(crate) struct RequestMetadata {
    #[serde(default)]
    pub(crate) stream: bool,
    pub(crate) model: Option<String>,
    /// A stable client supplied key lets providers reuse their prompt cache
    /// for a conversation. `prompt_cache_key` is the OpenAI API spelling;
    /// the other fields cover clients that expose the same value as a
    /// session or conversation identifier.
    #[serde(alias = "promptCacheKey")]
    pub(crate) prompt_cache_key: Option<String>,
    #[serde(alias = "sessionId")]
    pub(crate) session_id: Option<String>,
    #[serde(alias = "sessionKey")]
    pub(crate) session: Option<String>,
    #[serde(alias = "conversationId")]
    pub(crate) conversation_id: Option<String>,
    #[serde(alias = "previousResponseId")]
    pub(crate) previous_response_id: Option<String>,
    /// Responses clients may carry a stable conversation identifier as a
    /// string or as `{ "id": "..." }`.
    pub(crate) conversation: Option<serde_json::Value>,
}

impl ReplayableRequestBody {
    pub(crate) fn bytes(&self) -> Option<&Bytes> {
        match self {
            Self::Memory(bytes) => Some(bytes),
            Self::File { .. } => None,
        }
    }

    pub(crate) async fn json(&self) -> Option<serde_json::Value> {
        match self {
            Self::Memory(bytes) => serde_json::from_slice(bytes).ok(),
            Self::File { file, .. } => {
                let mut file = file.try_clone().ok()?;
                tokio::task::spawn_blocking(move || {
                    use std::io::{BufReader, Seek, SeekFrom};

                    file.seek(SeekFrom::Start(0)).ok()?;
                    serde_json::from_reader(BufReader::new(file)).ok()
                })
                .await
                .ok()
                .flatten()
            }
        }
    }

    pub(crate) fn len(&self) -> u64 {
        match self {
            Self::Memory(bytes) => bytes.len() as u64,
            Self::File { len, .. } => *len,
        }
    }

    pub(crate) async fn metadata(&self) -> Option<RequestMetadata> {
        self.parse_metadata().await
    }

    pub(crate) async fn parse_metadata<T: serde::de::DeserializeOwned + Send + 'static>(
        &self,
    ) -> Option<T> {
        match self {
            Self::Memory(bytes) => serde_json::from_slice(bytes).ok(),
            Self::File { file, .. } => {
                let mut file = file.try_clone().ok()?;
                tokio::task::spawn_blocking(move || {
                    use std::io::{BufReader, Seek, SeekFrom};

                    file.seek(SeekFrom::Start(0)).ok()?;
                    serde_json::from_reader(BufReader::new(file)).ok()
                })
                .await
                .ok()
                .flatten()
            }
        }
    }

    pub(crate) async fn request_body(&self) -> Result<reqwest::Body, std::io::Error> {
        match self {
            Self::Memory(bytes) => Ok(reqwest::Body::from(bytes.clone())),
            Self::File { file, .. } => {
                use std::io::{Seek, SeekFrom};

                let mut file = file.try_clone()?;
                file.seek(SeekFrom::Start(0))?;
                let file = tokio::fs::File::from_std(file);
                Ok(reqwest::Body::from(file))
            }
        }
    }
}

pub(crate) enum RequestBodyReadError {
    TooLarge,
    Io(std::io::Error),
    Transport(String),
}

impl std::fmt::Debug for RequestBodyReadError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TooLarge => formatter.write_str("request body too large"),
            Self::Io(err) => formatter.debug_tuple("Io").field(err).finish(),
            Self::Transport(err) => formatter.debug_tuple("Transport").field(err).finish(),
        }
    }
}

pub(crate) async fn read_replayable_request_body(
    body: Incoming,
) -> Result<ReplayableRequestBody, RequestBodyReadError> {
    read_replayable_request_chunks(
        body.into_data_stream(),
        MAX_REQUEST_BODY_BYTES,
        REQUEST_BODY_MEMORY_THRESHOLD,
    )
    .await
}

pub(crate) async fn read_replayable_request_chunks<S, E>(
    stream: S,
    max_bytes: usize,
    memory_threshold: usize,
) -> Result<ReplayableRequestBody, RequestBodyReadError>
where
    S: Stream<Item = Result<Bytes, E>> + Send,
    E: std::fmt::Display,
{
    let mut stream = Box::pin(stream);
    let mut memory = Vec::new();
    let mut file: Option<tokio::fs::File> = None;
    let mut total = 0_usize;

    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|err| RequestBodyReadError::Transport(err.to_string()))?;
        total = total
            .checked_add(chunk.len())
            .ok_or(RequestBodyReadError::TooLarge)?;
        if total > max_bytes {
            return Err(RequestBodyReadError::TooLarge);
        }

        if let Some(file) = file.as_mut() {
            file.write_all(&chunk)
                .await
                .map_err(RequestBodyReadError::Io)?;
            continue;
        }
        if total <= memory_threshold {
            memory.extend_from_slice(&chunk);
            continue;
        }

        let file_handle = tempfile::tempfile().map_err(RequestBodyReadError::Io)?;
        let mut async_file = tokio::fs::File::from_std(file_handle);
        async_file
            .write_all(&memory)
            .await
            .map_err(RequestBodyReadError::Io)?;
        async_file
            .write_all(&chunk)
            .await
            .map_err(RequestBodyReadError::Io)?;
        memory.clear();
        file = Some(async_file);
    }

    match file {
        Some(mut file) => {
            file.flush().await.map_err(RequestBodyReadError::Io)?;
            let file = file.into_std().await;
            Ok(ReplayableRequestBody::File {
                file,
                len: total as u64,
            })
        }
        None => Ok(ReplayableRequestBody::Memory(Bytes::from(memory))),
    }
}

use super::*;
use aipass_proxy_conversion::providers::{grok::Adapter, ProviderStream};

/// Apply the same Rust adapter to API responses and adapted downstream WS.
/// Reading a submitted response has no deadline; dropping this stream cancels IO.
pub(super) fn normalize(
    mut response: SubscriptionResponse,
    adapter: Adapter,
) -> SubscriptionResponse {
    if !response.status.is_success() {
        return response;
    }
    let sse = response
        .headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.contains("text/event-stream"));
    response.headers.remove(header::CONTENT_LENGTH);
    response.headers.remove(header::CONTENT_ENCODING);
    response.body = Box::pin(stream::unfold(
        (
            response.body,
            adapter,
            Vec::<u8>::new(),
            VecDeque::new(),
            false,
        ),
        move |(mut source, mut adapter, mut buffer, mut queue, mut done)| async move {
            loop {
                if let Some(chunk) = queue.pop_front() {
                    return Some((
                        Ok(Bytes::from(chunk)),
                        (source, adapter, buffer, queue, done),
                    ));
                }
                if done {
                    return None;
                }
                let next = source.next().await;
                let output = match next {
                    Some(Err(error)) => {
                        return Some((Err(error), (source, adapter, buffer, queue, true)));
                    }
                    Some(Ok(bytes)) if sse => adapter.push(&bytes),
                    Some(Ok(bytes)) => {
                        buffer.extend_from_slice(&bytes);
                        if buffer.len() > 32 * 1024 * 1024 {
                            Err("Grok response exceeds limit".into())
                        } else {
                            continue;
                        }
                    }
                    None if sse => {
                        done = true;
                        adapter.finish()
                    }
                    None => {
                        done = true;
                        serde_json::from_slice::<serde_json::Value>(&buffer)
                            .map_err(|_| "invalid Grok response".to_string())
                            .and_then(|mut body| {
                                adapter.response(&mut body)?;
                                Ok(vec![body.to_string().into_bytes()])
                            })
                    }
                };
                match output {
                    Ok(chunks) => queue.extend(chunks),
                    Err(error) => {
                        return Some((
                            Err(std::io::Error::other(error).into()),
                            (source, adapter, buffer, queue, true),
                        ))
                    }
                }
            }
        },
    ));
    response
}

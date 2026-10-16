use super::*;
use aipass_proxy_conversion::providers::connect::text;

struct Close(Arc<Entry>);
impl Drop for Close {
    fn drop(&mut self) {
        if let Ok(mut state) = self.0.state.lock() {
            state.closed = true;
            state.busy = false;
            state.parked = false;
            state.pending.clear();
            state.text.clear();
        }
    }
}
pub(super) fn start(
    entry: Arc<Entry>,
    mut commands: mpsc::Receiver<Turn>,
    mut abort: watch::Receiver<bool>,
    token: SensitiveString,
    outbound: Option<UpstreamProxyConfig>,
    base: Option<String>,
    model: String,
) {
    let _close = Close(entry.clone());
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build();
    let Ok(runtime) = runtime else {
        return;
    };
    runtime.block_on(async {
        let first=tokio::select! { _=abort.changed()=>return,turn=commands.recv()=>turn };
        let Some(first)=first else {return;};
        let mut output=first.output;
        let mut outer_abort = abort.clone();
        let result=async {
            if *abort.borrow() {return Err("Cursor request cancelled".into());}
            let client=transport(outbound.as_ref())?;
            #[cfg(test)]
            let client=if base.as_ref().is_some_and(|b|b.starts_with("http://127.0.0.1:")) { Client::builder().no_proxy().http2_prior_knowledge().build().unwrap() } else {client};
            let run=codec::request(&first.body,&model,&entry.session)?;
            let mut base=match base {Some(base)=>base,None=>agent_url_transport(&client,token.expose()).await};
            let mut retried=false;
            let (upload,response)=loop {
                let (upload,response)=open_transport(&client,token.expose(),&base,run.first.clone()).await?;
                if response.status().is_success() {break (upload,response);}
                let status=response.status().as_u16();
                let data=read_bytes(response,LIMIT).await?;
                let value=serde_json::from_slice(&data).unwrap_or(json!({"message":"Cursor request failed"}));
                let (status,message)=codec::failure(status,&value);
                if !retried && message.to_ascii_lowercase().contains("region") {
                    let next=agent_url_transport(&client,token.expose()).await;
                    if next!=base {base=next;retried=true;continue;}
                }
                output.send(Ok(Event::Rejected(status,message))).await.map_err(|_|"Cursor request cancelled")?;
                return Ok(());
            };
            if response.version()!=reqwest::Version::HTTP_2 {return Err("Cursor requires bidirectional HTTP/2".into());}
            let mut converter=codec::CursorStream::new_retained(s(&first.body,"model"),&format!("chatcmpl-{}",hex_id()),run);
            drop(first.body);
            let mut stream=response.bytes_stream();
            let mut heartbeat=tokio::time::interval(Duration::from_secs(5));
            heartbeat.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            let created=Instant::now();
            let mut parked_at=None;
            let mut count=0usize;
            let mut segment_text=String::new();
            loop {
                if *abort.borrow() {return Err("Cursor request cancelled".into());}
                tokio::select! {
                    _=abort.changed()=>return Err("Cursor request cancelled".into()),
                    _=output.closed(),if parked_at.is_none()=>return Err("Cursor downstream closed".into()),
                    _=heartbeat.tick()=>{
                        // This is an idle retention bound, never a generation
                        // deadline. A running model can take as long as needed.
                        if parked_at.is_some_and(|at:Instant|at.elapsed()>Duration::from_secs(600)) {return Err("Cursor tool session expired".into());}
                        upload.try_send(Ok(codec::CursorStream::heartbeat())).map_err(|_|"Cursor upload stalled")?;
                    },
                    turn=commands.recv(),if parked_at.is_some()=>{
                        let turn=turn.ok_or("Cursor session closed")?;
                        output=turn.output;
                        // Retire before any result upload when the transport is
                        // old. An uncertain result upload is never reconstructed.
                        if created.elapsed()>Duration::from_secs(840) {return Err("Cursor native connection retired; restart the conversation".into());}
                        let messages=turn.body["messages"].as_array().ok_or("Cursor messages missing")?;
                        let first_result=messages.iter().rposition(|m|m["role"]!="tool").map_or(0,|i|i+1);
                        let frames=converter.resume(&messages[first_result..],&format!("chatcmpl-{}",hex_id()))?;
                        {
                            let mut state=entry.state.lock().map_err(|_|"Cursor session unavailable")?;
                            state.history=digest(&json!(messages));state.history_len=messages.len();state.pending.clear();state.text.clear();state.parked=false;
                        }
                        // State is reserved before the first write. A partial
                        // batch can only fail this Run, never trigger a replay.
                        for frame in frames {write(&upload,frame).await?;}
                        parked_at=None;segment_text.clear();
                    },
                    data=stream.next()=>{
                        let data=data.ok_or("Cursor native response ended before TurnEnded")?.map_err(|_|"Cursor native response interrupted")?;
                        count=count.saturating_add(data.len());
                        if count>128*1024*1024 {return Err("Cursor native reply exceeds limit".into());}
                        let batch=converter.push(&data)?;
                        if let Some((status,message))=batch.error {
                            if !converter.has_output() {output.send(Ok(Event::Rejected(status,message))).await.map_err(|_|"Cursor request cancelled")?;return Ok(());}
                            return Err("Cursor native response failed after generation started".into());
                        }
                        for frame in batch.outgoing {write(&upload,frame).await?;}
                        for chunk in batch.chunks {
                            // Keep only the text needed to verify the pending
                            // assistant turn; no full transcript is retained.
                            for line in std::str::from_utf8(&chunk).map_err(|_|"invalid Cursor chat encoding")?.lines().filter_map(|l|l.strip_prefix("data: ")) {
                                if let Ok(value)=serde_json::from_str::<Value>(line) {
                                    if let Some(delta)=value.pointer("/choices/0/delta/content") {segment_text.push_str(&text(delta));}
                                }
                            }
                            if segment_text.len()>16*1024*1024 {return Err("Cursor pending text exceeds limit".into());}
                            tokio::select! {_=abort.changed()=>return Err("Cursor request cancelled".into()), result=output.send(Ok(Event::Data(chunk)))=>result.map_err(|_|"Cursor downstream closed")?};
                        }
                        if converter.done() {
                            converter.finish()?;
                            if let Ok(mut state)=entry.state.lock() {state.closed=true;state.busy=false;state.parked=false;}
                            output.send(Ok(Event::End)).await.map_err(|_|"Cursor downstream closed")?;
                            return Ok(());
                        }
                        if converter.parked() && parked_at.is_none() {
                            {
                                let mut state=entry.state.lock().map_err(|_|"Cursor session unavailable")?;
                                state.pending=converter.pending().to_vec();state.text=std::mem::take(&mut segment_text);state.parked=true;state.busy=false;state.touched=Instant::now();
                            }
                            {
                                let mut pool=pool().lock().map_err(|_|"Cursor coordinator unavailable")?;
                                if *abort.borrow() {return Err("Cursor request cancelled".into());}
                                if pool.calls.len().saturating_add(converter.pending().len())>4096 {return Err("Cursor session history capacity reached".into());}
                                for call in converter.pending() {pool.calls.insert(call.id.clone(),(entry.owner,entry.binding.clone(),Instant::now()));}
                            }
                            output.send(Ok(Event::End)).await.map_err(|_|"Cursor downstream closed")?;
                            parked_at=Some(Instant::now());
                        }
                    }
                }
            }
        };
        let result=tokio::select! {_=outer_abort.changed()=>Err("Cursor request cancelled".into()),result=result=>result};
        if let Err(error)=result {let _=output.send(Err(error)).await;}
    });
}
async fn write(upload: &Upload, frame: Vec<u8>) -> Result<()> {
    tokio::time::timeout(Duration::from_secs(30), upload.send(Ok(frame)))
        .await
        .map_err(|_| "Cursor result upload stalled; outcome uncertain")?
        .map_err(|_| "Cursor result upload closed; outcome uncertain".to_string())
}

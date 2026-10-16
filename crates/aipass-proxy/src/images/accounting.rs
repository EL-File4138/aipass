use super::*;

impl Drop for Attempt {
    fn drop(&mut self) {
        let _ = self.state.usage.record_attempt(&AttemptRecord {
            id: Uuid::new_v4(),
            request_id: Some(self.request_id),
            started_at: self.started_at,
            duration_ms: self.started.elapsed().as_millis() as u64,
            first_token_ms: None,
            route_id: self.route_id,
            target_id: self.target.config.id,
            provider_entry_id: self.target.config.provider_entry_id,
            secret_id: self.target.config.secret_id.clone(),
            model: self.model.clone(),
            status: self.status.map(|s| s.as_u16()),
            success: self.outcome,
        });
        if self.record_request {
            if let Some(success) = self.outcome {
                record_request(&self.state, success, None);
            }
            let _ = self.state.usage.record(&UsageRecord {
                id: self.request_id,
                started_at: self.request_started_at,
                duration_ms: self.request_started.elapsed().as_millis() as u64,
                first_token_ms: None,
                route_id: self.route_id,
                provider_entry_id: self.target.config.provider_entry_id,
                secret_id: self.target.config.secret_id.clone(),
                model: self.model.clone(),
                inbound_protocol: self.inbound,
                upstream_protocol: self.upstream,
                status: if self.outcome == Some(true) {
                    self.status.unwrap_or(StatusCode::OK).as_u16()
                } else {
                    502
                },
                attempts: self.number,
                // Image billing differs from mainline text token pricing. Do not
                // invent a text-model cost from an image response's usage object.
                input_tokens: 0,
                output_tokens: 0,
                cache_read_tokens: 0,
                cache_creation_tokens: 0,
                cache_creation_1h_tokens: 0,
                estimated_cost_micros: 0,
            });
        }
    }
}

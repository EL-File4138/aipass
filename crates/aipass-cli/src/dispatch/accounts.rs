use crate::*;

pub(crate) fn handle_accounts_command(
    json: bool,
    vault: Option<PathBuf>,
    cli_password: Option<String>,
    command: AccountsCommand,
) -> Result<()> {
    match command {
        AccountsCommand::Policy {
            id,
            allow_paid_credits,
            auto_reset_credits,
            native_group,
            disable_native_switch,
            return_preferred,
        } => {
            let agent = CliAgent::from_parts(vault, cli_password)?;
            let mut options: aipass_agent_protocol::ProviderRuntimeOptions =
                agent.request(AgentRequest::ProviderRuntimeGet { id })?;
            let changed = allow_paid_credits.is_some()
                || auto_reset_credits.is_some()
                || native_group.is_some()
                || disable_native_switch
                || return_preferred.is_some();
            if let Some(value) = allow_paid_credits {
                options.subscription.allow_paid_credits = value;
            }
            if let Some(value) = auto_reset_credits {
                options.subscription.auto_reset_credits = value;
            }
            if let Some(route_id) = native_group {
                options.subscription.native_switch =
                    Some(aipass_agent_protocol::NativeSwitchPolicy {
                        route_id,
                        return_preferred: return_preferred.unwrap_or(true),
                    });
            }
            if disable_native_switch {
                options.subscription.native_switch = None;
            }
            if let Some(value) = return_preferred {
                options
                    .subscription
                    .native_switch
                    .as_mut()
                    .context("--return-preferred requires an enabled native group")?
                    .return_preferred = value;
            }
            if changed {
                options = agent.request(AgentRequest::ProviderRuntimeSet { id, options })?;
            }
            output(
                json,
                serde_json::to_value(options.subscription)?,
                "Subscription policy",
            )
        }
        AccountsCommand::Reset {
            id,
            credit_id,
            request_id,
        } => {
            let agent = CliAgent::from_parts(vault, cli_password)?;
            let result: serde_json::Value = agent.request(AgentRequest::SubscriptionReset {
                entry_id: id,
                request_id,
                credit_id,
            })?;
            let text = result["outcome"].as_str().unwrap_or("unknown").to_owned();
            output(json, result, &text)
        }
        AccountsCommand::Import {
            provider_ids,
            directory,
        } => {
            if directory.is_some() && provider_ids.len() != 1 {
                anyhow::bail!("--directory requires exactly one --provider");
            }
            let sources = directory
                .map(|root| aipass_agent_protocol::SubscriptionImportSource {
                    provider: provider_ids[0].clone(),
                    root,
                    selector: String::new(),
                })
                .into_iter()
                .collect();
            let agent = CliAgent::from_parts(vault, cli_password)?;
            let task: aipass_agent_protocol::SubscriptionImportTask =
                agent.request(AgentRequest::SubscriptionImportStart {
                    input: aipass_agent_protocol::SubscriptionImportInput {
                        provider_ids,
                        sources,
                        retry: None,
                    },
                })?;
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?;
            runtime.block_on(async {
                let interrupt = tokio::signal::ctrl_c();
                tokio::pin!(interrupt);
                let mut cancelling = false;
                loop {
                    // Poll/cancel must never reopen a vault locked after import began.
                    let status: aipass_agent_protocol::SubscriptionImportTask = agent.request_no_unlock(AgentRequest::SubscriptionImportPoll { ticket: task.ticket })?;
                    if matches!(status.phase.as_str(), "complete" | "cancelled") {
                        let imported = status.results.iter().filter(|r| r.status == aipass_agent_protocol::SubscriptionImportStatus::Imported).count();
                        if !json {
                            for item in &status.results {
                                println!("{} · {} · {} · {}{}", item.source.provider,
                                    item.account_identity.as_deref().unwrap_or("—"),
                                    serde_json::to_value(item.status)?.as_str().unwrap_or("failed"),
                                    item.source.root.display(),
                                    item.error_code.as_ref().map(|code| format!(" · {code}")).unwrap_or_default());
                            }
                        }
                        output(json, serde_json::to_value(&status)?, &format!("{imported} imported · {} sources checked", status.completed))?;
                        return Ok::<(), anyhow::Error>(());
                    }
                    tokio::select! {
                        _ = tokio::time::sleep(std::time::Duration::from_secs(1)) => {},
                        signal = &mut interrupt, if !cancelling => {
                            signal?;
                            let _: aipass_agent_protocol::SubscriptionImportTask = agent.request_no_unlock(AgentRequest::SubscriptionImportCancel { ticket: task.ticket })?;
                            cancelling = true;
                        }
                    }
                }
            })?;
            Ok(())
        }
        AccountsCommand::Refresh { provider_ids } => {
            let agent = CliAgent::from_parts(vault.clone(), cli_password.clone())?;
            let results: Vec<OfficialAccountRefreshResult> =
                agent.request(AgentRequest::OfficialAccountsRefresh { provider_ids })?;
            let imported = results
                .iter()
                .filter(|result| result.status == "imported")
                .count();
            let refreshed = results
                .iter()
                .filter(|result| result.status == "refreshed")
                .count();
            output(
                json,
                serde_json::to_value(&results)?,
                &format!("{imported} imported, {refreshed} refreshed"),
            )
        }
    }
}

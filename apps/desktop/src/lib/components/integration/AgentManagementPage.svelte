<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { Banner, Button, IconButton, SelectField, scrollMask } from "@aipass/ui";
  import { RefreshCw, Terminal } from "lucide-svelte";
  import type { ProviderEntry } from "@aipass/schemas";
  import { t } from "../../stores/i18n";
  import type { ToolDetection, ToolConfigTarget, ToolConfigStatus, ToolSwitchRequest, ToolConfigPreview, ToolConfigApplyResult } from "../../types";
  import { integrationToolDefinitions } from "../../utils/integrations";
  import { agentCredentialChoices } from "../../utils/agent-credentials";
  import IntegrationToolIcon from "./IntegrationToolIcon.svelte";
  import IntegrationPreviewDialog from "./IntegrationPreviewDialog.svelte";

  export let entries: ProviderEntry[] = [];
  export let invokeTauri: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  export let onRefreshEntries: () => Promise<void>;
  let detections: ToolDetection[] = [];
  let statuses: Partial<Record<ToolConfigTarget, ToolConfigStatus>> = {};
  let selections: Partial<Record<ToolConfigTarget, string>> = {};
  let loading = true;
  let busy: ToolConfigTarget | undefined;
  let error = "";
  let message = "";
  let generation = 0;
  let mounted = false;
  let pending: ToolSwitchRequest | undefined;
  let preview: ToolConfigPreview | undefined;
  let previewOpen = false;
  let selectionRevision = "";
  $: revision = JSON.stringify(entries);
  $: tools = integrationToolDefinitions.filter(tool => detections.some(d => d.tool === tool.id && (d.binaryFound || d.configPath)));
  $: choices = Object.fromEntries(tools.map(tool => [tool.id, agentCredentialChoices(entries, tool.id)]));
  $: if (previewOpen && selectionRevision !== revision && !busy) { previewOpen = false; pending = undefined; }

  async function refresh(clearError = true) {
    const current = ++generation;
    loading = true; if (clearError) error = "";
    try {
      const detected = await invokeTauri<ToolDetection[]>("tool_config_detect");
      const active = detected.filter(d => d.binaryFound || d.configPath);
      const results = await Promise.all(active.map(async d => {
        try { return await invokeTauri<ToolConfigStatus>("tool_config_status", { tool: d.tool }); }
        catch { return { tool: d.tool, state: "storage_unavailable", overrides: [] } as ToolConfigStatus; }
      }));
      if (!mounted || generation !== current) return;
      detections = detected;
      statuses = Object.fromEntries(results.map(s => [s.tool, s]));
      const next: typeof selections = {};
      for (const s of results) {
        const available = agentCredentialChoices(entries, s.tool);
        next[s.tool] = available.find(c => c.value === selections[s.tool])?.value
          ?? available.find(c => c.request.id === s.entryId && c.request.secretId === s.secretId)?.value ?? "";
      }
      selections = next;
    } catch (e) { if (mounted && generation === current) error = String(e); }
    finally { if (mounted && generation === current) loading = false; }
  }
  function focused() { if (!busy && !previewOpen) void refresh(); }
  onMount(() => { mounted = true; void refresh(); window.addEventListener("focus", focused); });
  onDestroy(() => { mounted = false; generation++; window.removeEventListener("focus", focused); });

  async function prepare(tool: ToolConfigTarget) {
    const choice = choices[tool]?.find(c => c.value === selections[tool]);
    if (!choice || busy || loading) return;
    busy = tool; error = message = "";
    const current = generation;
    const sourceRevision = revision;
    try {
      const request = { ...choice.request };
      const result = await invokeTauri<ToolConfigPreview>("tool_config_preview", { request });
      if (!mounted || current !== generation || sourceRevision !== revision) return;
      if (!result.previewId) throw new Error($t("agents.previewUnavailable"));
      pending = { ...request, previewId: result.previewId }; preview = result;
      selectionRevision = revision; previewOpen = true;
    } catch (e) { if (mounted && current === generation) error = String(e); }
    finally { if (mounted) busy = undefined; }
  }
  async function apply() {
    if (!pending || busy || selectionRevision !== revision) return;
    const request = { ...pending };
    busy = request.tool; error = "";
    try {
      const result = await invokeTauri<ToolConfigApplyResult>("agent_management_apply", { request });
      if (!mounted) return;
      previewOpen = false;
      if (result.outcome === "login_required") {
        error = $t("integration.loginRequired");
      } else if (result.outcome && result.outcome !== "applied") {
        error = result.message || $t("integration.externalChange");
      } else {
        message = $t("agents.switched"); selections = { ...selections, [request.tool]: `${request.id}:${request.secretId}` };
      }
    } catch (e) { if (mounted) { previewOpen = false; error = String(e); } }
    finally {
      if (mounted) {
        busy = undefined; pending = undefined;
        try { await onRefreshEntries(); } catch (e) { if (mounted) error ||= String(e); }
        if (mounted) await refresh(false);
      }
    }
  }
  async function restore(tool: ToolConfigTarget) {
    const operationId = statuses[tool]?.operationId;
    if (!operationId || busy) return;
    busy = tool; error = message = "";
    try {
      const result = await invokeTauri<ToolConfigApplyResult>("tool_config_rollback", { operationId });
      if (!mounted) return;
      if (result.outcome && result.outcome !== "applied") error = result.message || $t("integration.externalChange");
      else message = $t("integration.restored");
    } catch (e) { if (mounted) error = String(e); }
    finally { if (mounted) { busy = undefined; await refresh(false); } }
  }
</script>

<section class="agent-management" aria-label={$t("agents.title")}>
  <header>
    <h1>{$t("agents.title")}</h1>
    <IconButton label={$t("integration.scanInstallations")} disabled={loading || Boolean(busy)} on:click={() => refresh()}>
      <RefreshCw size={16} class={loading ? "spinning" : ""} />
    </IconButton>
  </header>
  <div class="body" use:scrollMask>
    {#if message}<Banner tone="success">{message}</Banner>{/if}
    {#if error}<Banner tone="danger">{error}</Banner>{/if}
    <div class="agent-list" aria-busy={loading}>
      {#each tools as tool (tool.id)}
        {@const status = statuses[tool.id]}
        {@const available = choices[tool.id] ?? []}
        {@const detection = detections.find(d => d.tool === tool.id)}
        {@const needsRuntime = tool.id === "cursor" && !detection?.localRuntimeFound}
        <div class="agent-row">
          <div class="identity">
            <span class="tool-icon"><IntegrationToolIcon tool={tool.id} size={24} /></span>
            <div class="tool-copy"><strong>{tool.name}</strong>
              <span class="current" title={status?.accountIdentity || status?.entryTitle}>
                {status?.entryTitle || status?.accountIdentity || $t("integration.unmanaged")}
              </span>
              {#if !detection?.binaryFound}<span class="subtle">{$t("agents.configOnly")}</span>{/if}
              {#if needsRuntime}<span class="subtle">{$t("agents.cursorRuntimeRequired")}</span>{/if}
            </div>
          </div>
          <div class="switch-controls">
            <SelectField value={selections[tool.id] ?? ""} options={available}
              placeholder={$t(available.length ? "agents.choose" : "agents.noCompatible")}
              ariaLabel={`${tool.name}: ${$t("agents.credential")}`} disabled={loading || Boolean(busy) || !available.length || needsRuntime}
              onValueChange={value => selections = { ...selections, [tool.id]: value }} />
            <Button size="sm" variant="secondary" disabled={loading || Boolean(busy) || !selections[tool.id] || needsRuntime || status?.state === "conflict" || status?.state === "storage_unavailable"}
              loading={busy === tool.id} on:click={() => prepare(tool.id)}>{$t("agents.switch")}</Button>
          </div>
          {#if status?.state && status.state !== "ready" && status.state !== "unmanaged"}
            <div class="row-note">{$t(`integration.state.${status.state}`)}
              {#if status.operationId}<Button size="sm" disabled={Boolean(busy) || loading} on:click={() => restore(tool.id)}>{$t("integration.restore")}</Button>{/if}
            </div>
          {/if}
          {#if status?.overrides.length}<div class="row-note">{$t("integration.environmentOverrides", { keys: status.overrides.join(", ") })}</div>{/if}
        </div>
      {:else}
        <div class="empty"><Terminal size={28} /><strong>{$t(loading ? "agents.scanning" : "agents.empty")}</strong><span>{$t("agents.emptyDetail")}</span></div>
      {/each}
    </div>
  </div>
</section>
<IntegrationPreviewDialog open={previewOpen} {preview} toolName={integrationToolDefinitions.find(t => t.id === pending?.tool)?.name ?? ""}
  busy={Boolean(busy)} managedSwitch onConfirm={apply} onOpenChange={open => { if (!busy) { previewOpen = open; if (!open) pending = undefined; } }} />

<style lang="scss">
  .agent-management { grid-column: 2 / -1; display: flex; flex-direction: column; background: var(--surface); }
  header { display: flex; align-items: center; justify-content: space-between; gap: 16px; padding: 22px 24px 18px; border-bottom: 1px solid var(--divider); flex-shrink: 0; }
  h1 { margin: 0; font-size: 18px; font-weight: 650; }
  .body { padding: 20px 24px; display: flex; flex-direction: column; gap: 16px; overflow: auto; min-height: 0; }
  .agent-list { border: 1px solid var(--border); border-radius: 12px; overflow: hidden; flex-shrink: 0; }
  .agent-row { display: grid; grid-template-columns: minmax(160px, 1fr) minmax(260px, 1.4fr); align-items: center; gap: 12px 20px; padding: 15px 18px; border-bottom: 1px solid var(--divider); }
  .agent-row:last-child { border-bottom: 0; }
  .identity { display: flex; align-items: center; gap: 13px; min-width: 0; }
  .tool-icon { display: grid; place-items: center; width: 36px; height: 36px; flex-shrink: 0; color: var(--text); background: var(--surface-2); border-radius: 8px; }
  .tool-copy { display: flex; flex-direction: column; gap: 5px; min-width: 0; }
  .tool-copy strong { font-size: 14px; font-weight: 600; }
  .current { color: var(--text-secondary); font-size: 11px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .subtle { color: var(--text-tertiary); font-size: 11px; }
  .switch-controls { display: grid; grid-template-columns: minmax(0, 1fr) auto; align-items: end; gap: 10px; min-width: 0; }
  .switch-controls :global(.btn) { min-height: 34px; }
  .row-note { grid-column: 1 / -1; display: flex; align-items: center; gap: 12px; color: var(--warning); font-size: 11px; overflow-wrap: anywhere; }
  .empty { display: flex; flex-direction: column; align-items: center; gap: 12px; padding: 48px 20px; text-align: center; color: var(--text-secondary); font-size: 12px; }
  :global(.spinning) { animation: agent-scan 1s linear infinite; }
  @keyframes agent-scan { to { transform: rotate(360deg); } }
</style>

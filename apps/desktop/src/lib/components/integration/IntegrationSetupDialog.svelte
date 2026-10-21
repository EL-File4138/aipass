<script lang="ts">
  import { Dialog } from "bits-ui";
  import { Banner, Button } from "@aipass/ui";
  import { X } from "lucide-svelte";
  import { t } from "../../stores/i18n";
  import type { IntegrationToolDefinition } from "../../utils/integrations";
  import type { MaybePromise } from "../../types";

  export let tool: IntegrationToolDefinition | undefined = undefined;
  export let endpoint = "";
  export let model = "";
  export let protocol = "";
  export let onCopyKey: () => MaybePromise<unknown>;
  export let onClose: () => void;
  let error = "";
  let copied = "";
  let busy = false;
  $: if (!tool) { error = ""; copied = ""; }
  async function copy(kind: string, value?: string) {
    if (busy) return;
    busy = true; error = ""; copied = "";
    try {
      if (kind === "key") {
        if (await onCopyKey() === false) return;
      } else { await navigator.clipboard.writeText(value ?? ""); }
      copied = kind;
    } catch (e) { error = String(e); }
    finally { busy = false; }
  }
</script>

<Dialog.Root open={Boolean(tool)} onOpenChange={open => { if (!open) onClose(); }}>
  <Dialog.Portal>
    <Dialog.Overlay class="setup-overlay" />
    <Dialog.Content class="setup-dialog">
      <header><Dialog.Title>{tool?.name}</Dialog.Title><Dialog.Close aria-label={$t("common.close")}><X size={16} /></Dialog.Close></header>
      <Dialog.Description>{$t(`integration.setup.${tool?.setupGuide ?? "cline"}`)}</Dialog.Description>
      <dl>
        <div><dt>{$t("integration.setupProtocol")}</dt><dd>{protocol}</dd></div>
        <div><dt>{$t("integration.setupEndpoint")}</dt><dd><code>{endpoint}</code></dd><Button size="sm" disabled={busy || !endpoint} on:click={() => copy("endpoint", endpoint)}>{$t(copied === "endpoint" ? "integration.setupCopied" : "common.copy")}</Button></div>
        <div><dt>{$t("integration.setupModel")}</dt><dd><code>{model}</code></dd><Button size="sm" disabled={busy || !model} on:click={() => copy("model", model)}>{$t(copied === "model" ? "integration.setupCopied" : "common.copy")}</Button></div>
        <div><dt>API Key</dt><dd></dd><Button size="sm" disabled={busy} on:click={() => copy("key")}>{$t(copied === "key" ? "integration.setupCopied" : "integration.setupCopyKey")}</Button></div>
      </dl>
      {#if error}<Banner tone="danger">{error}</Banner>{/if}
      <footer><Button on:click={onClose}>{$t("common.close")}</Button></footer>
    </Dialog.Content>
  </Dialog.Portal>
</Dialog.Root>

<style>
  :global(.setup-overlay){position:fixed;inset:0;z-index:80;background:rgba(0,0,0,.45);backdrop-filter:blur(4px)}
  :global(.setup-dialog){position:fixed;inset:50% auto auto 50%;transform:translate(-50%,-50%);z-index:81;width:min(580px,calc(100vw - 48px));max-height:calc(100vh - 48px);overflow:auto;padding:20px;border:1px solid var(--divider);border-radius:var(--radius-lg);background:var(--surface);color:var(--text);box-shadow:0 20px 80px rgba(0,0,0,.3)}
  header{display:flex;justify-content:space-between;align-items:center;font-size:15px;font-weight:600;margin-bottom:12px}header :global(button){color:var(--text-secondary)}
  :global(.setup-dialog [data-dialog-description]){font-size:12px;line-height:1.6;color:var(--text-secondary);margin:0 0 16px}
  dl{margin:0;display:flex;flex-direction:column;gap:10px}dl>div{display:flex;align-items:center;gap:12px;min-width:0}dt{width:68px;flex-shrink:0;font-size:12px;color:var(--text-secondary)}dd{margin:0;flex:1;min-width:0;font-size:12px;overflow-wrap:anywhere}code{font-size:11px}footer{display:flex;justify-content:flex-end;margin-top:18px}
</style>

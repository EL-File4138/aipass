<script lang="ts">
  import { Popover } from "bits-ui";
  import { Button, IconButton } from "@aipass/ui";
  import { FolderOpen, X } from "lucide-svelte";
  import { t } from "../../stores/i18n";
  export let path: string;
  export let onOpen: () => Promise<unknown>;
  let trigger: HTMLButtonElement | null = null;
  let opening = false;
  let error = false;
  async function openLocation() {
    if (opening) return;
    opening = true; error = false;
    try { await onOpen(); } catch { error = true; }
    finally { opening = false; }
  }
</script>

<Popover.Root>
  <Popover.Trigger bind:ref={trigger}>
    {#snippet child({ props })}<IconButton {...props} size="sm" label={$t("subscriptionImport.source.location")}><FolderOpen size={14} /></IconButton>{/snippet}
  </Popover.Trigger>
  <Popover.Portal>
    <Popover.Content class="account-location" side="bottom" align="start" sideOffset={6} onCloseAutoFocus={(event) => { event.preventDefault(); trigger?.focus(); }}>
      {#snippet child({ props, wrapperProps })}
        <div {...wrapperProps} style:z-index="220">
          <div {...props}>
            <div class="location-heading"><strong>{$t("subscriptionImport.source.path")}</strong><div class="location-actions"><Button size="sm" variant="ghost" loading={opening} on:click={() => { void openLocation(); }}>{$t("subscriptionImport.source.open")}</Button><Popover.Close class="location-close" aria-label={$t("common.close")}><X size={14} /></Popover.Close></div></div>
            <code>{path}</code>
            {#if error}<p class="location-error" role="alert">{$t("subscriptionImport.source.openFailed")}</p>{/if}
          </div>
        </div>
      {/snippet}
    </Popover.Content>
  </Popover.Portal>
</Popover.Root>

<style>
  :global(.account-location) { width: 360px; max-width: calc(100vw - 48px); padding: 12px; background: var(--surface); color: var(--text); border: 1px solid var(--border); border-radius: var(--radius); box-shadow: 0 4px 16px rgb(0 0 0 / 12%); font-size: 12px; }
  .location-heading { display: flex; align-items: center; justify-content: space-between; gap: 12px; margin-bottom: 8px; }
  .location-actions { display: flex; align-items: center; gap: 4px; }
  :global(.location-close) { display: inline-flex; align-items: center; justify-content: center; width: 24px; height: 24px; border-radius: var(--radius); color: var(--text-secondary); }
  :global(.location-close:hover) { background: var(--surface-2); }
  :global(.location-close:focus-visible) { outline: 2px solid var(--accent); }
  .location-error { margin: 8px 0 0; color: var(--danger); font-size: 11px; }
  code { display: block; padding: 8px 10px; background: var(--surface-2); border-radius: var(--radius); overflow-wrap: anywhere; user-select: text; font-size: 11px; line-height: 1.5; }
</style>

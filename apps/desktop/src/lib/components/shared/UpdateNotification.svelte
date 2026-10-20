<script lang="ts">
  import { Banner, Button, ProgressButton } from "@aipass/ui";
  import { t } from "../../stores/i18n";
  import type { UpdateProgress } from "../../services/updates";
  import type { MaybePromise } from "../../types";

  export let version: string;
  export let installing = false;
  export let confirmChecking = false;
  export let progress: UpdateProgress | undefined;
  export let error = "";
  export let onInstall: () => MaybePromise;
  export let onDismiss: () => MaybePromise;

  $: percent = progress?.totalBytes && progress.totalBytes > 0
    ? Math.min(100, Math.round(progress.downloadedBytes / progress.totalBytes * 100)) : undefined;
</script>

<div class="update-banner">
  <div class="update-notice" role="status">
    <span class="update-banner-text">{$t("updates.bannerTitle", { version })}</span>
    <div class="update-actions">
      <ProgressButton variant="primary" size="sm" on:click={onInstall}
        disabled={installing || confirmChecking} progress={percent}
        indeterminate={installing && (progress?.phase === "installing" || percent === undefined)}>
        {#if installing}
          {#if progress?.phase === "installing"}{$t("updates.installing")}
          {:else if percent !== undefined}{$t("updates.downloadProgress", { percent })}
          {:else}{$t("updates.downloading")}{/if}
        {:else}{$t("updates.installRestart")}{/if}
      </ProgressButton>
      <Button variant="ghost" size="sm" on:click={onDismiss} disabled={installing}>{$t("updates.later")}</Button>
    </div>
  </div>
  {#if error}<Banner tone="danger">{error}</Banner>{/if}
</div>

<style lang="scss">
  .update-banner {
    position: absolute;
    right: 16px;
    bottom: 16px;
    z-index: 60;
    display: grid;
    gap: 8px;
    width: min(420px, calc(100% - 32px));
  }
  .update-notice {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    align-items: center;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    box-shadow: var(--shadow-pop);
  }
  .update-banner-text { min-width: 0; color: var(--text); font-size: 12px; overflow-wrap: anywhere; }
  .update-actions { display: flex; align-items: center; gap: 8px; }
</style>

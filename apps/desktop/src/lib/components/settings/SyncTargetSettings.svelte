<script lang="ts">
  import { Button, Card, Field } from "@aipass/ui";
  import { Wifi } from "lucide-svelte";
  import { supportsCloudKit } from "../../platform";
  import { t } from "../../stores/i18n";
  import type { MaybePromise, SyncMode } from "../../types";
  import SegmentedControl from "../shared/SegmentedControl.svelte";

  export let syncMode: SyncMode = "local";
  export let syncFolder = "";
  export let webdavUrl = "";
  export let webdavUsername = "";
  export let webdavPassword = "";
  export let hasSavedWebdavPassword = false;
  export let syncBusy = false;
  export let savingSyncSettings = false;
  export let iCloudSupported = supportsCloudKit;
  export let onSave: () => MaybePromise = () => {};
  export let onRun: () => MaybePromise = () => {};
  export let onClear: () => MaybePromise = () => {};

  $: supportedTarget = syncMode !== "icloud" || iCloudSupported;
  $: syncReady = supportedTarget && (syncMode === "local" ? !!syncFolder.trim() : syncMode === "webdav" ? !!webdavUrl.trim() : true);
</script>

<Card title={$t("settings.syncTarget")}>
  <div class="rows">
    <SegmentedControl
      ariaLabel={$t("settings.syncTarget")}
      bind:value={syncMode}
      options={[
        { value: "local", label: $t("settings.local") },
        ...(iCloudSupported ? [{ value: "icloud", label: "iCloud" }] : []),
        { value: "onedrive", label: "OneDrive" },
        { value: "webdav", label: "WebDAV" }
      ]}
    />
    {#if syncMode === "local"}
      <Field label={$t("settings.folder")}>
        <input bind:value={syncFolder} placeholder="~/Sync/AIPass" />
      </Field>
    {:else if syncMode === "icloud"}
      <p class="hint">{$t(iCloudSupported ? "settings.icloudDesc" : "settings.icloudUnavailable")}</p>
    {:else if syncMode === "onedrive"}
      <p class="hint">{$t("settings.onedriveDesc")}</p>
    {:else}
      <Field label={$t("settings.url")}>
        <input bind:value={webdavUrl} placeholder="https://cloud.example/dav" />
      </Field>
      <Field label={$t("settings.username")}>
        <input bind:value={webdavUsername} autocomplete="username" placeholder={$t("settings.usernamePlaceholder")} />
      </Field>
      <Field label={$t("settings.password")}>
        <input bind:value={webdavPassword} type="password" autocomplete="current-password" placeholder="••••••••" />
      </Field>
      {#if hasSavedWebdavPassword}
        <p class="hint">{$t("settings.savedPasswordHint")}</p>
      {/if}
    {/if}
    <div class="row-actions">
      <Button variant="secondary" on:click={() => onSave()} disabled={syncBusy || !supportedTarget} loading={savingSyncSettings}>
        {$t("common.save")}
      </Button>
      {#if syncMode === "webdav" && hasSavedWebdavPassword}
        <Button variant="ghost" on:click={() => onClear()} disabled={syncBusy || savingSyncSettings}>{$t("settings.clearPassword")}</Button>
      {/if}
      <Button variant="primary" on:click={() => onRun()} disabled={!syncReady || syncBusy || savingSyncSettings}>
        <Wifi size={14} /> {syncBusy ? $t("syncStatus.syncing") : $t("settings.syncNow")}
      </Button>
    </div>
  </div>
</Card>

<style>
  .rows { display: flex; flex-direction: column; gap: 14px; padding: 16px; }
  .row-actions { display: flex; justify-content: flex-end; gap: 8px; }
  .hint { color: var(--text-tertiary); font-size: 12px; line-height: 1.45; }
</style>

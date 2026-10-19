<script lang="ts">
  import { Card, SwitchField } from "@aipass/ui";
  import { t } from "../../stores/i18n";
  import type { MaybePromise, SecurityPreferences } from "../../types";
  export let autoLockMinutes = 60;
  export let clipboardClearSeconds = 45;
  export let credentialClickAction: "reveal" | "copy" = "reveal";
  export let confirmSecretCopy = true;
  export let lockOnSleep = true;
  export let lockOnScreenLock = true;
  export let autoLockOptions: { value: number; label: string }[] = [];
  export let onSavePreferences: (preferences?: SecurityPreferences) => MaybePromise = () => {};
  async function savePreferences() {
    await onSavePreferences({ autoLockMinutes, clipboardClearSeconds, credentialClickAction, confirmSecretCopy, lockOnSleep, lockOnScreenLock });
  }
</script>

<Card title={$t("settings.lockPolicy")}>
  <div class="rows">
    <div class="row">
      <div class="row-text">
        <span class="row-label">{$t("settings.autoLock")}</span>
        <span class="row-desc">{$t("settings.autoLockDesc")}</span>
      </div>
      <select
        class="select-input"
        value={autoLockMinutes}
        on:change={(event) => { autoLockMinutes = Number(event.currentTarget.value); void savePreferences(); }}
        aria-label={$t("settings.autoLock")}
      >
        {#each autoLockOptions as option}
          <option value={option.value}>{option.label}</option>
        {/each}
      </select>
    </div>
    <div class="row">
      <div class="row-text">
        <span class="row-label">{$t("settings.clipboardClear")}</span>
        <span class="row-desc">{$t("settings.clipboardClearDesc")}</span>
      </div>
      <input
        class="num-input"
        type="number"
        min="1"
        max="600"
        aria-label={$t("settings.clipboardClear")}
        value={clipboardClearSeconds}
        on:input={(event) => { clipboardClearSeconds = event.currentTarget.valueAsNumber; }}
        on:change={savePreferences}
      />
    </div>
    <div class="row">
      <span class="row-label">{$t("settings.credentialClickAction")}</span>
      <select class="select-input" value={credentialClickAction} aria-label={$t("settings.credentialClickAction")} on:change={(event) => { credentialClickAction = event.currentTarget.value as "reveal" | "copy"; void savePreferences(); }}>
        <option value="reveal">{$t("settings.credentialClickReveal")}</option>
        <option value="copy">{$t("settings.credentialClickCopy")}</option>
      </select>
    </div>
    <SwitchField label={$t("settings.confirmSecretCopy")} checked={confirmSecretCopy} onCheckedChange={(checked) => { confirmSecretCopy = checked; void savePreferences(); }} />
    <SwitchField
      label={$t("settings.lockOnSleep")}
      description={$t("settings.lockOnSleepDesc")}
      checked={lockOnSleep}
      onCheckedChange={(checked) => { lockOnSleep = checked; void savePreferences(); }}
    />
    <SwitchField
      label={$t("settings.lockOnScreenLock")}
      description={$t("settings.lockOnScreenLockDesc")}
      checked={lockOnScreenLock}
      onCheckedChange={(checked) => { lockOnScreenLock = checked; void savePreferences(); }}
    />
  </div>
</Card>

<style>

  .rows { display: flex; flex-direction: column; gap: 12px; padding: 16px; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 16px; min-height: 32px; }
  .row-text { display: flex; flex-direction: column; gap: 2px; min-width: 0; }
  .row-label { font-size: 13px; font-weight: 500; color: var(--text); }
  .row-desc { font-size: 12px; color: var(--text-tertiary); line-height: 1.3; }
  .num-input, .select-input { height: 32px; padding: 0 10px; border: 1px solid var(--border); border-radius: var(--radius); background: var(--surface); color: var(--text); font-size: 13px; }
  .num-input { width: 80px; text-align: right; }
  .select-input { width: 132px; }
  .num-input:focus, .select-input:focus { outline: 0; border-color: var(--accent); box-shadow: 0 0 0 3px var(--accent-ring); }
</style>

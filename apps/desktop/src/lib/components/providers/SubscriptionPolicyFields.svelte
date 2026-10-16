<script lang="ts">
  import { onMount } from "svelte";
  import { Banner, Button, SelectField, SwitchField } from "@aipass/ui";
  import { t } from "../../stores/i18n";
  import type { Options } from "./ProviderRuntimeDialog.svelte";
  export let id: string;
  export let providerId: string;
  export let policy: Options["subscription"];
  export let busy = false;
  export let dirty = false;
  export let invokeTauri: <T>(command: string, args?: Record<string, unknown>) => Promise<T>;
  export let onChanged: () => void;
  export let onSaved: () => void;
  let routes: { id: string; name: string }[] = [];
  let resetting = false;
  let error = "";
  let notice = "";
  let current = true;
  onMount(() => {
    void invokeTauri<{ routes: { id: string; name: string; enabled: boolean; targets: { providerEntryId: string; enabled: boolean }[] }[] }>("server_config_get")
      .then(config => { if (current) routes = config.routes.filter(r => r.enabled && r.targets.some(t => t.enabled && t.providerEntryId === id)); })
      .catch(err => { if (current) error = String(err); });
    return () => { current = false; };
  });
  async function reset() {
    if (resetting || busy || dirty) return;
    resetting = true; busy = true; error = ""; notice = "";
    try {
      const value = await invokeTauri<{ outcome: string }>("subscription_reset", { id });
      if (current) { notice = $t(`subscriptionPolicy.resetOutcome.${value.outcome}`); onSaved(); }
    } catch (err) { if (current) error = String(err); }
    finally { if (current) { resetting = false; busy = false; } }
  }
</script>

<div class="subscription-policy">
  {#if error}<Banner tone="danger">{error}</Banner>{/if}
  {#if notice}<Banner tone="success">{notice}</Banner>{/if}
  {#if providerId === "codex"}
    <SwitchField label={$t("subscriptionPolicy.paidCredits")} description={$t("subscriptionPolicy.paidCreditsHint")}
      checked={policy.allowPaidCredits} disabled={busy || resetting}
      onCheckedChange={value => { policy.allowPaidCredits = value; onChanged(); }} />
    <SwitchField label={$t("subscriptionPolicy.autoReset")} checked={policy.autoResetCredits} disabled={busy || resetting}
      onCheckedChange={value => { policy.autoResetCredits = value; onChanged(); }} />
    <div><Button variant="secondary" size="sm" loading={resetting} disabled={busy || dirty} on:click={reset}>{$t("subscriptionPolicy.resetNow")}</Button></div>
  {/if}
  <SwitchField label={$t("subscriptionPolicy.nativeSwitch")} description={$t("subscriptionPolicy.nativeSwitchHint")}
    checked={Boolean(policy.nativeSwitch)} disabled={busy || resetting || !routes.length}
    onCheckedChange={value => { policy.nativeSwitch = value ? { routeId: routes[0].id, returnPreferred: true } : null; onChanged(); }} />
  {#if policy.nativeSwitch}
    <SelectField label={$t("subscriptionPolicy.group")} value={policy.nativeSwitch.routeId} disabled={busy || resetting}
      options={routes.map(r => ({ value: r.id, label: r.name }))}
      onValueChange={value => { policy.nativeSwitch!.routeId = value; onChanged(); }} />
    <SwitchField label={$t("subscriptionPolicy.returnPreferred")} checked={policy.nativeSwitch.returnPreferred} disabled={busy || resetting}
      onCheckedChange={value => { policy.nativeSwitch!.returnPreferred = value; onChanged(); }} />
  {/if}
</div>

<style>
  .subscription-policy { display: grid; gap: 14px; padding: 18px; border: 1px solid var(--border); border-radius: var(--radius-lg); background: var(--surface); }
</style>

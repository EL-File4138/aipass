<script lang="ts">
  import { subscriptionAccountPresentation } from "@aipass/schemas";
  import type { ProviderEntry } from "@aipass/schemas";
  import { Card } from "@aipass/ui";
  import { t } from "../../stores/i18n";

  export let entry: ProviderEntry;
  $: account = subscriptionAccountPresentation(entry);
</script>

<Card title={$t("providerDetail.account")} padded={false}>
  <div class="subscription-account">
    <strong>{account.account || entry.title}</strong>
    {#if account.scope}<small>{$t("subscriptionUsage.workspace", { workspace: account.scope.slice(-8) })}</small>{/if}
    {#if entry.accountIdentity && entry.accountIdentity !== account.account}<details><summary>{$t("subscriptionUsage.accountId")}</summary><code>{entry.accountIdentity}</code></details>{/if}
  </div>
</Card>

<style>
  .subscription-account {
    padding: 12px 14px;
    color: var(--text);
    font-size: 13px;
    display: grid; gap: 6px;
    overflow-wrap: anywhere;
  }
  small, details { color: var(--text-tertiary); font-size: 11px; }
  summary { cursor: pointer; }
  code { display: block; margin-top: 6px; }
</style>

<script lang="ts">
  import { Field } from "@aipass/ui";
  import { Trash2 } from "lucide-svelte";
  import { t } from "../../stores/i18n";
  import type { ModelPricing } from "../../types";
  export let pricing: ModelPricing;
  export let onChange: (patch: Partial<ModelPricing>) => void;
  export let onNumber: (key: keyof Omit<ModelPricing, "model">, input: HTMLInputElement) => void;
  export let onRemove: () => void;
</script>

                  <div class="pricing-row">
                    <div class="pricing-model-row">
                      <Field label={$t("settings.modelPrefix")}><input value={pricing.model} on:change={(event) => onChange( { model: event.currentTarget.value.trim() })} /></Field>
                      <button type="button" class="pricing-remove" title={$t("settings.removePricing")} aria-label={$t("settings.removePricing")} on:click={() => onRemove()}><Trash2 size={14} /></button>
                    </div>
                    <div class="pricing-price-grid">
                      <Field label={$t("settings.inputPrice")}><input type="number" min="0" value={pricing.inputMicrosPerMillion} on:change={(event) => onNumber( "inputMicrosPerMillion", event.currentTarget)} /></Field>
                      <Field label={$t("settings.outputPrice")}><input type="number" min="0" value={pricing.outputMicrosPerMillion} on:change={(event) => onNumber( "outputMicrosPerMillion", event.currentTarget)} /></Field>
                      <Field label={$t("settings.cacheReadPrice")}><input type="number" min="0" value={pricing.cacheReadMicrosPerMillion} on:change={(event) => onNumber( "cacheReadMicrosPerMillion", event.currentTarget)} /></Field>
                      <Field label={$t("settings.cacheCreationPrice")}><input type="number" min="0" value={pricing.cacheCreationMicrosPerMillion} on:change={(event) => onNumber( "cacheCreationMicrosPerMillion", event.currentTarget)} /></Field>
                      <Field label={$t("settings.cacheCreation1hPrice")}><input type="number" min="0" value={pricing.cacheCreation1hMicrosPerMillion ?? ""} on:change={(event) => onNumber("cacheCreation1hMicrosPerMillion", event.currentTarget)} /></Field>
                    </div>
                  </div>
<style>
  .pricing-row {
    display: grid;
    gap: 12px;
    padding: 12px;
    border: 1px solid var(--divider);
    border-radius: var(--radius);
  }

  .pricing-model-row {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 28px;
    align-items: end;
    gap: 8px;
  }

  .pricing-price-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px 12px;
    align-items: start;
  }

  .pricing-remove {
    display: grid;
    place-items: center;
    width: 28px;
    height: 34px;
    color: var(--text-tertiary);
    border-radius: var(--radius-sm);
  }

  .pricing-remove:hover {
    color: var(--danger);
    background: color-mix(in oklab, var(--danger) 8%, transparent);
  }

</style>

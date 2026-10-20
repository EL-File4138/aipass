<script lang="ts">
  import { Select } from "bits-ui";
  import { Check, ChevronDown } from "lucide-svelte";

  import { t } from "../i18n";
  import { scrollMask } from "../actions/scrollMask";

  export let value = "";
  export let label = "";
  export let ariaLabel = "";
  export let placeholder = "";
  export let disabled = false;
  export let options: Array<{ value: string; label: string; disabled?: boolean }> = [];
  export let onValueChange: (value: string) => void = () => {};

  $: selectedLabel = options.find((option) => option.value === value)?.label ?? "";
</script>

<div class="select-field">
  {#if label}
    <span class="select-label">{label}</span>
  {/if}
  <Select.Root
    type="single"
    {value}
    onValueChange={(next) => {
      value = next;
      onValueChange(next);
    }}
    {disabled}
    items={options.map((option) => ({ value: option.value, label: option.label, disabled: option.disabled }))}
  >
    <Select.Trigger class="select-trigger" aria-label={ariaLabel || label || placeholder}>
      <span class="select-value" class:placeholder={!selectedLabel}>
        {selectedLabel || placeholder || $t("common.select")}
      </span>
      <ChevronDown size={14} />
    </Select.Trigger>
    <Select.Portal>
      <Select.Content class="select-content select-menu" align="start" sideOffset={4} collisionPadding={8}>
        {#snippet child({ props, wrapperProps })}
          <!-- The positioned wrapper creates the stacking context, so its layer
               must sit above dialog content (z-index 201). -->
          <div {...wrapperProps} style:z-index="220">
            <div {...props}>
              <Select.Viewport class="select-viewport select-menu-viewport">
                {#snippet child({ props })}
                  <div {...props} use:scrollMask>
                    {#if options.length === 0}
                      <div class="select-empty" role="status">{$t("common.noOptions")}</div>
                    {/if}
                    {#each options as option}
                      <Select.Item
                        class="select-item select-menu-item"
                        value={option.value}
                        label={option.label}
                        disabled={option.disabled}
                      >
                        {#snippet children({ selected })}
                          <span class="select-item-text" title={option.label}>{option.label}</span>
                          <span class="select-menu-indicator" aria-hidden="true">
                            {#if selected}<Check size={14} />{/if}
                          </span>
                        {/snippet}
                      </Select.Item>
                    {/each}
                  </div>
                {/snippet}
              </Select.Viewport>
            </div>
          </div>
        {/snippet}
      </Select.Content>
    </Select.Portal>
  </Select.Root>
</div>

<style lang="scss">
  .select-field {
    display: grid;
    grid-template-columns: minmax(0, 1fr);
    gap: 6px;
    min-width: 0;
  }

  .select-label {
    color: var(--text-secondary);
    font-size: 12px;
    font-weight: 500;
  }

  :global(.select-trigger) {
    display: inline-flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    min-width: 0;
    min-height: 34px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    font-size: 13px;
    cursor: pointer;
    transition: border-color 120ms ease, background-color 120ms ease;
  }

  :global(.select-trigger:hover:not([data-disabled])) {
    border-color: var(--border-strong);
  }

  :global(.select-trigger[data-state="open"]),
  :global(.select-trigger:focus-visible) {
    outline: 0;
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-ring);
  }

  :global(.select-trigger[data-disabled]) {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .select-value {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    text-align: left;
  }

  .select-value.placeholder {
    color: var(--text-tertiary);
  }

  .select-empty { padding: 10px; color: var(--text-tertiary); font-size: 12px; }

  .select-item-text {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>

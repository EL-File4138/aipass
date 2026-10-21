<script lang="ts">
  import { Button, Modal, Field, SelectField, SwitchField, IconButton } from "@aipass/ui";
  import { ArrowDown, ArrowUp, Copy, Plus, Trash2 } from "lucide-svelte";
  import type { ProviderEntry } from "@aipass/schemas";
  import type { MaybePromise, ProxyConfig, SearchConfig, SearchApiConfig, SearchVendor } from "../../types";
  import { t } from "../../stores/i18n";
  import { advertisedProxyAddress } from "../../utils/server";

  export let open = false;
  export let config: ProxyConfig;
  export let entries: ProviderEntry[] = [];
  export let onOpenChange: (open: boolean) => void = () => {};
  export let onSave: (config: ProxyConfig) => MaybePromise<boolean | void> = () => {};
  export let onCopy: (text: string) => MaybePromise = () => {};
  let vendors: {value: SearchVendor; label: string; base: string}[];
  $: vendors = [
    {value:"tavily",label:"Tavily",base:"https://api.tavily.com"},
    {value:"brave",label:"Brave",base:"https://api.search.brave.com"},
    {value:"exa",label:"Exa",base:"https://api.exa.ai"},
    {value:"firecrawl",label:"Firecrawl",base:"https://api.firecrawl.dev"},
    {value:"you",label:"You.com",base:"https://ydc-index.io"},
    {value:"deepseek",label:"DeepSeek",base:"https://api.deepseek.com"},
    {value:"searxng",label:"SearXNG",base:""},
    {value:"bocha",label:"Bocha",base:"https://api.bocha.cn"},
    {value:"yuanbao",label:$t("search.yuanbao"),base:"https://api.wsa.cloud.tencent.com"}
  ];
  let draft: SearchConfig = {apis: [], apiFirst: false};
  let routeId = "";
  let model = "";
  let saving = false;
  let error = "";
  let wasOpen = false;
  $: {
    if (open && !wasOpen) {
    draft = structuredClone(config.search ?? {apis: [], apiFirst: false});
    routeId = draft.model?.routeId ?? "";
    model = draft.model?.model ?? "";
    error = "";
    }
    wasOpen = open;
  }
  $: credentialOptions = entries.filter(e => e.credentialKind === "api" && !e.deletedAt && !e.archivedAt).flatMap(e => e.secretRefs.map(s => ({value: `${e.id}/${s.id}`, label: `${e.title} / ${s.label || s.masked}`})));
  $: modelRoutes = [{value: "", label: $t("search.noModel")}, ...config.routes.filter(r => r.enabled).map(r => ({value:r.id,label:r.name}))];
  $: mcpUrl = `http://${advertisedProxyAddress(config.bindAddr)}/mcp/aipass/web-search`;
  function add() { draft = {...draft, apis: [...draft.apis, {id:crypto.randomUUID(), vendor:"tavily", enabled:true}]}; }
  function move(index: number, delta: number) {
    const apis = [...draft.apis];
    [apis[index], apis[index + delta]] = [apis[index + delta], apis[index]];
    draft = {...draft, apis};
  }
  function credential(api: SearchApiConfig, value: string) {
    const [providerEntryId, secretId] = value.split("/");
    api.providerEntryId = providerEntryId || undefined;
    api.secretId = secretId || undefined;
    draft = {...draft};
  }
  function vendor(api: SearchApiConfig, value: string) {
    if (api.vendor === value) return;
    api.vendor = value as SearchVendor;
    api.providerEntryId = undefined;
    api.secretId = undefined;
    api.baseUrl = undefined;
    draft = {...draft};
  }
  async function save() {
    if (saving) return;
    const apis = draft.apis.map(api => ({...api, baseUrl: api.baseUrl?.trim() || undefined}));
    if (apis.some(api => (api.vendor !== "searxng" && !api.providerEntryId) || (api.vendor === "searxng" && !api.baseUrl)) || (routeId && !model.trim())) {
      error = $t("search.incomplete"); return;
    }
    saving = true; error = "";
    try {
      const result = await onSave({...config, search: {...draft, apis, model: routeId ? {routeId, model:model.trim()} : undefined}});
      if (result !== false) onOpenChange(false);
      else error = $t("search.saveFailed");
    } catch { error = $t("search.saveFailed"); }
    finally { saving = false; }
  }
</script>

<Modal {open} title={$t("search.title")} size="lg" busy={saving} {onOpenChange}>
  <div class="search-settings">
    <div class="model-row">
      <SelectField label={$t("search.modelRoute")} bind:value={routeId} options={modelRoutes} disabled={saving} />
      <Field label={$t("search.model")}><input bind:value={model} disabled={saving || !routeId} placeholder="claude-sonnet-5-5" spellcheck="false" /></Field>
    </div>
    <p class="hint">{$t("search.modelHint")}</p>
    <div class="section-row"><strong>{$t("search.apis")}</strong><Button size="sm" variant="secondary" on:click={add} disabled={saving || draft.apis.length >= 32}><Plus size={14} />{$t("search.add")}</Button></div>
    {#if draft.apis.length === 0}<p class="hint">{$t("search.empty")}</p>{/if}
    {#each draft.apis as api, index (api.id)}
      <div class="api-row">
        <div class="api-fields">
          <SelectField ariaLabel={$t("search.vendor")} value={api.vendor} onValueChange={value => vendor(api, value)} options={vendors} disabled={saving} />
          <SelectField ariaLabel={$t("search.credential")} value={api.providerEntryId ? `${api.providerEntryId}/${api.secretId}` : ""} placeholder={$t("search.credential")} options={[...(api.vendor === "searxng" ? [{value:"",label:$t("search.noKey")}] : []), ...credentialOptions]} onValueChange={value => credential(api, value)} disabled={saving} />
        </div>
        <div class="api-actions">
          <label class="toggle"><input type="checkbox" bind:checked={api.enabled} disabled={saving} aria-label={$t("search.enabled")} /></label>
          <IconButton size="sm" label={$t("search.moveUp")} disabled={saving || index === 0} on:click={() => move(index,-1)}><ArrowUp size={14} /></IconButton>
          <IconButton size="sm" label={$t("search.moveDown")} disabled={saving || index === draft.apis.length-1} on:click={() => move(index,1)}><ArrowDown size={14} /></IconButton>
          <IconButton size="sm" label={$t("common.remove")} disabled={saving} on:click={() => draft = {...draft, apis:draft.apis.filter(a => a.id !== api.id)}}><Trash2 size={14} /></IconButton>
        </div>
        <input class="endpoint" aria-label={$t("search.endpoint")} bind:value={api.baseUrl} placeholder={vendors.find(v => v.value === api.vendor)?.base || $t("search.endpoint")} spellcheck="false" disabled={saving} />
      </div>
    {/each}
    <SwitchField label={$t("search.apiFirst")} bind:checked={draft.apiFirst} disabled={saving} />
    <div class="mcp-row"><div><strong>MCP</strong><code>{mcpUrl}</code><span class="hint">{$t("search.mcpAuth")}</span></div><IconButton size="sm" label={$t("common.copy")} on:click={() => onCopy(mcpUrl)}><Copy size={14} /></IconButton></div>
    {#if error}<p class="error" role="alert">{error}</p>{/if}
  </div>
  <svelte:fragment slot="footer"><Button variant="ghost" disabled={saving} on:click={() => onOpenChange(false)}>{$t("common.cancel")}</Button><Button variant="primary" loading={saving} on:click={save}>{$t("common.save")}</Button></svelte:fragment>
</Modal>

<style lang="scss">
  .search-settings { display:grid; gap:12px; min-width:0; }
  .model-row, .api-fields { display:grid; grid-template-columns:1fr 1fr; gap:10px; min-width:0; }
  .section-row, .api-actions, .mcp-row { display:flex; align-items:center; justify-content:space-between; gap:8px; }
  strong { font-size:12px; font-weight:600; }
  .hint { margin:0; font-size:12px; color:var(--text-secondary); }
  .api-row { display:grid; grid-template-columns:minmax(0,1fr) auto; gap:8px; padding:10px; border:1px solid var(--border); border-radius:var(--radius); }
  .endpoint { grid-column:1 / -1; }
  input:not([type="checkbox"]) { width:100%; min-width:0; box-sizing:border-box; padding:7px 10px; border:1px solid var(--border); border-radius:var(--radius); background:var(--surface); color:var(--text); font:inherit; font-size:12px; }
  .toggle { display:flex; align-items:center; }
  .mcp-row { border-top:1px solid var(--divider); padding-top:12px; }
  .mcp-row > div { display:grid; gap:5px; min-width:0; }
  code { font-size:12px; overflow-wrap:anywhere; }
  .error { margin:0; color:var(--danger); font-size:12px; }
</style>

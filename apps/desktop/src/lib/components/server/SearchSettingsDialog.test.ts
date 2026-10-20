// @vitest-environment happy-dom
import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import SearchSettingsDialog from "./SearchSettingsDialog.svelte";
import { setLocale } from "../../stores/i18n";
import type { ProxyConfig } from "../../types";
let app: ReturnType<typeof mount> | undefined;
beforeEach(() => { vi.useFakeTimers(); });
afterEach(async () => {
  try {
    if (app) await unmount(app);
    // Bits UI releases the body scroll lock in a deferred callback.
    await vi.runAllTimersAsync();
    expect(document.body.style.overflow).not.toBe("hidden");
  } finally {
    app=undefined;
    vi.useRealTimers();
    document.body.innerHTML="";
  }
});
const config: ProxyConfig={enabled:false,bindAddr:"127.0.0.1:8787",routes:[],pricing:[],upstreamProxy:{mode:"direct"},search:{apis:[{id:"source-one",vendor:"searxng",baseUrl:"https://search.example.com",enabled:true}],apiFirst:true}};
function button(label:string){const button=[...document.body.querySelectorAll("button")].find(b => b.textContent?.trim() === label);expect(button).toBeTruthy();return button!;}
test("saves vault references and preserves the rest of the proxy configuration",async()=>{
  setLocale("en");const onSave=vi.fn().mockResolvedValue(true);const onOpenChange=vi.fn();const target=document.createElement("div");document.body.append(target);
  app=mount(SearchSettingsDialog,{target,props:{open:true,config,onSave,onOpenChange}});flushSync();button("Save").click();await vi.waitFor(()=>{flushSync();expect(button("Save").disabled).toBe(false);});
  expect(onSave).toHaveBeenCalledWith(config);expect(onOpenChange).toHaveBeenCalledWith(false);expect(config.search?.apis).toHaveLength(1);
});
test("keeps failed saves open and rejects incomplete search sources",async()=>{
  setLocale("en");const onSave=vi.fn().mockResolvedValue(false);const onOpenChange=vi.fn();const target=document.createElement("div");document.body.append(target);
  app=mount(SearchSettingsDialog,{target,props:{open:true,config,onSave,onOpenChange}});flushSync();button("Save").click();await vi.waitFor(()=>{flushSync();expect(button("Save").disabled).toBe(false);});expect(onOpenChange).not.toHaveBeenCalled();expect(document.body.textContent).toContain("Could not save");
  button("Add").click();flushSync();button("Save").click();await vi.waitFor(()=>{flushSync();expect(button("Save").disabled).toBe(false);});expect(onSave).toHaveBeenCalledTimes(1);expect(document.body.textContent).toContain("Select credentials");
});
test("changing vendor clears the previous endpoint and credential",async()=>{
  setLocale("en");const onSave=vi.fn().mockResolvedValue(true);const target=document.createElement("div");document.body.append(target);
  const saved:ProxyConfig={...config,search:{apis:[{id:"one",vendor:"tavily",baseUrl:"https://old.example.com",providerEntryId:"old-entry",secretId:"old-key",enabled:true}],apiFirst:true}};
  app=mount(SearchSettingsDialog,{target,props:{open:true,config:saved,onSave}});flushSync();
  document.querySelector<HTMLButtonElement>('[aria-label="Search service"]')!.dispatchEvent(new PointerEvent("pointerdown",{bubbles:true,button:0,pointerType:"mouse"}));
  await vi.waitFor(()=>expect(document.querySelector('[role="option"][data-value="searxng"]')).toBeTruthy());
  document.querySelector<HTMLElement>('[role="option"][data-value="searxng"]')!.dispatchEvent(new PointerEvent("pointerup",{bubbles:true,button:0,pointerType:"mouse"}));flushSync();
  const endpoint=document.querySelector<HTMLInputElement>('.endpoint')!;expect(endpoint.value).toBe("");endpoint.value="https://new.example.com";endpoint.dispatchEvent(new Event("input",{bubbles:true}));flushSync();
  button("Save").click();await vi.waitFor(()=>expect(onSave).toHaveBeenCalledOnce());
  expect(onSave.mock.calls[0][0].search.apis[0]).toEqual({id:"one",vendor:"searxng",baseUrl:"https://new.example.com",enabled:true});
  expect(saved.search?.apis[0].secretId).toBe("old-key");
});
test("only one save runs while its result is pending",async()=>{
  setLocale("en");let finish!:(value:boolean)=>void;const onSave=vi.fn(()=>new Promise<boolean>(resolve=>{finish=resolve;}));const target=document.createElement("div");document.body.append(target);
  app=mount(SearchSettingsDialog,{target,props:{open:true,config,onSave}});flushSync();
  button("Save").click();button("Save").click();expect(onSave).toHaveBeenCalledOnce();finish(true);await vi.waitFor(()=>expect(button("Save").disabled).toBe(false));
});

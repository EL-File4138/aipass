import { flushSync, mount, unmount } from "svelte";
import { fromStore, writable } from "svelte/store";
import { afterEach, expect, test, vi } from "vitest";
import type { ProviderEntry } from "@aipass/schemas";
import { setLocale } from "../../stores/i18n";
import AgentManagementPage from "./AgentManagementPage.svelte";
const entry: ProviderEntry = { id: "api", title: "Gateway", favorite: false, providerKind: "third_party", credentialKind: "api", interfaceType: "openai_compatible", authScheme: "bearer", defaultModel: "test-model", domains: [], endpoints: [], tags: [], secretRefs: [{ id: "key", label: "Primary", masked: "••••", fingerprint: "fixture" }] };
const preview = { tool: "codex", mode: "plaintext", entryId: "api", entryTitle: "Gateway", targetPath: "/fixture/.codex/config.toml", summary: "Switch", preview: "+ credentials [redacted]", previewId: "snapshot" };
let app: ReturnType<typeof mount> | undefined;
afterEach(async () => { if (app) await unmount(app); app = undefined; await new Promise(r => setTimeout(r, 30)); document.body.innerHTML = ""; });
function invokeFixture(outcome = "applied") {
  return vi.fn(async (command: string) => {
    if (command === "tool_config_detect") return [{ tool: "codex", binaryFound: true }, { tool: "pi", binaryFound: false, configPath: "/leftover/.pi" }];
    if (command === "tool_config_status") return { tool: "codex", state: "ready", entryId: "api", secretId: "key", entryTitle: "Gateway", overrides: [] };
    if (command === "tool_config_preview") return preview;
    if (command === "agent_management_apply") return { ...preview, outcome, message: outcome === "applied" ? undefined : "Configuration changed externally", operationId: "op", backupPath: "/encrypted" };
    return null;
  });
}
async function settled() { await vi.waitFor(() => { flushSync(); expect(document.querySelector<HTMLButtonElement>(".switch-controls .btn")?.disabled).toBe(false); }); }
async function prepare() {
  document.querySelector<HTMLButtonElement>(".switch-controls .btn")!.click();
  await vi.waitFor(() => { flushSync(); expect(document.querySelector(".dialog-actions .btn-primary")).toBeTruthy(); });
}
test("installed Agents switch through the import gate only after preview confirmation", async () => {
  setLocale("en"); const invoke = invokeFixture(); const refresh = vi.fn(async () => {});
  app = mount(AgentManagementPage, { target: document.body, props: { entries: [entry], invokeTauri: invoke as never, onRefreshEntries: refresh } });
  await settled(); expect(document.querySelectorAll(".agent-row")).toHaveLength(1); expect(document.body.textContent).toContain("Codex");
  await prepare(); expect(document.body.textContent).toContain("imported before switching");
  expect(invoke.mock.calls.some(([command]) => command === "agent_management_apply")).toBe(false);
  document.querySelector<HTMLButtonElement>(".dialog-actions .btn-primary")!.click();
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("agent_management_apply", { request: { tool: "codex", id: "api", secretId: "key", mode: "plaintext", codexApiKeyMode: "auth_json", previewId: "snapshot" } }));
  await vi.waitFor(() => { flushSync(); expect(document.body.textContent).toContain("Credentials switched"); });
  expect(refresh).toHaveBeenCalledTimes(1); expect(invoke.mock.calls.some(([command]) => command === "tool_config_apply")).toBe(false);
});
test("a failed switch stays visible after status refresh and cancellation performs no write", async () => {
  setLocale("en"); const invoke = invokeFixture("conflict");
  app = mount(AgentManagementPage, { target: document.body, props: { entries: [entry], invokeTauri: invoke as never, onRefreshEntries: async () => {} } });
  await settled(); await prepare();
  document.querySelector<HTMLButtonElement>(".dialog-actions .btn-ghost")!.click();
  await vi.waitFor(() => { flushSync(); expect(document.querySelector(".preview-dialog-content")).toBeNull(); });
  expect(invoke.mock.calls.some(([command]) => command === "agent_management_apply")).toBe(false);
  await prepare(); document.querySelector<HTMLButtonElement>(".dialog-actions .btn-primary")!.click();
  await vi.waitFor(() => expect(invoke.mock.calls.filter(([command]) => command === "tool_config_status")).toHaveLength(2));
  flushSync(); expect(document.body.textContent).toContain("Configuration changed externally"); expect(document.body.textContent).not.toContain("Credentials switched");
});
test("credential edits while previewing invalidate the pending selection", async () => {
  setLocale("en"); const entries = writable([entry]); const state = fromStore(entries); const invoke = invokeFixture();
  app = mount(AgentManagementPage, { target: document.body, props: { get entries() { return state.current; }, invokeTauri: invoke as never, onRefreshEntries: async () => {} } });
  await settled(); await prepare(); entries.set([{ ...entry, title: "Edited" }]);
  await vi.waitFor(() => { flushSync(); expect(document.querySelector(".preview-dialog-content")).toBeNull(); });
  expect(invoke.mock.calls.some(([command]) => command === "agent_management_apply")).toBe(false);
});

test("the detail shows only the active account usage until a switch is applied", async () => {
  setLocale("en");
  const active = {...entry, quota: {remaining: "87", unit: "credits"}};
  const other = {...entry, id: "other", title: "Other account", quota: {remaining: "999", unit: "credits"}};
  const invoke = vi.fn(async (command: string) => {
    if (command === "server_usage_summary") return {providers: [
      {providerEntryId: "api", secretId: "key", requestCount: 3, inputTokens: 120, outputTokens: 40, cacheReadTokens: 8, cacheCreationTokens: 2},
      {providerEntryId: "other", secretId: "key", requestCount: 100, inputTokens: 50000, outputTokens: 50000, cacheReadTokens: 0, cacheCreationTokens: 0}
    ]};
    return invokeFixture()(command);
  });
  app = mount(AgentManagementPage, {target: document.body, props: {entries: [active, other], invokeTauri: invoke as never, onRefreshEntries: async () => {}}});
  await settled();
  await vi.waitFor(() => { flushSync(); expect(document.querySelector(".account-detail")?.textContent).toContain("120"); });
  const trigger = document.querySelector<HTMLButtonElement>('.switch-controls .select-trigger')!;
  trigger.dispatchEvent(new PointerEvent("pointerdown", {bubbles: true, button: 0, pointerType: "mouse"}));
  await vi.waitFor(() => { flushSync(); expect(document.querySelector('[role="option"][data-value="other:key"]')).toBeTruthy(); });
  document.querySelector<HTMLElement>('[role="option"][data-value="other:key"]')!.dispatchEvent(new PointerEvent("pointerup", {bubbles: true, button: 0, pointerType: "mouse"}));
  flushSync();
  expect(document.querySelector(".account-detail")?.textContent).toContain("87");
  expect(document.querySelector(".account-detail")?.textContent).not.toContain("999");
  expect(document.querySelector(".account-detail")?.textContent).not.toContain("50K");
  expect(invoke.mock.calls.some(([command]) => command === "agent_management_apply")).toBe(false);
});

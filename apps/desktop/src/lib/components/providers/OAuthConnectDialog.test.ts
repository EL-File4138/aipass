// @vitest-environment happy-dom
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";
import OAuthConnectDialog from "./OAuthConnectDialog.svelte";
import allSubscriptions from "../../../../../../crates/aipass-agent/src/subscriptions/catalog.json";
let app: Record<string, unknown> | undefined;
afterEach(async () => { if (app) { await unmount(app as never); await new Promise(r => setTimeout(r, 30)); } app = undefined; document.body.innerHTML = ""; vi.restoreAllMocks(); });
const catalog = [{ id: "codex", name: "ChatGPT (Codex)", methods: [{index: 0, type: "oauth", label: "Sign in with official Codex CLI", native: false, prompts: []}, {index: 1, type: "oauth", label: "Connect existing CLI account", native: true, prompts: []}] }];
function render(invokeTauri: ReturnType<typeof vi.fn>) { const target = document.createElement("div"); document.body.append(target); app = mount(OAuthConnectDialog, {target, props: {invokeTauri: invokeTauri as never}}) as never; flushSync(); }
function click(label: string) { const b = [...document.querySelectorAll("button")].find(b => b.querySelector("strong")?.textContent === label || b.textContent?.trim() === label); expect(b).toBeTruthy(); b!.click(); flushSync(); }
async function settle() { await new Promise(r => setTimeout(r, 20)); flushSync(); }
test("offers searchable subscriptions directly from the Agent catalog", async () => {
 const invoke = vi.fn(async command => command === "community_catalog" ? allSubscriptions : undefined); render(invoke); await settle();
 const titles = [...document.querySelectorAll("strong")].map(n => n.textContent);
 expect(titles).toContain("OpenCode Go"); expect(titles).toContain("Trae (China)"); expect(titles).toContain("Volcengine Coding Plan");
 expect(document.querySelectorAll(".provider-icon")).toHaveLength(allSubscriptions.length + 1);
 expect(document.querySelectorAll(".provider-icon .initials")).toHaveLength(0);
 const search = document.querySelector<HTMLInputElement>('input[aria-label="Search providers"]')!;
 search.value = "qwen"; search.dispatchEvent(new Event("input", {bubbles:true})); flushSync();
 expect(document.querySelectorAll(".provider")).toHaveLength(1);
 expect(document.querySelector(".provider strong")?.textContent).toBe("Qwen Token Plan");
 expect(invoke).toHaveBeenCalledWith("community_catalog");
});
test("catalog loading and failure remain visible until a successful retry", async () => {
 let reject!: (reason: Error) => void;
 const first = new Promise((_resolve, fail) => { reject = fail; });
 const invoke = vi.fn().mockImplementationOnce(() => first).mockResolvedValue(allSubscriptions);
 render(invoke);
 expect(document.querySelector('[role="status"]')?.textContent).toContain("Loading");
 reject(new Error("fixture unavailable")); await settle();
 expect(document.querySelector('[role="alert"]')?.textContent).toBe("Could not load providers");
 click("Retry"); await settle();
 expect(document.querySelector('[role="alert"]')).toBeNull();
 expect(document.querySelectorAll(".provider")).toHaveLength(allSubscriptions.length + 1);
});
test("checks Codex CLI and blocks sign-in until installation is available", async () => {
 let available = false;
 const invoke = vi.fn(async command => command === "community_catalog" ? catalog : command === "subscription_cli_status" ? {available, reason: "missing"} : undefined);
 render(invoke); await settle(); click("ChatGPT (Codex)"); await settle();
 expect(invoke).toHaveBeenCalledWith("subscription_cli_status", {provider: "codex"});
 expect(document.body.textContent).toContain("Official CLI not found");
 expect(document.querySelectorAll(".provider-icon .initials")).toHaveLength(0);
 const next = [...document.querySelectorAll("button")].find(b => b.textContent?.trim() === "Continue"); expect(next?.disabled).toBe(true);
 click("Installation guide"); await settle(); expect(invoke).toHaveBeenCalledWith("subscription_open_install", {provider: "codex"});
 available = true; click("Check again"); await settle(); expect(next?.disabled).toBe(false);
 expect(invoke.mock.calls.some(([c]) => c.startsWith("oauth_"))).toBe(false);
});
test("starts official browser CLI login through the subscription coordinator", async () => {
 const invoke = vi.fn(async command => command === "community_catalog" ? catalog : command === "subscription_cli_status" ? {available: true} : {ticket: "t", status: "pending"});
 render(invoke); await settle(); click("ChatGPT (Codex)"); await settle(); click("Continue"); await settle();
 expect(invoke).toHaveBeenCalledWith("community_login_start", {input: {provider: "codex", method: 0, inputs: {}, apiKey: null}});
 expect(invoke.mock.calls.some(([c]) => c.startsWith("oauth_"))).toBe(false);
});

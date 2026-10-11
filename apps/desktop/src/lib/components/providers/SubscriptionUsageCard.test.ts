import { flushSync, mount, unmount, type ComponentProps } from "svelte";
import { afterEach, expect, test, vi } from "vitest";
import { setLocale } from "../../stores/i18n";
import SubscriptionUsageCard from "./SubscriptionUsageCard.svelte";
import { readableAccountIdentity, subscriptionAccountPresentation } from "@aipass/schemas";

let app: ReturnType<typeof mount>;
afterEach(async () => { if (app) await unmount(app); document.body.innerHTML = ""; setLocale("en"); });
function render(props: Partial<ComponentProps<typeof SubscriptionUsageCard>> = {}) {
  setLocale("zh-CN");
  app = mount(SubscriptionUsageCard, { target: document.body, props }); flushSync();
}
const snapshot = { source: "community:codex", observedAt: "2026-10-11T14:00:00Z", plan: "pro", windows: [
  { id: "five", label: "codex · primary", usedPercent: 29, windowMinutes: 300 },
  { id: "week", label: "codex · secondary", usedPercent: 0, windowMinutes: 10080 },
  { id: "spark", label: "spark · primary", usedPercent: 150, windowMinutes: 300 }
] };

test("period bars preserve zero readings, clamp overflow and keep model scope", () => {
  render({ subscription: snapshot });
  const bars = [...document.querySelectorAll('[role="progressbar"]')];
  expect(bars.map(b => b.getAttribute("aria-label"))).toEqual(["5 小时限额", "周限额", "spark (5 小时限额)"]);
  expect(bars.map(b => b.getAttribute("aria-valuenow"))).toEqual(["29", "0", "100"]);
  expect(document.body.textContent).not.toContain("community:codex");
  expect(document.querySelector(".credits")).toBeNull();
});
test("credits are a separate optional balance, including an explicit zero", () => {
  render({ subscription: { ...snapshot, creditsRemaining: "0" } });
  expect(document.querySelector(".credits")?.textContent).toBe("额外 Credits0");
});
test.each([[{}, "尚未读取用量"], [{ refreshing: true }, "正在读取用量"]])("empty and loading states do not fabricate exhausted limits", (props, text) => {
  render(props); expect(document.body.textContent).toContain(text);
  expect(document.querySelector('[role="progressbar"]')).toBeNull();
});
test("ownership failures have a localized recovery message with cached bars", () => {
  render({ subscription: { ...snapshot, error: "ZCode native account changed; reconnect explicitly", stale: true }, refreshable: true });
  expect(document.body.textContent).toContain("需要重新连接账号");
  expect(document.body.textContent).not.toContain("reconnect explicitly");
  expect(document.querySelectorAll('[role="progressbar"]')).toHaveLength(3);
  expect(document.querySelector(".windows.stale")).toBeTruthy();
});
test("unknown failures have a retry button without raw transport text", () => {
  const onRefresh = vi.fn(); render({ error: "private-token-in-error", refreshable: true, onRefresh });
  expect(document.body.textContent).toContain("暂时无法读取用量");
  expect(document.body.textContent).not.toContain("private-token-in-error");
  document.querySelector<HTMLButtonElement>(".usage-footer button")!.click();
  expect(onRefresh).toHaveBeenCalledOnce();
});
test("legacy generated names keep email separate from the complete ownership key", () => {
  const identity = "alice@example.test:workspace-a";
  expect(subscriptionAccountPresentation({ title: `codex · ${identity}`, providerId: "codex", accountIdentity: identity }))
    .toEqual({ title: "Codex", subtitle: "alice@example.test (kspace-a)", account: "alice@example.test", scope: "workspace-a" });
  expect(readableAccountIdentity("f055738b-a707-4b5b-8304-c37dd2525035")).toBe("d2525035");
  expect(subscriptionAccountPresentation({ title: "My work account", providerId: "codex", accountIdentity: identity }).title).toBe("My work account");
});

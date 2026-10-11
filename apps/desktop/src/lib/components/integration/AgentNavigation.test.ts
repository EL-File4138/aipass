import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";
import { VaultSidebar } from "@aipass/ui";
import { setLocale } from "../../stores/i18n";
const providerCounts = { all: 0, recent: 0, favorites: 0, official: 0, third_party: 0, self_hosted: 0, unknown: 0 };
let app: ReturnType<typeof mount>;
afterEach(async () => { if (app) await unmount(app); document.body.innerHTML = ""; });
test("surfaces without Agent management do not show a dead navigation item", () => {
  setLocale("en");
  app = mount(VaultSidebar, { target: document.body, props: { providerCounts } }); flushSync();
  expect([...document.querySelectorAll("button")].find(b => b.textContent?.trim() === "Agent")).toBeUndefined();
});
test("desktop Agent navigation invokes the supplied handler", () => {
  setLocale("en"); const onAgentView = vi.fn();
  app = mount(VaultSidebar, { target: document.body, props: { providerCounts, onAgentView } }); flushSync();
  [...document.querySelectorAll("button")].find(b => b.textContent?.trim() === "Agent")!.click();
  expect(onAgentView).toHaveBeenCalledOnce();
});

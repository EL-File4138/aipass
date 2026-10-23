import { emptyDraft, ProviderFormFields } from "@aipass/ui";
import { flushSync, mount, unmount } from "svelte";
import { expect, test } from "vitest";

test("new entries start with an API address and keep domain overrides optional", async () => {
  const app = mount(ProviderFormFields, {target: document.body, props: {draft: emptyDraft()}});
  flushSync();
  expect(document.querySelector('[data-provider-field="endpoint"]')).toBeTruthy();
  expect(document.querySelector('[data-provider-field="domain"]')).toBeNull();
  expect(document.querySelector('[data-provider-field="quota"]')).toBeNull();
  expect(document.querySelector('.advanced-section .collapsible-trigger')?.getAttribute("aria-expanded")).toBe("false");
  await unmount(app);
});

test("preserves an explicit WebSocket opt-out when reopening the provider form", async () => {
  const draft = { ...emptyDraft(), interfaceType: "openai_compatible" as const };
  let app = mount(ProviderFormFields, { target: document.body, props: { draft, itemLayout: true, formMode: "edit", showWebsocketSetting: true } });
  flushSync();
  document.querySelector<HTMLButtonElement>(".advanced-section .collapsible-trigger")!.click();
  flushSync();
  const toggle = document.querySelector<HTMLButtonElement>("[role=switch]")!;
  expect(toggle.getAttribute("aria-checked")).toBe("true");
  toggle.click();
  flushSync();
  expect(draft.supportsWebsockets).toBe(false);
  expect(toggle.getAttribute("aria-checked")).toBe("false");
  await unmount(app);
  app = mount(ProviderFormFields, { target: document.body, props: { draft, itemLayout: true, formMode: "edit", showWebsocketSetting: true } });
  flushSync();
  expect(document.querySelector("[role=switch]")!.getAttribute("aria-checked")).toBe("false");
  await unmount(app);
});

import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";
import { localizedMessage, setLocale } from "../../stores/i18n";
const { check, install } = vi.hoisted(() => ({ check: vi.fn(), install: vi.fn() }));
vi.mock("../../build", () => ({ buildTimeLabel: () => "2026-10-20 12:00" }));
vi.mock("@tauri-apps/api/app", () => ({ getVersion: async () => "1.0.0-beta.1" }));
vi.mock("../../services/updates", async (original) => ({
  ...await original<object>(), checkForUpdates: check, installUpdate: install,
}));
import SettingsPanel from "./SettingsPanel.svelte";

let app: ReturnType<typeof mount>;
const button = (text: string) => [...document.querySelectorAll<HTMLButtonElement>("button")]
  .find(node => node.textContent?.trim() === text)!;
afterEach(async () => {
  if (app) await unmount(app);
  document.body.innerHTML = "";
});

test("an available update keeps installation errors visible and allows retry", async () => {
  setLocale("en");
  check.mockResolvedValue({ currentVersion: "1.0.0-beta.1", latestVersion: "1.0.0-beta.2", available: true });
  install.mockRejectedValue(localizedMessage("error.updateInstallPermission"));
  app = mount(SettingsPanel, { target: document.body, props: { initialTab: "about", browserExtensionStatus: undefined, ccSwitchDetection: undefined } });
  await vi.waitFor(() => { flushSync(); expect(button("Check now")).toBeTruthy(); expect(document.body.textContent).toContain("1.0.0-beta.1"); });
  button("Check now").click();
  await vi.waitFor(() => { flushSync(); expect(button("Install")).toBeTruthy(); });
  button("Install").click();
  await vi.waitFor(() => {
    flushSync();
    expect(install).toHaveBeenCalled();
    expect([...document.querySelectorAll('[role="alert"]')].some(node => node.textContent?.includes("No permission to install"))).toBe(true);
  });
  expect(button("Install").disabled).toBe(false);
  expect(document.body.textContent).toContain("1.0.0-beta.2");
});


test("auto-lock keeps custom durations and saves the never option once", async () => {
  setLocale("en");
  const save = vi.fn();
  app = mount(SettingsPanel, { target: document.body, props: {
    initialTab: "security", autoLockMinutes: 180, onSavePreferences: save,
    browserExtensionStatus: undefined, ccSwitchDetection: undefined
  } });
  await vi.waitFor(() => { flushSync(); expect(document.querySelector('button[aria-label="Auto-lock"]')).toBeTruthy(); });
  const trigger = document.querySelector<HTMLButtonElement>('button[aria-label="Auto-lock"]')!;
  expect(trigger.textContent).toContain("180");
  trigger.dispatchEvent(new KeyboardEvent("keydown", { key: "ArrowDown", bubbles: true }));
  const option = () => [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(node => node.textContent?.trim() === "Never");
  await vi.waitFor(() => { flushSync(); expect(option()).toBeTruthy(); });
  option()!.dispatchEvent(new PointerEvent("pointerup", { bubbles: true, button: 0, pointerType: "mouse" }));
  await vi.waitFor(() => { flushSync(); expect(trigger.textContent).toContain("Never"); expect(save).toHaveBeenCalledTimes(1); });
});

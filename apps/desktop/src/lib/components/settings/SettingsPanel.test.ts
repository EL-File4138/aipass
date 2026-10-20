import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
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
beforeEach(() => { vi.useFakeTimers(); });
afterEach(async () => {
  try {
    if (app) await unmount(app);
    // Bits UI restores the body scroll lock after a deferred teardown.
    await vi.runAllTimersAsync();
    expect(document.body.style.overflow).not.toBe("hidden");
  } finally {
    vi.useRealTimers();
    document.body.innerHTML = "";
    vi.clearAllMocks();
    setLocale("system");
  }
});

test.each([
  ["error.updateInstallPermission", "No permission to install"],
  ["error.updatesUnavailable", "Updates are unavailable in browser preview"],
] as const)("an available update retains %s and clears it after retry", async (key, message) => {
  setLocale("en");
  check.mockResolvedValue({ currentVersion: "1.0.0-beta.1", latestVersion: "1.0.0-beta.2", available: true });
  install.mockReset().mockRejectedValueOnce(localizedMessage(key)).mockResolvedValue(undefined);
  app = mount(SettingsPanel, { target: document.body, props: { initialTab: "about", browserExtensionStatus: undefined, ccSwitchDetection: undefined } });
  await vi.waitFor(() => { flushSync(); expect(button("Check now")).toBeTruthy(); expect(document.body.textContent).toContain("1.0.0-beta.1"); });
  button("Check now").click();
  await vi.waitFor(() => { flushSync(); expect(button("Install")).toBeTruthy(); });
  button("Install").click();
  await vi.waitFor(() => {
    flushSync();
    expect(install).toHaveBeenCalled();
    expect([...document.querySelectorAll('[role="alert"]')].some(node => node.textContent?.includes(message))).toBe(true);
  });
  expect(button("Install").disabled).toBe(false);
  expect(document.body.textContent).toContain("1.0.0-beta.2");
  button("Install").click();
  await vi.waitFor(() => {
    flushSync();
    expect(install).toHaveBeenCalledTimes(2);
    expect(document.body.textContent).not.toContain(message);
  });
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
  await vi.waitFor(() => { flushSync(); expect(trigger.textContent).toContain("Never"); expect(save).toHaveBeenCalledTimes(1); expect(save).toHaveBeenCalledWith(expect.objectContaining({autoLockMinutes:0})); });
});

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
  vi.clearAllMocks();
  setLocale("system");
});

test("an available update keeps installation errors visible and allows retry", async () => {
  setLocale("en");
  check.mockResolvedValue({ currentVersion: "1.0.0-beta.1", latestVersion: "1.0.0-beta.2", available: true });
  install.mockRejectedValueOnce(localizedMessage("error.updatesUnavailable")).mockResolvedValue(undefined);
  app = mount(SettingsPanel, {
    target: document.body,
    props: { initialTab: "about", browserExtensionStatus: undefined, ccSwitchDetection: undefined }
  });
  await vi.waitFor(() => { flushSync(); expect(button("Check now")).toBeTruthy(); expect(document.body.textContent).toContain("1.0.0-beta.1"); });
  button("Check now").click();
  await vi.waitFor(() => { flushSync(); expect(button("Install")).toBeTruthy(); });
  button("Install").click();
  await vi.waitFor(() => {
    flushSync();
    expect(install).toHaveBeenCalled();
    expect([...document.querySelectorAll('[role="alert"]')].some(node => node.textContent?.includes("Updates are unavailable in browser preview"))).toBe(true);
  });
  expect(button("Install").disabled).toBe(false);
  expect(document.body.textContent).toContain("1.0.0-beta.2");
  button("Install").click();
  await vi.waitFor(() => {
    flushSync();
    expect(install).toHaveBeenCalledTimes(2);
    expect(document.body.textContent).not.toContain("Updates are unavailable in browser preview");
  });
});

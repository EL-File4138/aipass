import { flushSync, mount, unmount } from "svelte";
import { afterEach, beforeEach, expect, test, vi } from "vitest";
import { setLocale } from "../../stores/i18n";
import SyncTargetSettings from "./SyncTargetSettings.svelte";

let app: ReturnType<typeof mount>;
const button = (label: string) => [...document.querySelectorAll<HTMLButtonElement>("button")]
  .find(node => node.textContent?.trim() === label);
beforeEach(() => setLocale("en"));
afterEach(async () => {
  if (app) await unmount(app);
  document.body.innerHTML = "";
});

test("non-Apple builds offer working folder and WebDAV controls without iCloud", () => {
  const save = vi.fn(); const run = vi.fn();
  app = mount(SyncTargetSettings, { target: document.body, props: {
    iCloudSupported: false, syncFolder: "/tmp/linux-sync", onSave: save, onRun: run
  } });
  flushSync();
  expect(button("iCloud")).toBeUndefined();
  expect(button("OneDrive")).toBeDefined();
  button("Save")!.click(); button("Sync now")!.click(); flushSync();
  expect(save).toHaveBeenCalledOnce(); expect(run).toHaveBeenCalledOnce();
  button("WebDAV")!.click(); flushSync();
  expect(document.querySelector('input[placeholder="https://cloud.example/dav"]')).not.toBeNull();
  expect(button("Sync now")!.disabled).toBe(true);
});

test("an existing iCloud selection is explained and cannot start or save on Linux", () => {
  const save = vi.fn(); const run = vi.fn();
  app = mount(SyncTargetSettings, { target: document.body, props: {
    iCloudSupported: false, syncMode: "icloud", syncFolder: "/tmp/linux-sync", onSave: save, onRun: run
  } });
  flushSync();
  expect(document.body.textContent).toContain("iCloud sync requires macOS.");
  expect(button("Save")!.disabled).toBe(true);
  expect(button("Sync now")!.disabled).toBe(true);
  button("Save")!.click(); button("Sync now")!.click();
  expect(save).not.toHaveBeenCalled(); expect(run).not.toHaveBeenCalled();
  button("Local")!.click(); flushSync();
  expect(button("Save")!.disabled).toBe(false);
  expect(button("Sync now")!.disabled).toBe(false);
});

test("macOS retains its CloudKit option and sync action", () => {
  const run = vi.fn();
  app = mount(SyncTargetSettings, { target: document.body, props: { iCloudSupported: true, onRun: run } });
  flushSync(); button("iCloud")!.click(); flushSync();
  expect(document.body.textContent).toContain("private iCloud database");
  expect(document.body.textContent).not.toContain("requires macOS");
  expect(button("Sync now")!.disabled).toBe(false);
  button("Sync now")!.click(); expect(run).toHaveBeenCalledOnce();
});

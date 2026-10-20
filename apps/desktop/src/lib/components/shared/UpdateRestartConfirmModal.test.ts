import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";
import { setLocale } from "../../stores/i18n";
import UpdateRestartConfirmModal from "./UpdateRestartConfirmModal.svelte";

let app: ReturnType<typeof mount>;
afterEach(async () => { if (app) await unmount(app); document.body.innerHTML = ""; });

test("confirmation closes after the parent's install attempt and ignores repeated clicks while busy", async () => {
  setLocale("en");
  let finish: () => void = () => {};
  const pending = new Promise<void>(resolve => { finish = resolve; });
  const confirm = vi.fn(() => pending);
  const changed = vi.fn();
  app = mount(UpdateRestartConfirmModal, { target: document.body, props: { open: true, onConfirm: confirm, onOpenChange: changed } });
  await vi.waitFor(() => { flushSync(); expect(document.querySelectorAll("button")).toHaveLength(2); });
  const buttons = [...document.querySelectorAll<HTMLButtonElement>("button")];
  const update = buttons[1];
  update.click(); flushSync(); update.click(); flushSync();
  expect(confirm).toHaveBeenCalledTimes(1);
  buttons[0].click(); flushSync();
  expect(changed).not.toHaveBeenCalled();
  finish();
  await vi.waitFor(() => { flushSync(); expect(changed).toHaveBeenCalledWith(false); });
  expect(changed).toHaveBeenCalledTimes(1);
});

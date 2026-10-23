// @vitest-environment happy-dom
import { flushSync, mount, unmount } from "svelte";
import { afterEach, expect, test, vi } from "vitest";
import { setLocale } from "../../stores/i18n";
import Dialog from "./CommunityConnectDialog.svelte";
import subscriptionCatalog from "../../../../../../crates/aipass-agent/src/subscriptions/catalog.json";
let app: ReturnType<typeof mount> | undefined;
afterEach(async () => {
  if (app) await unmount(app);
  app = undefined;
  await new Promise((resolve) => setTimeout(resolve, 30));
  document.body.innerHTML = "";
});
const catalog = [
  {
    id: "factory",
    name: "Factory",
    methods: [
      { index: 0, type: "api", label: "API key", native: false, prompts: [] },
    ],
  },
];
const pending = { ticket: "ticket-a", status: "pending", method: "auto" };
function deferred<T>() {
  let resolve!: (v: T) => void;
  const promise = new Promise<T>((r) => {
    resolve = r;
  });
  return { promise, resolve };
}
function button(text: string) {
  return [...document.querySelectorAll<HTMLButtonElement>("button")].find(
    (b) => b.textContent?.trim() === text,
  )!;
}
async function start(invoke: ReturnType<typeof vi.fn>, onConnected = vi.fn()) {
  setLocale("en");
  app = mount(Dialog, {
    target: document.body,
    props: { invokeTauri: invoke as never, onClose: vi.fn(), onConnected },
  });
  await vi.waitFor(() => {
    flushSync();
    expect(document.querySelector("input[type=password]")).toBeTruthy();
  });
  const input = document.querySelector<HTMLInputElement>(
    "input[type=password]",
  )!;
  input.value = "synthetic-fixture-key";
  input.dispatchEvent(new Event("input", { bubbles: true }));
  flushSync();
  button("Continue").click();
  return onConnected;
}
test("closing while login starts cancels the returned ticket without reconnecting", async () => {
  const result = deferred<typeof pending>();
  const invoke = vi.fn(async (command: string) => {
    if (command === "community_catalog") return catalog;
    if (command === "community_login_start") return result.promise;
  });
  const connected = await start(invoke);
  await vi.waitFor(() =>
    expect(invoke.mock.calls.some(([c]) => c === "community_login_start")).toBe(
      true,
    ),
  );
  await unmount(app!);
  app = undefined;
  result.resolve(pending);
  await vi.waitFor(() =>
    expect(invoke).toHaveBeenCalledWith("community_login_cancel", {
      ticket: "ticket-a",
    }),
  );
  expect(connected).not.toHaveBeenCalled();
  expect(invoke.mock.calls.some(([c]) => c === "community_login_poll")).toBe(
    false,
  );
});
test("late polling cannot restore a cancelled login", async () => {
  const first = deferred<typeof pending>();
  let polls = 0;
  const invoke = vi.fn(async (command: string) => {
    if (command === "community_catalog") return catalog;
    if (command === "community_login_start") return pending;
    if (command === "community_login_poll")
      return ++polls === 1
        ? first.promise
        : { ...pending, status: "cancelled" };
  });
  const connected = await start(invoke);
  await vi.waitFor(() => {
    flushSync();
    expect(button("Cancel")?.disabled).toBe(false);
  });
  button("Cancel").click();
  await vi.waitFor(() => {
    flushSync();
    expect(button("Continue")).toBeTruthy();
  });
  first.resolve(pending);
  await new Promise((resolve) => setTimeout(resolve, 20));
  flushSync();
  expect(button("Continue")).toBeTruthy();
  expect(button("Cancel")).toBeUndefined();
  expect(connected).not.toHaveBeenCalled();
});
test("an account committed before cancellation is surfaced exactly once", async () => {
  const first = deferred<typeof pending>();
  let polls = 0;
  const invoke = vi.fn(async (command: string) => {
    if (command === "community_catalog") return catalog;
    if (command === "community_login_start") return pending;
    if (command === "community_login_poll")
      return ++polls === 1
        ? first.promise
        : { ticket: "ticket-a", status: "complete", entryId: "entry-a" };
  });
  const connected = await start(invoke);
  await vi.waitFor(() => {
    flushSync();
    expect(button("Cancel")?.disabled).toBe(false);
  });
  button("Cancel").click();
  await vi.waitFor(() => expect(connected).toHaveBeenCalledWith("entry-a"));
  first.resolve(pending);
  await new Promise((resolve) => setTimeout(resolve, 20));
  expect(connected).toHaveBeenCalledTimes(1);
});


test("searching and switching providers clears the previous key before connecting", async () => {
  setLocale("en");
  const invoke = vi.fn(async (command: string) => {
    if (command === "community_catalog") return [...catalog, {
      id: "qoder", name: "Qoder", methods: [{ index: 0, type: "oauth", label: "Sign in with Qoder", native: false, prompts: [] }]
    }];
    if (command === "community_login_start" || command === "community_login_poll") return pending;
  });
  app = mount(Dialog, { target: document.body, props: { invokeTauri: invoke as never, onClose: vi.fn(), onConnected: vi.fn() } });
  await vi.waitFor(() => { flushSync(); expect(document.querySelector('input[type="password"]')).toBeTruthy(); });
  const key = document.querySelector<HTMLInputElement>('input[type="password"]')!;
  key.value = "synthetic-previous-key"; key.dispatchEvent(new Event("input", { bubbles: true }));
  const search = document.querySelector<HTMLInputElement>('input[aria-label="Search providers"]')!;
  search.value = "qod"; search.dispatchEvent(new Event("input", { bubbles: true })); flushSync();
  const matches = [...document.querySelectorAll<HTMLButtonElement>(".provider-option")];
  expect(matches).toHaveLength(1); matches[0].click(); flushSync();
  expect(document.querySelector('input[type="password"]')).toBeNull();
  button("Continue").click();
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("community_login_start", {
    input: { provider: "qoder", method: 0, inputs: {}, apiKey: null }
  }));
});


test("plan connection sends the selected billing region through typed login input", async () => {
  setLocale("zh-CN");
  const invoke = vi.fn(async (command: string) => {
    if (command === "community_catalog") return [{
      id: "mimo-plan", name: "MiMo Token Plan", methods: [{
        index: 0, type: "api", label: "Plan key", labelKey: "subscriptionMethod.mimo-plan.0", native: false,
        prompts: [{type: "select", key: "region", message: "Region", messageKey: "subscriptionConnect.region", options: [
          {value: "cn", label: "China", labelKey: "subscriptionRegion.cn"},
          {value: "sgp", label: "Singapore", labelKey: "subscriptionRegion.sgp"}
        ]}]
      }]
    }];
    if (command === "community_login_start") return {...pending,userCode:"ABCD-EFGH"};
    if (command === "community_login_poll") return {...pending,userCode:"ABCD-EFGH"};
  });
  app = mount(Dialog,{target:document.body,props:{invokeTauri:invoke as never,initialProvider:"mimo-plan",onClose:vi.fn(),onConnected:vi.fn()}});
  await vi.waitFor(() => {flushSync();expect(document.querySelector('input[type="password"]')).toBeTruthy();});
  expect(document.body.textContent).toContain("中国");expect(document.body.textContent).not.toContain("国内");
  const input=document.querySelector<HTMLInputElement>('input[type="password"]')!;
  input.value="tp-synthetic-plan-key";input.dispatchEvent(new Event("input",{bubbles:true}));flushSync();
  button("继续连接").click();
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("community_login_start",{input:{provider:"mimo-plan",method:0,inputs:{region:"cn"},apiKey:"tp-synthetic-plan-key"}}));
  await vi.waitFor(() => {flushSync();expect(document.body.textContent).toContain("ABCD-EFGH");});
  expect(document.querySelector('input[type="password"]')?.getAttribute("value") ?? "").not.toContain("synthetic");
});

test.each(["token-plan", "coding-plan"])("Bailian %s key pages follow the selected account region", async (plan) => {
  setLocale("en");
  const invoke = vi.fn(async (command: string) => {
    if (command === "community_catalog") return subscriptionCatalog;
    if (command === "community_login_start") return pending;
    if (command === "community_login_poll") return {...pending, status: "failed", error: "fixture key rejected"};
  });
  app = mount(Dialog, {target: document.body, props: {invokeTauri: invoke as never, initialProvider: `bailian-${plan}`, onClose: vi.fn(), onConnected: vi.fn()}});
  await vi.waitFor(() => { flushSync(); expect(button("Get API key")).toBeTruthy(); });
  button("Get API key").click();
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("oauth_open_verification", {uri: "https://bailian.console.aliyun.com"}));
  document.querySelector<HTMLButtonElement>('[aria-label="Region"]')!.dispatchEvent(new PointerEvent("pointerdown", {bubbles: true, button: 0, pointerType: "mouse"})); flushSync();
  await vi.waitFor(() => expect(document.querySelector('[role="option"]')).toBeTruthy());
  const option = [...document.querySelectorAll<HTMLElement>('[role="option"]')].find(e => e.textContent?.trim() === "International")!;
  option.dispatchEvent(new PointerEvent("pointerup", {bubbles: true, button: 0, pointerType: "mouse"})); flushSync();
  button("Get API key").click();
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("oauth_open_verification", {uri: `https://modelstudio.console.alibabacloud.com/ap-southeast-1/subscription/${plan}`}));
  const input = document.querySelector<HTMLInputElement>('input[type="password"]')!;
  input.value = "sk-sp-synthetic-plan-key"; input.dispatchEvent(new Event("input", {bubbles: true})); flushSync();
  const body = document.querySelector<HTMLElement>(".connection-body")!;
  body.scrollTop = 70;
  button("Continue").click();
  await vi.waitFor(() => { flushSync(); expect(document.body.textContent).toContain("fixture key rejected"); });
  expect(document.querySelector('[aria-label="Region"]')?.textContent).toContain("International");
  expect(input.value).toBe("");
  expect(body.scrollTop).toBe(0);
});

test("Kilo failure retains the organization scope while erasing the key", async () => {
  setLocale("en");
  const invoke=vi.fn(async (command:string)=>command==="community_catalog"?subscriptionCatalog:command==="community_login_start"?pending:command==="community_login_poll"?{...pending,status:"failed",error:"fixture key rejected"}:undefined);
  app=mount(Dialog,{target:document.body,props:{invokeTauri:invoke as never,initialProvider:"kilo-gateway",onClose:vi.fn(),onConnected:vi.fn()}});
  await vi.waitFor(()=>{flushSync();expect(document.querySelector('input[type="password"]')).toBeTruthy();});
  const input=document.querySelector<HTMLInputElement>('input[type="password"]')!;
  const org=document.querySelector<HTMLInputElement>('fieldset input:not([type="password"])')!;
  org.value="org-a";org.dispatchEvent(new Event("input",{bubbles:true}));
  input.value="synthetic-key";input.dispatchEvent(new Event("input",{bubbles:true}));flushSync();button("Continue").click();
  await vi.waitFor(()=>{flushSync();expect(document.body.textContent).toContain("fixture key rejected");});
  expect(input.value).toBe("");expect(org.value).toBe("org-a");
  expect(invoke).toHaveBeenCalledWith("community_login_start",{input:{provider:"kilo-gateway",method:0,inputs:{organization:"org-a"},apiKey:"synthetic-key"}});
});

test("reconnect pins the original provider and sends its existing entry id", async () => {
  setLocale("en");
  const invoke = vi.fn(async (command: string) => command === "community_catalog" ? [...catalog, {
    id: "qoder", name: "Qoder", localAuthorization: true,
    methods: [{ index: 0, type: "oauth", label: "Sign in", native: false, prompts: [] }]
  }] : pending);
  app = mount(Dialog, { target: document.body, props: { invokeTauri: invoke as never, entryId: "existing-account", initialProvider: "qoder", onClose: vi.fn(), onConnected: vi.fn() } });
  await vi.waitFor(() => { flushSync(); expect(button("Continue").disabled).toBe(false); });
  expect(document.querySelector(".provider-picker")).toBeNull();
  expect(document.body.textContent).toContain("excluded from sync and backups");
  expect(document.body.textContent).not.toContain("Factory");
  button("Continue").click();
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("community_login_start", { input: {
    provider: "qoder", method: 0, inputs: {}, apiKey: null, entryId: "existing-account"
  } }));
});

import { expect, test } from "vitest";
import type { ProviderEntry } from "@aipass/schemas";
import { agentCredentialChoices } from "./agent-credentials";
const api: ProviderEntry = { id: "api", title: "Gateway", favorite: false, providerKind: "third_party", credentialKind: "api",
  interfaceType: "openai_compatible", authScheme: "bearer", domains: [], endpoints: [], tags: [],
  secretRefs: [{ id: "openai", label: "Responses", masked: "••••", fingerprint: "test" },
    { id: "claude", label: "Claude", interfaceType: "anthropic_messages", masked: "••••", fingerprint: "test" }] };
test("Agent choices respect individual credential protocols and stable IDs", () => {
  expect(agentCredentialChoices([api], "codex").map(c => c.request)).toEqual([
    { tool: "codex", id: "api", secretId: "openai", mode: "plaintext", codexApiKeyMode: "auth_json" }
  ]);
  expect(agentCredentialChoices([api], "claude-code").map(c => c.value)).toEqual(["api:claude"]);
  expect(agentCredentialChoices([{ ...api, archivedAt: "2026-10-11T00:00:00Z" }], "codex")).toEqual([]);
});
test("native subscriptions stay with their official Agent and model-dependent Agents require a model", () => {
  const subscription = { ...api, id: "subscription", providerId: "codex", providerKind: "official" as const, credentialKind: "oauth" as const,
    secretRefs: [api.secretRefs[0]] };
  expect(agentCredentialChoices([subscription], "codex")[0].request.mode).toBe("official");
  expect(agentCredentialChoices([subscription], "claude-code")).toEqual([]);
  expect(agentCredentialChoices([subscription], "opencode")).toEqual([]);
  expect(agentCredentialChoices([api], "pi")).toEqual([]);
  expect(agentCredentialChoices([{ ...api, secretRefs: [{ ...api.secretRefs[0], defaultModel: "model" }] }], "pi")).toHaveLength(1);
});

test("multiple keys have distinct recognizable labels without changing selection IDs", () => {
  const keys = { ...api, secretRefs: [{ ...api.secretRefs[0], id: "work", label: "Work" }, { ...api.secretRefs[0], id: "personal", label: "Personal" }] };
  expect(agentCredentialChoices([keys], "codex").map(c => c.label)).toEqual(["Gateway (Work)", "Gateway (Personal)"]);
  const duplicates = { ...keys, secretRefs: keys.secretRefs.map(s => ({ ...s, label: "Primary" })) };
  const choices = agentCredentialChoices([duplicates], "codex");
  expect(new Set(choices.map(c => c.label)).size).toBe(2);
  expect(choices.map(c => c.value)).toEqual(["api:work", "api:personal"]);
});

test("separate imported entries with the same title remain distinguishable", () => {
  const one = { ...api, secretRefs: [api.secretRefs[0]] };
  const choices = agentCredentialChoices([one, { ...one, id: "second" }], "codex");
  expect(new Set(choices.map(c => c.label)).size).toBe(2);
  expect(choices.map(c => c.value)).toEqual(["api:openai", "second:openai"]);
});

test("native account labels keep the email and a short workspace scope", () => {
  const identity = "alice@example.test:workspace-1234567890";
  const subscription = {...api, title: `codex · ${identity}`, providerId: "codex", providerKind: "official" as const, credentialKind: "oauth" as const, accountIdentity: identity, secretRefs: [api.secretRefs[0]]};
  const [choice] = agentCredentialChoices([subscription], "codex");
  expect(choice.label).toContain("alice@example.test");
  expect(choice.label).toContain("34567890");
  expect(choice.label).not.toContain("workspace-1234567890");
  expect(choice.value).toBe("api:openai");
});

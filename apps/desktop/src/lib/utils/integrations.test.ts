import { describe, expect, it } from "vitest";

import {
  installedIntegrationTools,
  compatibleToolsFor,
  integrationToolDefinitions,
  localProxyAvailability,
  providerIntegrationAvailability,
  supportsIntegration
} from "./integrations";

function tool(id: (typeof integrationToolDefinitions)[number]["id"]) {
  const definition = integrationToolDefinitions.find((item) => item.id === id);
  if (!definition) throw new Error(`missing integration tool ${id}`);
  return definition;
}

describe("local proxy integration availability", () => {
  it("keeps mainstream agents in the catalog", () => {
    expect(integrationToolDefinitions.map((item) => item.id)).toEqual([
      "codex",
      "claude-code",
      "gemini-cli",
      "opencode",
      "grok",
      "pi",
      "cursor",
      "kimi-code",
      "zed",
      "deepseek-harness",
      "antigravity-cli",
      "muse-code",
      "hermes-agent",
      "command-code",
      "codebuddy",
      "workbuddy",
      "vscode-chat",
      "cline",
      "trae",
      "qoder",
      "zcode",
      "jetbrains-copilot"
    ]);
  });

  it("matches each agent to the protocols it actually supports", () => {
    expect(localProxyAvailability(tool("codex"), "open_ai_responses", true)).toBe(
      "available"
    );
    expect(localProxyAvailability(tool("codex"), "open_ai_chat_completions", true)).toBe(
      "protocol"
    );
    expect(localProxyAvailability(tool("opencode"), "open_ai_chat_completions", true)).toBe(
      "available"
    );
    expect(localProxyAvailability(tool("opencode"), "open_ai_responses", true)).toBe(
      "available"
    );
    expect(localProxyAvailability(tool("opencode"), "anthropic_messages", true)).toBe(
      "available"
    );
    expect(localProxyAvailability(tool("grok"), "anthropic_messages", true)).toBe(
      "available"
    );
    expect(localProxyAvailability(tool("pi"), "open_ai_responses", true)).toBe("available");
  });

  it("requires a usable model for Grok and Pi", () => {
    expect(localProxyAvailability(tool("grok"), "open_ai_responses", false)).toBe(
      "default-model"
    );
    expect(localProxyAvailability(tool("pi"), "anthropic_messages", false)).toBe(
      "default-model"
    );
  });

  it("distinguishes Cursor local runtime from unsupported native APIs", () => {
    expect(tool("gemini-cli").localProxyUnsupportedReason).toBe("native-api");
    expect(localProxyAvailability(tool("cursor"), "open_ai_chat_completions", true)).toBe(
      "available"
    );
    expect(localProxyAvailability(tool("cursor"), "open_ai_responses", true)).toBe(
      "available"
    );
    expect(localProxyAvailability(tool("gemini-cli"), "anthropic_messages", true)).toBe(
      "unsupported"
    );
  });
});

describe("provider integration support", () => {
  it("uses the local proxy for Cursor Anthropic routes instead of offering unsupported direct setup", () => {
    const entry = {
      id: "anthropic", title: "Anthropic",
      interfaceType: "anthropic_messages" as const, authScheme: "x_api_key" as const,
      defaultModel: "fixture-model"
    };
    expect(supportsIntegration("cursor", entry)).toBe(false);
    expect(supportsIntegration("grok", entry)).toBe(true);
    expect(supportsIntegration("pi", entry)).toBe(true);
    expect(localProxyAvailability(tool("cursor"), "anthropic_messages", true)).toBe("available");
  });

  it.each([undefined, "", "   "])("keeps Grok Build discoverable with missing model %s", (defaultModel) => {
    const entry = {
      id: "entry", title: "OpenAI compatible",
      interfaceType: "openai_compatible" as const, authScheme: "bearer" as const,
      defaultModel
    };
    expect(compatibleToolsFor(entry)).toContain(tool("grok"));
    expect(providerIntegrationAvailability(tool("grok"), entry)).toBe("default-model");
    expect(supportsIntegration("grok", entry)).toBe(false);
    expect(supportsIntegration("grok", { ...entry, defaultModel: "fixture-model" })).toBe(true);
  });

  it.each(["open_ai_chat_completions", "open_ai_responses"] as const)("offers Grok Build for %s routes", (protocol) => {
    expect(localProxyAvailability(tool("grok"), protocol, true)).toBe("available");
  });

  it("offers Grok and Pi only when a default model is available", () => {
    const entry = {
      id: "entry",
      title: "OpenAI compatible",
      interfaceType: "openai_compatible" as const,
      authScheme: "bearer" as const
    };
    expect(supportsIntegration("grok", entry)).toBe(false);
    expect(supportsIntegration("pi", { ...entry, defaultModel: "gpt-5.4" })).toBe(true);
  });
});

it("shares installed-only filtering and ignores configuration left by an uninstalled Agent", () => {
  expect(installedIntegrationTools([])).toEqual([]);
  expect(installedIntegrationTools([
    { tool: "zed", binaryFound: true },
    { tool: "kimi-code", binaryFound: false, configPath: "/old/.kimi-code" },
    { tool: "cursor", binaryFound: false, localRuntimeFound: true },
    { tool: "deepseek-harness", binaryFound: true }
  ]).map(t => t.id)).toEqual(["cursor", "zed", "deepseek-harness"]);
});

it.each(["kimi-code", "zed", "deepseek-harness"] as const)("keeps installed %s visible when it needs a model and supports every proxy protocol", id => {
  const entry = { id: "api", title: "API", interfaceType: "openai_compatible" as const, authScheme: "bearer" as const };
  expect(compatibleToolsFor(entry)).toContain(tool(id));
  expect(providerIntegrationAvailability(tool(id), entry)).toBe("default-model");
  for (const api of ["open_ai_chat_completions", "open_ai_responses", "anthropic_messages"] as const) {
    expect(localProxyAvailability(tool(id), api, true)).toBe("available");
  }
});

it("offers Antigravity CLI only for Gemini credentials and never for a local proxy", () => {
  expect(supportsIntegration("antigravity-cli", { id: "gemini", title: "Gemini", interfaceType: "gemini", authScheme: "google_api_key" })).toBe(true);
  expect(supportsIntegration("antigravity-cli", { id: "api", title: "API", interfaceType: "openai_compatible", authScheme: "bearer" })).toBe(false);
  expect(localProxyAvailability(tool("antigravity-cli"), "open_ai_responses", true)).toBe("unsupported");
});

it("recognizes an installed Cline extension without a standalone CLI", () => {
  expect(installedIntegrationTools([{ tool: "cline", binaryFound: false, extensionFound: true }]).map(t => t.id)).toEqual(["cline"]);
});
it("uses protocol-specific BYOK writers and explicit UI guides", () => {
  expect(localProxyAvailability(tool("muse-code"), "open_ai_responses", true)).toBe("unsupported");
  expect(localProxyAvailability(tool("muse-code"), "open_ai_chat_completions", true)).toBe("unsupported");
  const entry = { id: "fixture", title: "Fixture", interfaceType: "openai_compatible" as const, authScheme: "bearer" as const, defaultModel: "fixture-model" };
  expect(supportsIntegration("muse-code", { ...entry, endpoint: "https://ordinary.test/v1" })).toBe(false);
  expect(supportsIntegration("muse-code", { ...entry, endpoint: "https://api.meta.ai/v1" })).toBe(true);
  for (const protocol of ["open_ai_chat_completions", "open_ai_responses", "anthropic_messages"] as const) expect(localProxyAvailability(tool("qoder"), protocol, true)).toBe("available");
  for (const id of ["codebuddy", "workbuddy"] as const) {
    expect(localProxyAvailability(tool(id), "open_ai_chat_completions", true)).toBe("available");
    expect(localProxyAvailability(tool(id), "anthropic_messages", true)).toBe("protocol");
  }
  for (const id of ["vscode-chat", "cline", "trae", "qoder", "zcode", "jetbrains-copilot"] as const) expect(tool(id).setupGuide).toBe(id);
});


it("limits ZCode and JetBrains Copilot guides to confirmed endpoint protocols", () => {
  expect(localProxyAvailability(tool("zcode"), "open_ai_responses", true)).toBe("protocol");
  expect(localProxyAvailability(tool("zcode"), "anthropic_messages", true)).toBe("available");
  expect(localProxyAvailability(tool("jetbrains-copilot"), "open_ai_chat_completions", true)).toBe("available");
  expect(localProxyAvailability(tool("jetbrains-copilot"), "anthropic_messages", true)).toBe("protocol");
});

it("routes bearer-only Anthropic gateways through the proxy instead of sending x-api-key directly", () => {
  const entry = {id:"bearer",title:"Gateway",interfaceType:"anthropic_messages" as const,authScheme:"bearer" as const,defaultModel:"fixture"};
  expect(supportsIntegration("claude-code", entry)).toBe(true);
  for (const id of ["opencode","grok","pi","kimi-code","zed","deepseek-harness","hermes-agent","command-code","vscode-chat","cline","trae","qoder","zcode"] as const) {
    expect(supportsIntegration(id, entry)).toBe(false);
    expect(localProxyAvailability(tool(id), "anthropic_messages", true)).toBe("available");
  }
});


it("requires the actual Cursor Local runtime instead of a regular Cursor CLI", () => {
  expect(installedIntegrationTools([{tool:"cursor",binaryFound:true,localRuntimeFound:false}])).toEqual([]);
  expect(installedIntegrationTools([{tool:"cursor",binaryFound:true,localRuntimeFound:true}]).map(t => t.id)).toEqual(["cursor"]);
});

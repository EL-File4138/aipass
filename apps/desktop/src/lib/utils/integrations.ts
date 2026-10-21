import type { AuthScheme, InterfaceType } from "@aipass/schemas";

import type { ProxyProtocol, ToolConfigMode, ToolConfigTarget, ToolDetection } from "../types";

export type LocalProxyAvailability =
  | "available"
  | "unsupported"
  | "protocol"
  | "default-model";

export type IntegrationToolDefinition = {
  id: ToolConfigTarget;
  name: string;
  defaultMode: ToolConfigMode;
  localProxyProtocols: ProxyProtocol[];
  localProxyUnsupportedReason?: "native-api" | "custom-endpoint" | "local-runtime-required" | "model-catalog";
  requiresDefaultModel?: boolean;
  disabledReason?: string;
  setupGuide?: "vscode-chat" | "cline" | "trae" | "qoder" | "zcode" | "jetbrains-copilot";
};

export type IntegrationEntry = {
  id: string;
  title: string;
  interfaceType: InterfaceType;
  authScheme: AuthScheme;
  defaultModel?: string;
  endpoint?: string;
};

export const integrationToolDefinitions: IntegrationToolDefinition[] = [
  {
    id: "codex",
    name: "Codex",
    defaultMode: "plaintext",
    localProxyProtocols: ["open_ai_responses"]
  },
  {
    id: "claude-code",
    name: "Claude Code",
    defaultMode: "helper",
    localProxyProtocols: ["anthropic_messages"]
  },
  {
    id: "gemini-cli",
    name: "Gemini CLI",
    defaultMode: "helper",
    localProxyProtocols: [],
    localProxyUnsupportedReason: "native-api"
  },
  {
    id: "opencode",
    name: "OpenCode",
    defaultMode: "helper",
    localProxyProtocols: [
      "open_ai_responses",
      "open_ai_chat_completions",
      "anthropic_messages"
    ]
  },
  {
    id: "grok",
    name: "Grok Build",
    defaultMode: "helper",
    localProxyProtocols: [
      "open_ai_responses",
      "open_ai_chat_completions",
      "anthropic_messages"
    ],
    requiresDefaultModel: true
  },
  {
    id: "pi",
    name: "Pi",
    defaultMode: "helper",
    localProxyProtocols: [
      "open_ai_responses",
      "open_ai_chat_completions",
      "anthropic_messages"
    ],
    requiresDefaultModel: true
  },
  {
    id: "cursor",
    name: "Cursor Agent Local",
    defaultMode: "helper",
    localProxyProtocols: [
      "open_ai_responses",
      "open_ai_chat_completions",
      "anthropic_messages"
    ]
  },
  {
    id: "kimi-code", name: "Kimi Code", defaultMode: "helper",
    localProxyProtocols: ["open_ai_responses", "open_ai_chat_completions", "anthropic_messages"],
    requiresDefaultModel: true
  },
  {
    id: "zed", name: "Zed", defaultMode: "helper",
    localProxyProtocols: ["open_ai_responses", "open_ai_chat_completions", "anthropic_messages"],
    requiresDefaultModel: true
  },
  {
    id: "deepseek-harness", name: "DeepSeek Harness", defaultMode: "helper",
    localProxyProtocols: ["open_ai_responses", "open_ai_chat_completions", "anthropic_messages"],
    requiresDefaultModel: true
  },
  {
    id: "antigravity-cli", name: "Antigravity CLI", defaultMode: "helper",
    localProxyProtocols: [], localProxyUnsupportedReason: "native-api"
  },
  { id: "muse-code", name: "Muse Code", defaultMode: "helper", localProxyProtocols: [], localProxyUnsupportedReason: "model-catalog", requiresDefaultModel: true },
  { id: "hermes-agent", name: "Hermes Agent", defaultMode: "helper", localProxyProtocols: ["open_ai_responses", "open_ai_chat_completions", "anthropic_messages"], requiresDefaultModel: true },
  { id: "command-code", name: "Command Code", defaultMode: "helper", localProxyProtocols: ["open_ai_responses", "open_ai_chat_completions", "anthropic_messages"], requiresDefaultModel: true },
  { id: "codebuddy", name: "CodeBuddy", defaultMode: "plaintext", localProxyProtocols: ["open_ai_chat_completions"], requiresDefaultModel: true },
  { id: "workbuddy", name: "WorkBuddy", defaultMode: "plaintext", localProxyProtocols: ["open_ai_chat_completions"], requiresDefaultModel: true },
  { id: "vscode-chat", name: "VS Code Chat", defaultMode: "helper", localProxyProtocols: ["open_ai_responses", "open_ai_chat_completions", "anthropic_messages"], requiresDefaultModel: true, setupGuide: "vscode-chat" },
  { id: "cline", name: "Cline", defaultMode: "helper", localProxyProtocols: ["open_ai_responses", "open_ai_chat_completions", "anthropic_messages"], requiresDefaultModel: true, setupGuide: "cline" },
  { id: "trae", name: "TRAE", defaultMode: "helper", localProxyProtocols: ["open_ai_chat_completions", "anthropic_messages"], requiresDefaultModel: true, setupGuide: "trae" },
  { id: "qoder", name: "Qoder", defaultMode: "helper", localProxyProtocols: ["open_ai_responses", "open_ai_chat_completions", "anthropic_messages"], requiresDefaultModel: true, setupGuide: "qoder" },
  { id: "zcode", name: "ZCode", defaultMode: "helper", localProxyProtocols: ["open_ai_chat_completions", "anthropic_messages"], requiresDefaultModel: true, setupGuide: "zcode" },
  { id: "jetbrains-copilot", name: "JetBrains Copilot", defaultMode: "helper", localProxyProtocols: ["open_ai_chat_completions"], requiresDefaultModel: true, setupGuide: "jetbrains-copilot" }
];

function supportsIntegrationProtocol(tool: ToolConfigTarget, entry: IntegrationEntry): boolean {
  switch (tool) {
    case "muse-code":
      return entry.interfaceType === "openai_compatible" && entry.authScheme === "bearer" &&
        entry.endpoint?.replace(/\/+$/, "") === "https://api.meta.ai/v1";
    case "codebuddy":
    case "workbuddy":
    case "jetbrains-copilot":
    case "codex":
      return entry.interfaceType === "openai_compatible" && entry.authScheme === "bearer";
    case "claude-code":
      return (
        entry.interfaceType === "anthropic_messages" &&
        (entry.authScheme === "x_api_key" || entry.authScheme === "bearer")
      );
    case "antigravity-cli":
    case "gemini-cli":
      return entry.interfaceType === "gemini" && entry.authScheme === "google_api_key";
    case "opencode":
      return !(entry.interfaceType === "anthropic_messages" && entry.authScheme !== "x_api_key");
    case "kimi-code":
      return (entry.interfaceType === "gemini" && entry.authScheme === "google_api_key") ||
        supportsIntegrationProtocol("zed", entry);
    case "cursor":
      return entry.interfaceType === "openai_compatible" && entry.authScheme === "bearer";
    case "hermes-agent":
    case "command-code":
    case "vscode-chat":
    case "cline":
    case "zcode":
    case "trae":
    case "qoder":
    case "zed":
    case "deepseek-harness":
    case "grok":
    case "pi":
      return (
        (entry.interfaceType === "openai_compatible" && entry.authScheme === "bearer") ||
        (entry.interfaceType === "anthropic_messages" &&
          entry.authScheme === "x_api_key")
      );
  }
}

export function providerIntegrationAvailability(
  tool: IntegrationToolDefinition,
  entry: IntegrationEntry
): LocalProxyAvailability {
  if (!supportsIntegrationProtocol(tool.id, entry)) return "protocol";
  if (tool.requiresDefaultModel && !entry.defaultModel?.trim()) return "default-model";
  return "available";
}

export function supportsIntegration(tool: ToolConfigTarget, entry: IntegrationEntry): boolean {
  const definition = integrationToolDefinitions.find((item) => item.id === tool);
  return Boolean(definition && providerIntegrationAvailability(definition, entry) === "available");
}

export function localProxyAvailability(
  tool: IntegrationToolDefinition,
  protocol: ProxyProtocol,
  hasDefaultModel: boolean
): LocalProxyAvailability {
  if (tool.localProxyUnsupportedReason) return "unsupported";
  if (!tool.localProxyProtocols.includes(protocol)) return "protocol";
  if (tool.requiresDefaultModel && !hasDefaultModel) return "default-model";
  return "available";
}

export function compatibleToolsFor(entry: IntegrationEntry): IntegrationToolDefinition[] {
  return integrationToolDefinitions.filter((tool) => supportsIntegrationProtocol(tool.id, entry));
}

export function integrationToolName(tool: ToolConfigTarget): string {
  return integrationToolDefinitions.find((definition) => definition.id === tool)?.name ?? tool;
}

export function compatibleEntriesForTool(
  entries: IntegrationEntry[],
  tool: ToolConfigTarget
): IntegrationEntry[] {
  return entries.filter((entry) => supportsIntegration(tool, entry));
}

export function toolIsInstalled(detection: ToolDetection): boolean {
  if (detection.tool === "cursor") return detection.localRuntimeFound === true;
  return detection.binaryFound || detection.localRuntimeFound === true || detection.extensionFound === true;
}

export function installedIntegrationTools(
  detections: ToolDetection[], tools: IntegrationToolDefinition[] = integrationToolDefinitions
): IntegrationToolDefinition[] {
  const installed = new Set(detections.filter(toolIsInstalled).map(d => d.tool));
  return tools.filter(tool => installed.has(tool.id));
}

import { secretAuthScheme, secretInterfaceType, subscriptionAccountPresentation, type ProviderEntry } from "@aipass/schemas";
import type { ToolConfigTarget, ToolSwitchRequest } from "../types";
import { supportsIntegration } from "./integrations";

export type AgentCredentialChoice = { value: string; label: string; request: ToolSwitchRequest };

export function agentCredentialChoices(entries: ProviderEntry[], tool: ToolConfigTarget): AgentCredentialChoice[] {
  const choices = entries.filter(entry => !entry.archivedAt).flatMap(entry => entry.secretRefs.flatMap<AgentCredentialChoice>(secret => {
    const native = entry.credentialKind === "oauth" && entry.providerKind === "official" && (
      (tool === "codex" && ["codex", "openai"].includes(entry.providerId ?? "")) ||
      (tool === "claude-code" && entry.providerId === "anthropic")
    );
    if (!native && entry.credentialKind !== "api") return [];
    const effective = { ...entry, endpoint: secret.endpoint ?? entry.endpoints.find(e => e.kind === "api")?.url, defaultModel: secret.defaultModel ?? entry.defaultModel,
      interfaceType: secretInterfaceType(secret, entry.interfaceType),
      authScheme: secretAuthScheme(secret, entry.interfaceType, entry.authScheme) };
    if (!native && !supportsIntegration(tool, effective)) return [];
    const detail = secret.label.trim() || secret.id;
    const duplicate = entry.secretRefs.filter(s => (s.label.trim() || s.id) === detail).length > 1;
    const account = native ? subscriptionAccountPresentation(entry) : undefined;
    const title = account ? `${account.title}${account.subtitle ? ` (${account.subtitle})` : ""}` : entry.title;
    const label = entry.secretRefs.length > 1 ? `${title} (${detail}${duplicate ? ` / ${secret.id}` : ""})` : title;
    return [{ value: `${entry.id}:${secret.id}`, label,
      request: { tool, id: entry.id, secretId: secret.id, mode: native ? "official" : ["zed", "antigravity-cli", "muse-code", "hermes-agent", "command-code"].includes(tool) ? "helper" : "plaintext",
        ...(tool === "codex" && !native ? { codexApiKeyMode: "auth_json" as const } : {}) } }];
  }));
  return choices.map(choice => choices.filter(c => c.label === choice.label).length > 1
    ? { ...choice, label: `${choice.label} (${choice.request.id.slice(-8)} / ${choice.request.secretId})` }
    : choice);
}

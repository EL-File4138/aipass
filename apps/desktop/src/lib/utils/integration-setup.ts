import { secretAuthScheme, secretInterfaceType, type ProviderEntry } from "@aipass/schemas";

export function integrationSetupDetails(entry: ProviderEntry | undefined, secretId?: string) {
  const secret = entry?.secretRefs.find(s => s.id === secretId);
  if (!entry || !secret) return undefined;
  const interfaceType = secretInterfaceType(secret, entry.interfaceType);
  return {
    endpoint: secret.endpoint ?? entry.endpoints.find(e => e.kind === "api")?.url ?? "",
    model: secret.defaultModel ?? entry.defaultModel ?? "",
    protocol: interfaceType === "anthropic_messages" ? "Anthropic Messages" : "OpenAI Chat Completions",
    authScheme: secretAuthScheme(secret, entry.interfaceType, entry.authScheme)
  };
}

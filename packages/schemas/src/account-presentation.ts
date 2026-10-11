import type { ProviderEntry } from "./index.js";

const names: Record<string, string> = {
  codex: "Codex", openai: "Codex", anthropic: "Claude", "claude-code": "Claude",
  "commandcode-plan": "Command Code", commandcode: "Command Code", "gemini-cli": "Gemini CLI",
  copilot: "GitHub Copilot", grok: "Grok Build", xai: "Grok Build", zcode: "ZCode",
  kiro: "Kiro", cursor: "Cursor", devin: "Devin", workbuddy: "WorkBuddy", "workbuddy-ai": "WorkBuddy AI"
};

export function subscriptionProviderName(provider: string): string {
  return names[provider] ?? provider;
}

export function readableAccountIdentity(identity: string): string {
  const email = identity.match(/^[^\s:@]+@[^\s:]+/);
  if (email) return email[0];
  if (identity.includes("::")) return identity.split("::").filter(Boolean)
    .map(part => part === "zai" ? "Z.ai" : part === "bigmodel" ? "BigModel" : part).join(" / ");
  if (/^[0-9a-f-]{20,}$/i.test(identity) || identity.startsWith("arn:")) return identity.slice(-8);
  return identity;
}

// Presentation only: retain the complete identity for ownership and workspace matching.
export function subscriptionAccountPresentation(entry: Pick<ProviderEntry, "title" | "providerId" | "accountIdentity">) {
  const identity = entry.accountIdentity ?? "";
  const provider = subscriptionProviderName(entry.providerId ?? "");
  const account = readableAccountIdentity(identity);
  const generated = entry.title === entry.providerId || entry.title === provider ||
    entry.title === `${entry.providerId} · ${identity}` || entry.title === `${provider} · ${identity}` || entry.title === `${provider} · ${account}`;
  const scope = identity.includes(":") && identity.includes("@") ? identity.slice(identity.indexOf(":") + 1) : "";
  const subtitle = account && (generated || !entry.title.includes(account)) ? account : "";
  return {
    title: generated ? provider || entry.title : entry.title,
    subtitle: subtitle && scope ? `${subtitle} (${scope.slice(-8)})` : subtitle,
    account: generated ? account || provider : entry.title,
    scope
  };
}

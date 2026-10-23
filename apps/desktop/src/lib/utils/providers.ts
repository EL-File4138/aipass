import type { ProviderEntry } from "@aipass/schemas";
import { encodeListValues } from "@aipass/ui";

import type { EntrySummary } from "../types";

export { emptyDraft, mergeHeaderPairs } from "@aipass/ui";

export { isExpiringSoon, providerCounts, entryMatchesFilter } from "@aipass/schemas";

// Older entries materialized the default API hosts. Treat that exact set as
// the default so changing an API address does not retain a stale host.
export function domainOverride(entry: ProviderEntry): string {
  const hosts = new Set(entry.endpoints.filter(endpoint => endpoint.kind === "api").flatMap(endpoint => {
    try { const url = new URL(endpoint.url ?? ""); return /^https?:$/.test(url.protocol) ? [url.hostname] : []; } catch { return []; }
  }));
  return hosts.size === entry.domains.length && entry.domains.every(domain => hosts.has(domain)) ? "" : encodeListValues(entry.domains);
}

export function summaryToEntry(summary: EntrySummary): ProviderEntry {
  return {
    id: summary.id,
    title: summary.title,
    favorite: summary.favorite ?? false,
    providerId: summary.providerId,
    providerKind: summary.providerKind,
    credentialKind: summary.credentialKind ?? "api",
    accountIdentity: summary.accountIdentity,
    domains: summary.domains,
    faviconUrl: summary.faviconUrl,
    endpoints: summary.endpoints,
    interfaceType: summary.interfaceType,
    maxConcurrentRequests: summary.maxConcurrentRequests,
    supportsWebsockets: summary.supportsWebsockets,
    websocketWarning: summary.websocketWarning,
    authScheme: summary.authScheme,
    secretRefs: summary.secretRefs ?? [
      {
        id: "primary",
        label: "primary",
        masked: summary.maskedSecret,
        fingerprint: summary.fingerprint
      }
    ],
    defaultModel: summary.defaultModel,
    modelAliases: summary.modelAliases,
    quota: summary.quota,
    subscription: summary.subscription,
    gateway: summary.gateway,
    usageSource: summary.usageSource,
    tags: summary.tags,
    notes: summary.notes,
    headerNames: summary.headerNames,
    createdAt: summary.createdAt,
    updatedAt: summary.updatedAt,
    lastUsedAt: summary.lastUsedAt,
    archivedAt: summary.archivedAt,
    deletedAt: summary.deletedAt
  };
}

import { expect, test } from "vitest";
import type { ProviderEntry } from "@aipass/schemas";
import { domainOverride } from "./providers";

test("default domains follow changed APIs while custom matching domains remain explicit", () => {
  const entry = {domains: ["api.example.test"], endpoints: [{kind: "api", url: "https://api.example.test/v1"}, {kind: "console", url: "https://console.example.test"}]} as ProviderEntry;
  expect(domainOverride(entry)).toBe("");
  expect(domainOverride({...entry, domains: ["custom.example.test"]})).toBe("custom.example.test");
  expect(domainOverride({...entry, domains: ["api.example.test", "console.example.test"]})).toContain("console.example.test");
});

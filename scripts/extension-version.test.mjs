import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync, spawnSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { resolve, join } from "node:path";
import { isExtensionVersion } from "./extension-version.mjs";

test("browser versions reject prerelease text, overflow, leading zeroes and zero-only values", () => {
  for (const version of ["1", "0.2.0", "0.2.0.65535"]) assert.equal(isExtensionVersion(version), true);
  for (const version of [undefined, "", "0", "0.0.0.0", "01.2.3", "0.2.0.65536", "1.2.3.4.5", "0.2.0-beta.2", "0.2.0-nightly.20261012.1"]) assert.equal(isExtensionVersion(version), false);
});

test("both prerelease channels stamp numeric versions and retain their full display version", () => {
  const script = resolve("scripts/set-extension-release-version.mjs");
  for (const version of ["0.2.0-beta.2", "0.2.0-nightly.20261012.1"]) {
    const root = mkdtempSync(join(tmpdir(), "aipass-extension-version-"));
    try {
      mkdirSync(join(root, "apps/extension/public"), { recursive: true });
      const packagePath = join(root, "apps/extension/package.json");
      const manifestPath = join(root, "apps/extension/public/manifest.json");
      const original = '{"version":"0.1.0"}\n';
      writeFileSync(packagePath, original);
      writeFileSync(manifestPath, original);
      const failed = spawnSync(process.execPath, [script, version], { cwd: root });
      assert.notEqual(failed.status, 0);
      assert.equal(readFileSync(packagePath, "utf8"), original);
      assert.equal(readFileSync(manifestPath, "utf8"), original);
      execFileSync(process.execPath, [script, version, "0.2.0.123"], { cwd: root });
      assert.equal(JSON.parse(readFileSync(packagePath, "utf8")).version, version);
      assert.deepEqual(JSON.parse(readFileSync(manifestPath, "utf8")), { version: "0.2.0.123", version_name: version });
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
});

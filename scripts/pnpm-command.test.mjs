import assert from "node:assert/strict";
import { mkdtempSync, mkdirSync, writeFileSync, rmSync, symlinkSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { execFileSync } from "node:child_process";
import { test } from "node:test";
import { pnpmCommand } from "./pnpm-command.mjs";

test("Windows pnpm shims resolve to Node without interpreting arguments", () => {
  const root = mkdtempSync(join(tmpdir(), "pnpm 中文 & '%-"));
  try {
    const script = join(root, "node_modules/pnpm/bin/pnpm.cjs");
    mkdirSync(join(root, "node_modules/pnpm/bin"), { recursive: true });
    writeFileSync(script, "console.log(JSON.stringify(process.argv.slice(2)))");
    const args = ["a b", "中文", "& whoami", "%PATH%", "$(pwd)", "'quoted'", 'a"b'];
    const [command, argv] = pnpmCommand(args, { platform: "win32", env: { PATH: root } });
    assert.equal(command, process.execPath);
    assert.deepEqual(JSON.parse(execFileSync(command, argv, { encoding: "utf8" })), args);
    assert.deepEqual(pnpmCommand(args, { platform: "win32", env: { npm_execpath: script } }), [command, argv]);
    const native = join(root, "node_modules/pnpm/pnpm.exe");
    writeFileSync(native, "MZfixture");
    writeFileSync(join(root, "node_modules/pnpm/package.json"), JSON.stringify({bin:{pnpm:"pnpm.exe"}}));
    assert.deepEqual(pnpmCommand(args, { platform: "win32", env: { PATH: root } }), [native, args]);
    if (process.platform !== "win32") {
      const native = join(root, "pnpm");
      symlinkSync(process.execPath, native);
      const [nativeCommand, nativeArgs] = pnpmCommand(["--version"], { env: { npm_execpath: native } });
      assert.equal(execFileSync(nativeCommand, nativeArgs, { encoding: "utf8" }).trim(), process.version);
    }
    assert.throws(() => pnpmCommand(args, { platform: "win32", env: {} }), /Cannot locate pnpm/);
  } finally { rmSync(root, { recursive: true, force: true }); }
});

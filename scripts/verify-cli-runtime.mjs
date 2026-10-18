#!/usr/bin/env node
// Exercise installed binaries with disposable data and no vendor credentials.
import assert from "node:assert/strict";
import { randomUUID } from "node:crypto";
import { execFileSync, spawn } from "node:child_process";
import {
  cpSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { join, resolve } from "node:path";
import { setTimeout as delay } from "node:timers/promises";

assert.ok(
  process.argv[2],
  "Pass the directory containing CLI, Agent and Native Host",
);
const binaries = resolve(process.argv[2]);
const root = mkdtempSync(
  process.platform === "win32"
    ? join(tmpdir(), "aipass-cli-")
    : "/tmp/aipass-cli-",
);
const suffix = process.platform === "win32" ? ".exe" : "";
const stage = join(root, "bin");
mkdirSync(stage);
for (const name of ["aipass", "aipass-agent", "aipass-native-host"]) {
  assert.ok(
    existsSync(join(binaries, name + suffix)),
    `Missing required ${name}`,
  );
  cpSync(join(binaries, name + suffix), join(stage, name + suffix));
}
const vault = join(root, "data/vault");
const home = join(root, "home");
const runtime = join(root, "run");
for (const directory of [home, runtime, join(root, "data/agent")])
  mkdirSync(directory, { recursive: true });
writeFileSync(
  join(root, "data/agent/sync-settings.json"),
  '{"mode":"local","cloudkitMigrationComplete":true}',
);
const env = {
  ...Object.fromEntries(
    Object.entries(process.env).filter(([key]) => !key.startsWith("AIPASS_")),
  ),
  HOME: home,
  USERPROFILE: home,
  XDG_CONFIG_HOME: join(home, ".config"),
  XDG_DATA_HOME: join(home, ".local/share"),
  XDG_CACHE_HOME: join(home, ".cache"),
  XDG_RUNTIME_DIR: runtime,
  AIPASS_VAULT_DIR: vault,
  AIPASS_AGENT_RUNTIME_DIR: runtime,
  AIPASS_LOG_DIR: join(root, "logs"),
  AIPASS_AGENT_SUPPRESS_TRAY: "1",
};
for (const key of [
  "AIPASS_AGENT_BINARY",
  "AIPASS_AGENT_PATH",
  "AIPASS_MASTER_PASSWORD",
  "AIPASS_INPUT_API_KEY",
])
  delete env[key];
const cli = join(stage, "aipass" + suffix);
const run = (args, extra = {}) =>
  JSON.parse(
    execFileSync(cli, ["--json", ...args], {
      env: { ...env, ...extra },
      encoding: "utf8",
      timeout: 30_000,
      stdio: ["ignore", "pipe", "pipe"],
    }),
  );
// Starting explicitly avoids registering a login service during this test.
const agent = spawn(join(stage, "aipass-agent" + suffix), ["--vault", vault], {
  env,
  stdio: "ignore",
});
let stopped = false;
agent.on("exit", () => {
  stopped = true;
});
try {
  for (let attempt = 0; attempt < 100; attempt++) {
    assert.ok(!stopped, "Agent exited during startup");
    if (readdirSync(runtime).some((name) => name.endsWith(".token"))) break;
    await delay(100);
  }
  const password = "fixture-master-password-2026";
  run(["init"], { AIPASS_MASTER_PASSWORD: password });
  const extension = "a".repeat(32);
  const installed = run([
    "native-host",
    "install",
    "--extension-id",
    extension,
    "--browser",
    "chrome",
  ]);
  assert.equal(installed.hostPath, join(stage, "aipass-native-host" + suffix));
  const ping = Buffer.from(
    JSON.stringify({
      type: "ping",
      id: randomUUID(),
      protocol_version: 1,
      extension_id: extension,
    }),
  );
  const length = Buffer.alloc(4);
  length.writeUInt32LE(ping.length);
  const reply = execFileSync(installed.hostPath, [], {
    env,
    input: Buffer.concat([length, ping]),
    timeout: 30_000,
  });
  assert.equal(reply.readUInt32LE(0), reply.length - 4);
  assert.equal(JSON.parse(reply.subarray(4).toString()).ok, true);
  const key = "sk-aipass-platform-runtime-fixture-secret";
  const id = run(
    [
      "add",
      "--title",
      "Platform fixture",
      "--provider",
      "openai",
      "--domain",
      "fixture.invalid",
      "--endpoint",
      "https://fixture.invalid/v1",
      "--interface",
      "openai-compatible",
      "--auth",
      "bearer",
    ],
    { AIPASS_INPUT_API_KEY: key },
  ).id;
  assert.equal(typeof id, "string");
  assert.equal(run(["list"]).length, 1);
  assert.equal(run(["search", key]).length, 1);
  assert.ok(!JSON.stringify(run(["list"])).includes(key), "List leaked a key");
  assert.equal(run(["get", id, "--field", "api_key", "--reveal"]).secret, key);
  run(["update", id, "--title", "Updated fixture"]);
  assert.match(
    JSON.stringify(run(["get", id, "--field", "title"])),
    /Updated fixture/,
  );
  const backup = join(root, "backup.aipass-backup");
  run(["vault", "export", "--output", backup, "--export-password", password]);
  assert.ok(
    !readFileSync(backup).includes(Buffer.from(key)),
    "Backup leaked a key",
  );
  const sync = join(root, "sync");
  run(["sync", "--dir", sync]);
  run(["archive", id]);
  assert.equal(run(["list"]).length, 0);
  run(["restore", id]);
  assert.equal(run(["list"]).length, 1);
  run(["lock"]);
  assert.equal(run(["vault", "status"]).locked, true);
  assert.throws(() => run(["get", id, "--field", "api_key", "--reveal"]));
  run(["unlock"], { AIPASS_MASTER_PASSWORD: password });
  run(["delete", id, "--yes"]);
  assert.equal(run(["list"]).length, 0);
  const scan = (directory) => {
    for (const entry of readdirSync(directory, { withFileTypes: true })) {
      const path = join(directory, entry.name);
      if (entry.isDirectory()) scan(path);
      else if (entry.isFile())
        assert.ok(
          !readFileSync(path).includes(Buffer.from(key)),
          `Plaintext key in ${path}`,
        );
    }
  };
  for (const directory of [vault, sync, join(root, "logs")])
    if (existsSync(directory)) scan(directory);
  console.log(
    "CLI runtime passed: create, CRUD, search, reveal, encrypted backup, folder sync, lock/unlock, Native Messaging ping, plaintext scan",
  );
} finally {
  agent.kill("SIGTERM");
  for (let attempt = 0; attempt < 50 && !stopped; attempt++) await delay(100);
  if (!stopped) agent.kill("SIGKILL");
  rmSync(root, { recursive: true, force: true });
}

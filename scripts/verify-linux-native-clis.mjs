#!/usr/bin/env node
// Production Agent/CLI, isolated vendor homes and synthetic grants. No vendor requests.
import assert from "node:assert/strict";
import { execFileSync, spawn } from "node:child_process";
import {
  chmodSync,
  existsSync,
  mkdirSync,
  mkdtempSync,
  readFileSync,
  readdirSync,
  rmSync,
  symlinkSync,
  writeFileSync,
} from "node:fs";
import { dirname, join, resolve } from "node:path";
import { createConnection } from "node:net";
import { DatabaseSync } from "node:sqlite";
import { setTimeout as delay } from "node:timers/promises";

assert.equal(process.platform, "linux", "Linux only");
assert.ok(process.argv[2], "Pass directory containing aipass and aipass-agent");
const binaries = resolve(process.argv[2]);
// Optional official installations: only --version, --help and Claude auth status run.
const vendors = process.argv[3] ? resolve(process.argv[3]) : null;
const root = mkdtempSync("/tmp/aipass-native-");
const home = join(root, "home");
const runtime = join(root, "run");
const vault = join(root, "data/vault");
const write = (path, data) => {
  mkdirSync(dirname(path), { recursive: true, mode: 0o700 });
  writeFileSync(path, typeof data === "string" ? data : JSON.stringify(data), {
    mode: 0o600,
  });
};
for (const dir of [home, runtime]) mkdirSync(dir, { mode: 0o700 });
write(join(root, "data/agent/sync-settings.json"), {
  mode: "local",
  cloudkitMigrationComplete: true,
});
const env = {
  HOME: home,
  USERPROFILE: home,
  SHELL: "/bin/bash",
  PATH: "/usr/bin:/bin",
  LANG: "C.UTF-8",
  XDG_CONFIG_HOME: join(root, "config"),
  XDG_DATA_HOME: join(root, "data-home"),
  XDG_CACHE_HOME: join(root, "cache"),
  XDG_RUNTIME_DIR: runtime,
  DBUS_SESSION_BUS_ADDRESS: `unix:path=${root}/absent-bus`,
  AIPASS_VAULT_DIR: vault,
  AIPASS_AGENT_RUNTIME_DIR: runtime,
  AIPASS_LOG_DIR: join(root, "logs"),
  AIPASS_AGENT_SUPPRESS_TRAY: "1",
  CARGO_HOME: join(root, "cargo"),
  PNPM_HOME: join(root, "pnpm"),
  FNM_DIR: join(root, "fnm"),
  CODEX_HOME: join(root, "codex"),
  COPILOT_HOME: join(root, "copilot"),
  GEMINI_CLI_HOME: join(root, "google-home"),
  CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC: "1",
  HTTP_PROXY: "http://127.0.0.1:9",
  HTTPS_PROXY: "http://127.0.0.1:9",
  NO_PROXY: "127.0.0.1,localhost",
  DO_NOT_TRACK: "1",
};
const paths = {
  claude: join(home, ".local/bin/claude"),
  codex: join(env.CARGO_HOME, "bin/codex"),
  gemini: join(env.FNM_DIR, "node-versions/v26.0.0/installation/bin/gemini"),
  copilot: join(env.PNPM_HOME, "copilot"),
  grok: join(home, ".grok/bin/grok"),
};
// env-node shebangs must find Node even though the desktop PATH cannot.
const node = join(home, ".nvm/versions/node/v26.0.0/bin/node");
mkdirSync(dirname(node), { recursive: true });
symlinkSync(process.execPath, node);
for (const [name, path] of Object.entries(paths)) {
  mkdirSync(dirname(path), { recursive: true });
  if (vendors && name !== "grok") {
    assert.ok(existsSync(join(vendors, name)), `Missing official ${name}`);
    symlinkSync(join(vendors, name), path);
  } else {
    write(
      path,
      `#!/usr/bin/env node
const a = process.argv.slice(2).join(' ');
if (a === '--version') console.log(${JSON.stringify(name === "claude" ? "2.1.0 (Claude Code)" : `${name} 1.0.0`)});
else if (a === 'auth status --json') console.log(JSON.stringify({loggedIn:true,authMethod:'claude.ai',email:'alice@example.test',orgId:'organization-a',subscriptionType:'max'}));
else console.log('Manage login --web-flow Gemini CLI --acp Sign in to Grok --device-auth --claudeai --no-session-persistence --setting-sources --strict-mcp-config');
`,
    );
    chmodSync(path, 0o700);
  }
}
const jwt = (claims) =>
  `e30.${Buffer.from(JSON.stringify(claims)).toString("base64url")}.fixture`;
const access = jwt({ exp: 2208988800 });
const codexAuth = (token = access) => ({
  tokens: {
    id_token: jwt({ sub: "alice", email: "alice@example.test" }),
    access_token: token,
    refresh_token: "synthetic-codex-refresh",
    account_id: "workspace-a",
  },
});
const codexFile = join(env.CODEX_HOME, "auth.json");
write(codexFile, codexAuth());
write(join(home, ".claude/.credentials.json"), {
  claudeAiOauth: {
    accessToken: "synthetic-claude-access",
    refreshToken: "synthetic-claude-refresh",
    expiresAt: 2208988800000,
    scopes: ["user:inference", "user:profile"],
    subscriptionType: "max",
  },
});
write(join(home, ".claude.json"), {
  oauthAccount: {
    emailAddress: "alice@example.test",
    organizationUuid: "organization-a",
    accountUuid: "alice",
  },
});
write(join(env.GEMINI_CLI_HOME, ".gemini/oauth_creds.json"), {
  access_token: "synthetic-gemini-access",
  refresh_token: "synthetic-gemini-refresh",
  expiry_date: 2208988800000,
});
write(join(env.GEMINI_CLI_HOME, ".gemini/google_accounts.json"), {
  active: "alice@example.test",
});
write(join(env.COPILOT_HOME, "config.json"), {
  lastLoggedInUser: { login: "alice", host: "https://github.com" },
  copilotTokens: { "https://github.com:alice": "synthetic-copilot-access" },
});
write(join(home, ".grok/auth.json"), {
  "https://auth.x.ai": {
    key: "synthetic-grok-access",
    email: "alice@example.test",
    expires_at: "2040-01-01T00:00:00Z",
  },
});
const cli = join(binaries, "aipass");
const run = (args, extra = {}) => {
  const output = execFileSync(cli, ["--json", ...args], {
    env: { ...env, ...extra },
    encoding: "utf8",
    timeout: 90_000,
    stdio: ["ignore", "pipe", "pipe"],
  });
  assert.ok(!output.includes("synthetic-"), "CLI response leaked a grant");
  assert.ok(!output.includes(access), "CLI response leaked an access token");
  return JSON.parse(output);
};
const agent = spawn(join(binaries, "aipass-agent"), ["--vault", vault], {
  env,
  stdio: "ignore",
});
let stopped = false;
agent.on("exit", () => {
  stopped = true;
});
const checks = [];
// Read the same authenticated capability response used by the desktop UI.
const capability = (provider) =>
  new Promise((accept, reject) => {
    const name = readdirSync(runtime).find((n) => n.endsWith(".sock"));
    const socket = createConnection(join(runtime, name));
    let bytes = Buffer.alloc(0);
    socket.setTimeout(40_000, () =>
      socket.destroy(new Error("Capability probe timed out")),
    );
    socket.on("error", reject);
    socket.on("connect", () => {
      const token = readFileSync(
        join(runtime, name.replace(/\.sock$/, ".token")),
        "utf8",
      );
      const body = Buffer.from(
        JSON.stringify({
          protocolVersion: 18,
          authToken: token,
          request:
            provider === "anthropic"
              ? { type: "claude.cli.status" }
              : { type: "subscription.cli.status", provider },
        }),
      );
      const header = Buffer.alloc(4);
      header.writeUInt32LE(body.length);
      socket.write(Buffer.concat([header, body]));
    });
    socket.on("data", (chunk) => {
      bytes = Buffer.concat([bytes, chunk]);
      if (bytes.length >= 4 && bytes.length >= bytes.readUInt32LE(0) + 4) {
        socket.destroy();
        try {
          const response = JSON.parse(bytes.subarray(4).toString());
          assert.equal(response.ok, true);
          accept(response.data);
        } catch (error) {
          reject(error);
        }
      }
    });
  });
const imported = (provider, status = "imported", identity = /alice/) => {
  const task = run(["accounts", "import", "--provider", provider]);
  const result = task.results.find((r) => r.status === status);
  assert.ok(
    result,
    `${provider}: expected ${status}, got ${JSON.stringify(task.results)}`,
  );
  assert.match(result.accountIdentity, identity);
  return result;
};
try {
  for (let i = 0; i < 100; i++) {
    assert.ok(!stopped, "Agent exited during startup");
    if (readdirSync(runtime).some((n) => n.endsWith(".token"))) break;
    await delay(100);
  }
  run(["init"], { AIPASS_MASTER_PASSWORD: "synthetic-master-password-2026" });
  for (const [provider, directory] of [
    ["cursor", join(env.XDG_CONFIG_HOME, "cursor")],
    [
      "kimi-work",
      join(env.XDG_CONFIG_HOME, "kimi-desktop/daimon-share/daimon"),
    ],
    ["devin", join(env.XDG_DATA_HOME, "devin")],
    ["kiro", join(env.XDG_DATA_HOME, "kiro-cli")],
  ]) {
    const task = run(["accounts", "import", "--provider", provider]);
    assert.ok(
      task.results.some(
        (r) => r.source.root === directory && r.errorCode === "not_found",
      ),
      `${provider}: wrong XDG discovery ${JSON.stringify(task.results)}`,
    );
  }
  write(join(env.XDG_DATA_HOME, "opencode/auth.json"), {
    "opencode-go": { type: "api", key: "synthetic-opencode-go" },
    opencode: { type: "api", key: "synthetic-opencode-zen" },
  });
  for (const provider of ["opencode-go", "opencode-zen"]) {
    assert.equal(
      imported(provider, "imported", /^key-/).source.root,
      join(env.XDG_DATA_HOME, "opencode"),
    );
    imported(provider, "existing", /^key-/);
  }
  const devinToken = jwt({ sub: "alice-devin", exp: 2208988800 });
  write(
    join(env.XDG_DATA_HOME, "devin/credentials.toml"),
    `windsurf_api_key = ${JSON.stringify(devinToken)}\n`,
  );
  imported("devin");
  imported("devin", "existing");
  const kiroRoot = join(env.XDG_DATA_HOME, "kiro-cli");
  mkdirSync(kiroRoot, { recursive: true });
  const db = new DatabaseSync(join(kiroRoot, "data.sqlite3"));
  try {
    db.exec("CREATE TABLE auth_kv (key TEXT PRIMARY KEY, value TEXT)");
    for (const kind of ["social", "odic", "external-idp"]) {
      db.prepare("INSERT INTO auth_kv VALUES (?, ?)").run(
        `kirocli:${kind}:token`,
        JSON.stringify({
          access_token: `synthetic-kiro-${kind}`,
          refresh_token: "synthetic-kiro-refresh",
          expires_at: "2040-01-01T00:00:00Z",
          profile_arn: `arn:fixture:alice-${kind}`,
        }),
      );
    }
  } finally {
    db.close();
  }
  for (const expected of ["imported", "existing"]) {
    const task = run(["accounts", "import", "--provider", "kiro"]);
    const matches = task.results.filter(
      (r) => r.source.root === kiroRoot && r.status === expected,
    );
    assert.equal(matches.length, 3, JSON.stringify(task.results));
    assert.equal(new Set(matches.map((r) => r.accountIdentity)).size, 3);
  }
  checks.push(
    "custom XDG discovery: Cursor, Kimi; Devin, three Kiro SQLite identities, OpenCode Go/Zen import and dedup",
  );
  for (const provider of [
    "anthropic",
    "codex",
    "gemini-cli",
    "copilot",
    "grok",
  ]) {
    const status = await capability(provider);
    assert.equal(
      status.available,
      true,
      `${provider}: ${JSON.stringify(status)}`,
    );
    imported(provider);
    imported(provider, "existing");
    checks.push(
      `${provider}: ${vendors && provider !== "grok" ? "official" : "fixture"} ${status.version}; discovery, credentials, import, dedup`,
    );
  }
  const fail = (expected) => {
    const task = run(["accounts", "import", "--provider", "codex"]);
    const item = task.results.find((r) => r.source.root === env.CODEX_HOME);
    assert.equal(item?.errorCode, expected, JSON.stringify(task.results));
  };
  write(codexFile, codexAuth(jwt({ exp: 1 })));
  fail("needs_login");
  write(codexFile, "{invalid");
  fail("invalid_format");
  write(codexFile, codexAuth());
  chmodSync(codexFile, 0);
  fail("permission_denied");
  chmodSync(codexFile, 0o600);
  rmSync(codexFile);
  fail("not_found");
  // One missing provider must not prevent unrelated sources from succeeding.
  imported("copilot", "existing");
  write(codexFile, codexAuth());
  imported("codex", "existing");
  checks.push(
    "expired, malformed, unreadable, missing, recovery, provider isolation",
  );
  if (!vendors) {
    chmodSync(paths.codex, 0o600);
    fail("cli_unavailable");
    chmodSync(paths.codex, 0o700);
    imported("codex", "existing");
    checks.push("non-executable file rejected");
  }
  const scan = (dir) => {
    if (!existsSync(dir)) return;
    for (const entry of readdirSync(dir, { withFileTypes: true })) {
      const path = join(dir, entry.name);
      if (entry.isDirectory()) scan(path);
      else if (entry.isFile()) {
        const bytes = readFileSync(path);
        for (const secret of [
          access,
          "synthetic-codex-refresh",
          "synthetic-copilot-access",
          "synthetic-gemini-access",
          "synthetic-grok-access",
          "synthetic-opencode-go",
          "synthetic-opencode-zen",
          "synthetic-kiro-",
          devinToken,
        ])
          assert.ok(
            !bytes.includes(Buffer.from(secret)),
            `Plaintext credential in ${path}`,
          );
      }
    }
  };
  scan(vault);
  scan(join(root, "logs"));
  console.log(
    JSON.stringify(
      { mode: vendors ? "official-cli" : "fixture-cli", checks },
      null,
      2,
    ),
  );
} finally {
  agent.kill("SIGTERM");
  for (let i = 0; i < 50 && !stopped; i++) await delay(100);
  if (!stopped) agent.kill("SIGKILL");
  rmSync(root, { recursive: true, force: true });
}

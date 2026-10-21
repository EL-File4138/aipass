// Run pnpm without a shell, including installations exposed as pnpm.cmd on Windows.
import { existsSync, readFileSync, realpathSync } from "node:fs";
import { basename, delimiter, dirname, join } from "node:path";

export function pnpmCommand(args, { platform = process.platform, env = process.env } = {}) {
  const entry = env.npm_execpath;
  if (entry && /^pnpm(?:\.[cm]?js|\.exe)?$/i.test(basename(entry)) && existsSync(entry)) {
    // pnpm 12 also ships a native executable named `pnpm` without a suffix.
    return /\.[cm]?js$/i.test(entry) ? [process.execPath, [entry, ...args]] : [entry, args];
  }
  if (platform !== "win32") return ["pnpm", args];
  for (const directory of (env.PATH ?? env.Path ?? "").split(delimiter).filter(Boolean)) {
    const executable = join(directory, "pnpm.exe");
    if (existsSync(executable)) return [executable, args];
    // npm's Windows shim may point at a native pnpm 12 binary inside the
    // package rather than a top-level pnpm.exe or the older bin/pnpm.cjs.
    for (const packageName of ["pnpm", "corepack"]) {
      const packageRoot = join(directory, "node_modules", packageName);
      try {
        const pkg = JSON.parse(readFileSync(join(packageRoot, "package.json"), "utf8"));
        const bin = typeof pkg.bin === "string" ? pkg.bin : pkg.bin?.pnpm;
        if (typeof bin === "string") {
          const entry = join(packageRoot, bin);
          if (existsSync(entry) && !/\.(?:cmd|bat)$/i.test(entry))
            return /\.[cm]?js$/i.test(entry) ? [process.execPath, [entry, ...args]] : [entry, args];
        }
      } catch { /* Try other supported layouts. */ }
    }
    const entries = [
      join(directory, "pnpm.cjs"),
      join(directory, "node_modules/pnpm/bin/pnpm.cjs"),
      join(directory, "node_modules/corepack/dist/pnpm.js"),
    ];
    const shim = join(directory, "pnpm");
    if (existsSync(shim)) {
      const resolved = realpathSync(shim);
      if (/\.[cm]?js$/i.test(resolved)) entries.unshift(resolved);
      entries.push(join(dirname(resolved), "pnpm.cjs"));
    }
    const script = entries.find(existsSync);
    if (script) return [process.execPath, [script, ...args]];
  }
  throw new Error("Cannot locate pnpm's JavaScript entry point. Install pnpm with Corepack, npm, or pnpm/action-setup.");
}

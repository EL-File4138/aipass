#!/usr/bin/env node
import assert from "node:assert/strict";
import { verifyModelCli } from "./verify-model-cli.mjs";
assert.ok(process.argv[2], "Pass the official cursor-agent-local executable");
await verifyModelCli(process.argv[2], "cursor");

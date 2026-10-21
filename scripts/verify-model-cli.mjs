#!/usr/bin/env node
// Official vendor runtime with synthetic localhost model responses; no paid inference.
import assert from "node:assert/strict";
import { createServer } from "node:http";
import { spawnSync, spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from "node:fs";
import { join, dirname } from "node:path";
export async function verifyModelCli(binary, tool = "cursor") {
  assert.ok(["darwin", "linux"].includes(process.platform), "macOS/Linux only");
  assert.ok(["cursor", "pi"].includes(tool), "Unsupported runtime");
  const probe = spawnSync(binary, ["--version"], {
    encoding: "utf8",
    timeout: 30000,
  });
  assert.equal(probe.status, 0, probe.stderr);
  const version = `${probe.stdout}${probe.stderr}`.trim();
  assert.ok(version, "Vendor version missing");
  for (const protocol of [
    "chat_completions",
    "responses",
    "anthropic_messages",
  ]) {
    const root = mkdtempSync(`/tmp/aipass-${tool}-smoke-`);
    mkdirSync(join(root, "work"));
    const requests = [];
    const server = createServer(async (req, res) => {
      let body = "";
      for await (const chunk of req) body += chunk;
      requests.push({
        method: req.method,
        path: req.url,
        authorization: req.headers.authorization,
        apiKey: req.headers["x-api-key"],
        body: body ? JSON.parse(body) : undefined,
      });
      if (req.method === "GET") {
        res.setHeader("Content-Type", "application/json");
        return res.end(
          JSON.stringify({
            object: "list",
            data: [
              { id: "fixture-model", object: "model", api_types: [protocol] },
            ],
          }),
        );
      }
      res.writeHead(200, { "Content-Type": "text/event-stream" });
      const event = (type, data) =>
        res.write(`event: ${type}\ndata: ${JSON.stringify(data)}\n\n`);
      if (protocol === "chat_completions") {
        res.write(
          `data: ${JSON.stringify({ id: "chatcmpl-fixture", object: "chat.completion.chunk", created: 0, model: "fixture-model", choices: [{ index: 0, delta: { role: "assistant", content: "OK" }, finish_reason: null }] })}\n\n`,
        );
        res.write(
          `data: ${JSON.stringify({ id: "chatcmpl-fixture", object: "chat.completion.chunk", created: 0, model: "fixture-model", choices: [{ index: 0, delta: {}, finish_reason: "stop" }], usage: { prompt_tokens: 1, completion_tokens: 1, total_tokens: 2 } })}\n\ndata: [DONE]\n\n`,
        );
      } else if (protocol === "anthropic_messages") {
        event("message_start", {
          type: "message_start",
          message: {
            id: "msg_fixture",
            type: "message",
            role: "assistant",
            model: "fixture-model",
            content: [],
            stop_reason: null,
            stop_sequence: null,
            usage: { input_tokens: 1, output_tokens: 0 },
          },
        });
        event("content_block_start", {
          type: "content_block_start",
          index: 0,
          content_block: { type: "text", text: "" },
        });
        event("content_block_delta", {
          type: "content_block_delta",
          index: 0,
          delta: { type: "text_delta", text: "OK" },
        });
        event("content_block_stop", { type: "content_block_stop", index: 0 });
        event("message_delta", {
          type: "message_delta",
          delta: { stop_reason: "end_turn", stop_sequence: null },
          usage: { output_tokens: 1 },
        });
        event("message_stop", { type: "message_stop" });
      } else {
        const response = {
          id: "resp_fixture",
          object: "response",
          created_at: 0,
          model: "fixture-model",
          status: "in_progress",
          output: [],
        };
        const item = {
          id: "msg_fixture",
          type: "message",
          role: "assistant",
          content: [],
        };
        event("response.created", {
          type: "response.created",
          sequence_number: 0,
          response,
        });
        event("response.output_item.added", {
          type: "response.output_item.added",
          sequence_number: 1,
          output_index: 0,
          item,
        });
        event("response.content_part.added", {
          type: "response.content_part.added",
          sequence_number: 2,
          item_id: item.id,
          output_index: 0,
          content_index: 0,
          part: { type: "output_text", text: "", annotations: [] },
        });
        event("response.output_text.delta", {
          type: "response.output_text.delta",
          sequence_number: 3,
          item_id: item.id,
          output_index: 0,
          content_index: 0,
          delta: "OK",
        });
        event("response.output_text.done", {
          type: "response.output_text.done",
          sequence_number: 4,
          item_id: item.id,
          output_index: 0,
          content_index: 0,
          text: "OK",
        });
        event("response.output_item.done", {
          type: "response.output_item.done",
          sequence_number: 5,
          output_index: 0,
          item: {
            ...item,
            status: "completed",
            content: [{ type: "output_text", text: "OK", annotations: [] }],
          },
        });
        event("response.completed", {
          type: "response.completed",
          sequence_number: 6,
          response: {
            ...response,
            status: "completed",
            output: [
              {
                ...item,
                status: "completed",
                content: [{ type: "output_text", text: "OK", annotations: [] }],
              },
            ],
            usage: { input_tokens: 1, output_tokens: 1, total_tokens: 2 },
          },
        });
      }
      res.end();
    });
    await new Promise((r) => server.listen(0, "127.0.0.1", r));
    const base = `http://127.0.0.1:${server.address().port}/v1`;
    if (tool === "pi") {
      mkdirSync(join(root, "pi"));
      writeFileSync(
        join(root, "pi/models.json"),
        JSON.stringify({
          providers: {
            fixture: {
              baseUrl:
                protocol === "anthropic_messages"
                  ? base.replace(/\/v1$/, "")
                  : base,
              api:
                protocol === "responses"
                  ? "openai-responses"
                  : protocol === "anthropic_messages"
                    ? "anthropic-messages"
                    : "openai-completions",
              apiKey: "!printf fixture-cursor-key",
              models: [{ id: "fixture-model" }],
            },
          },
        }),
      );
    }
    let output = "";
    const child = spawn(
      binary,
      tool === "pi"
        ? [
            "--print",
            "--no-session",
            "--offline",
            "--no-tools",
            "--no-extensions",
            "--no-skills",
            "--no-context-files",
            "--no-prompt-templates",
            "--provider",
            "fixture",
            "--model",
            "fixture-model",
            "Only reply OK",
          ]
        : [
            "--authless",
            "--model",
            "fixture-model",
            "--print",
            "--output-format",
            "text",
            "--trust",
            "--workspace",
            join(root, "work"),
            "Only reply OK",
          ],
      {
        cwd: join(root, "work"),
        env: {
          HOME: root,
          PATH: `${dirname(process.execPath)}:/usr/bin:/bin`,
          PI_CODING_AGENT_DIR: join(root, "pi"),
          PI_OFFLINE: "1",
          PI_TELEMETRY: "0",
          CURSOR_LOCAL_AGENT_BASE_URL: base,
          CURSOR_LOCAL_AGENT_API_KEY: "fixture-cursor-key",
          CURSOR_ENABLE_AUTHLESS: "1",
          CURSOR_LOCAL_AGENT_MAX_RETRIES: "0",
          AGENT_CLI_CREDENTIAL_STORE: "file",
          HTTP_PROXY: "http://127.0.0.1:9",
          HTTPS_PROXY: "http://127.0.0.1:9",
          NO_PROXY: "localhost,127.0.0.1",
        },
        stdio: ["ignore", "pipe", "pipe"],
      },
    );
    child.stdout.on("data", (b) => (output += b));
    child.stderr.on("data", (b) => (output += b));
    const timer = setTimeout(() => child.kill("SIGKILL"), 45000);
    const code = await new Promise((accept, reject) => {
      child.on("error", reject);
      child.on("exit", accept);
    });
    clearTimeout(timer);
    await new Promise((r) => server.close(r));
    try {
      const post = requests.find((r) => r.method === "POST");
      assert.ok(post, output);
      const expected =
        protocol === "responses"
          ? "/v1/responses"
          : protocol === "anthropic_messages"
            ? "/v1/messages"
            : "/v1/chat/completions";
      assert.equal(new URL(post.path, "http://127.0.0.1").pathname, expected);
      assert.ok(
        post.authorization === "Bearer fixture-cursor-key" ||
          post.apiKey === "fixture-cursor-key",
      );
      assert.equal(code, 0, output);
      assert.match(output, /OK/);
      console.log(
        JSON.stringify({
          tool,
          platform: process.platform,
          arch: process.arch,
          version,
          protocol,
          path: post.path,
          exitCode: code,
          receivedOK: true,
        }),
      );
    } finally {
      rmSync(root, { recursive: true, force: true });
    }
  }
}

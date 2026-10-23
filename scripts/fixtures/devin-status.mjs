// Synthetic HTTPS identity authority for packaged Agent import checks.
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { createServer } from "node:https";
import { join } from "node:path";
import {
  isMainThread,
  parentPort,
  Worker,
  workerData,
} from "node:worker_threads";

const statusPath =
  "/exa.seat_management_pb.SeatManagementService/GetUserStatus";

export async function devinStatusFixture(directory, key) {
  assert.ok(isMainThread);
  mkdirSync(directory, { recursive: true, mode: 0o700 });
  const ca = join(directory, "ca.pem");
  const privateKey = join(directory, "server.key");
  const certificate = join(directory, "server.pem");
  const extensions = join(directory, "extensions.cnf");
  writeFileSync(
    extensions,
    "basicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature,keyEncipherment\nextendedKeyUsage=serverAuth\nsubjectAltName=IP:127.0.0.1\n",
  );
  const openssl = (args) =>
    execFileSync("openssl", args, { stdio: "ignore", timeout: 30000 });
  openssl([
    "req",
    "-x509",
    "-newkey",
    "rsa:2048",
    "-nodes",
    "-keyout",
    join(directory, "ca.key"),
    "-out",
    ca,
    "-days",
    "1",
    "-subj",
    "/CN=AIPass Fixture CA",
  ]);
  openssl([
    "req",
    "-newkey",
    "rsa:2048",
    "-nodes",
    "-keyout",
    privateKey,
    "-out",
    join(directory, "server.csr"),
    "-subj",
    "/CN=127.0.0.1",
  ]);
  openssl([
    "x509",
    "-req",
    "-in",
    join(directory, "server.csr"),
    "-CA",
    ca,
    "-CAkey",
    join(directory, "ca.key"),
    "-CAcreateserial",
    "-out",
    certificate,
    "-days",
    "1",
    "-extfile",
    extensions,
  ]);
  const worker = new Worker(new URL(import.meta.url), {
    workerData: { privateKey, certificate, key },
  });
  try {
    const url = await new Promise((resolve, reject) => {
      worker.once("message", resolve);
      worker.once("error", reject);
      worker.once("exit", (code) =>
        reject(new Error(`Identity fixture exited: ${code}`)),
      );
    });
    return { url, ca, close: () => worker.terminate() };
  } catch (error) {
    await worker.terminate();
    throw error;
  }
}

if (!isMainThread) {
  const server = createServer(
    {
      key: readFileSync(workerData.privateKey),
      cert: readFileSync(workerData.certificate),
    },
    async (request, response) => {
      let bytes = 0;
      const chunks = [];
      for await (const chunk of request) {
        bytes += chunk.length;
        if (bytes > 65536) {
          request.destroy();
          return;
        }
        chunks.push(chunk);
      }
      let body;
      try {
        body = JSON.parse(Buffer.concat(chunks).toString());
      } catch {
        response.writeHead(400).end();
        return;
      }
      if (
        request.method !== "POST" ||
        request.url !== statusPath ||
        body.metadata?.apiKey !== workerData.key
      ) {
        response.writeHead(401).end();
        return;
      }
      response.writeHead(200, { "content-type": "application/json" });
      response.end(
        JSON.stringify({
          userStatus: {
            userId: "alice-devin",
            email: "alice@example.test",
            teamId: "fixture-team",
            planStatus: { planInfo: { devinInfo: { orgId: "fixture-org" } } },
          },
        }),
      );
    },
  );
  server.listen(0, "127.0.0.1", () =>
    parentPort.postMessage(`https://127.0.0.1:${server.address().port}`),
  );
}

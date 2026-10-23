import assert from "node:assert/strict";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { request } from "node:https";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";
import { devinStatusFixture } from "./devin-status.mjs";

test(
  "Devin identity fixture requires trusted TLS and the saved key",
  { skip: process.platform === "win32" },
  async () => {
    const root = mkdtempSync(join(tmpdir(), "aipass-identity-fixture-"));
    let fixture;
    try {
      fixture = await devinStatusFixture(root, "synthetic-key");
      const read = (key, ca) =>
        new Promise((resolve, reject) => {
          const req = request(
            `${fixture.url}/exa.seat_management_pb.SeatManagementService/GetUserStatus`,
            { method: "POST", ca },
            (response) => {
              let body = "";
              response.setEncoding("utf8").on("data", (chunk) => {
                body += chunk;
              });
              response.on("end", () =>
                resolve({ status: response.statusCode, body }),
              );
            },
          );
          req.on("error", reject);
          req.end(JSON.stringify({ metadata: { apiKey: key } }));
        });
      await assert.rejects(read("synthetic-key"));
      const ca = readFileSync(fixture.ca);
      assert.equal((await read("wrong-key", ca)).status, 401);
      const result = await read("synthetic-key", ca);
      assert.equal(result.status, 200);
      const user = JSON.parse(result.body).userStatus;
      assert.equal(user.userId, "alice-devin");
      assert.equal(user.teamId, "fixture-team");
      assert.equal(user.planStatus.planInfo.devinInfo.orgId, "fixture-org");
      assert.ok(!result.body.includes("synthetic-key"));
    } finally {
      await fixture?.close();
      rmSync(root, { recursive: true, force: true });
    }
  },
);

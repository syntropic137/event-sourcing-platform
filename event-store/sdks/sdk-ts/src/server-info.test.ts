import { test } from "node:test";
import assert from "node:assert/strict";
import {
  Server,
  ServerCredentials,
  status,
  type ServiceDefinition,
  type UntypedServiceImplementation,
} from "@grpc/grpc-js";
import { EventStoreService } from "./gen/eventstore/v1/eventstore.js";
import { EventStoreClientTS } from "./client.js";
import {
  Capabilities,
  CompatibilityError,
  LEGACY_SERVER_INFO,
  missingCapabilities,
  versionAtLeast,
  type ServerInfo,
} from "./server-info.js";

async function serve(
  def: ServiceDefinition,
  impl: UntypedServiceImplementation,
): Promise<{ addr: string; stop: () => Promise<void> }> {
  const server = new Server();
  server.addService(def, impl);
  const port = await new Promise<number>((resolve, reject) =>
    server.bindAsync("127.0.0.1:0", ServerCredentials.createInsecure(), (err, p) =>
      err ? reject(err) : resolve(p),
    ),
  );
  return {
    addr: `127.0.0.1:${port}`,
    stop: () => new Promise((resolve) => server.tryShutdown(() => resolve())),
  };
}

const reported = (serverVersion: string, capabilities: string[] = []): ServerInfo => ({
  serverVersion,
  apiVersion: "eventstore.v1",
  backend: "postgres",
  capabilities,
  legacy: false,
});

test("versionAtLeast compares numerically and fails closed", () => {
  const info = reported("0.17.0");
  assert.ok(versionAtLeast(info, "0.16.0"));
  assert.ok(versionAtLeast(info, "0.17.0"));
  assert.ok(versionAtLeast(info, "v0.17"));
  assert.ok(versionAtLeast(info, "0.17.0-rc.1"));
  assert.ok(versionAtLeast(reported("0.10.0"), "0.9.0"));
  assert.ok(!versionAtLeast(info, "0.17.1"));
  assert.ok(!versionAtLeast(info, "garbage"));
  assert.ok(!versionAtLeast(reported("0.17.0-rc.1"), "0.17.0"));
  assert.ok(!versionAtLeast(LEGACY_SERVER_INFO, "0.0.1"));
});

test("versionAtLeast follows SemVer 2.0 pre-release precedence", () => {
  // SemVer 2.0 section 11 example, ascending.
  const ordered = [
    "1.0.0-alpha",
    "1.0.0-alpha.1",
    "1.0.0-alpha.beta",
    "1.0.0-beta",
    "1.0.0-beta.2",
    "1.0.0-beta.11",
    "1.0.0-rc.1",
    "1.0.0",
  ];
  ordered.forEach((have, i) => {
    ordered.forEach((want, j) => {
      assert.equal(versionAtLeast(reported(have), want), i >= j, `${have} >= ${want}`);
    });
  });
  assert.ok(!versionAtLeast(reported("0.17.0-alpha.1"), "0.17.0-rc.2"));
  assert.ok(versionAtLeast(reported("0.17.0-rc.2"), "0.17.0-rc.1"));
  assert.ok(!versionAtLeast(reported("0.17.0-rc.1"), "0.17.0-rc.2"));
  assert.ok(versionAtLeast(reported("0.17.0-rc.10"), "0.17.0-rc.9"));
  assert.ok(versionAtLeast(reported("0.17.0-rc.2+build.5"), "0.17.0-rc.2"));
  assert.ok(versionAtLeast(reported("1.0.0-rc.9007199254740993"), "1.0.0-rc.9007199254740992"));
  assert.ok(!versionAtLeast(reported("0.17.0-rc..1"), "0.0.0"));
  assert.ok(!versionAtLeast(reported("0.17.0-"), "0.0.0"));
  // Numeric identifiers beyond 64 bits still compare numerically.
  assert.ok(versionAtLeast(reported("1.0.0"), "1.0.0-rc.18446744073709551616"));
  assert.ok(versionAtLeast(reported("1.0.0-rc.18446744073709551617"), "1.0.0-rc.18446744073709551616"));
  assert.ok(!versionAtLeast(reported("1.0.0-rc.18446744073709551616"), "1.0.0-rc.18446744073709551617"));
  assert.ok(versionAtLeast(reported("1.0.0-rc.99999999999999999999"), "1.0.0-rc.9"));
  assert.ok(versionAtLeast(reported("18446744073709551616.0.0"), "18446744073709551615.9.9"));
  assert.ok(!versionAtLeast(reported("1.0.0-rc.99999999999999999999"), "1.0.0-rc.a"));
});

test("missingCapabilities reports only absent flags", () => {
  const info = reported("0.17.0", [Capabilities.COMMIT_ORDERED_GLOBAL_NONCE]);
  assert.deepEqual(missingCapabilities(info, [Capabilities.COMMIT_ORDERED_GLOBAL_NONCE]), []);
  assert.deepEqual(
    missingCapabilities(info, ["future_flag", Capabilities.COMMIT_ORDERED_GLOBAL_NONCE]),
    ["future_flag"],
  );
});

test("current server: serverInfo and requirement helpers", async () => {
  const srv = await serve(EventStoreService, {
    getServerInfo: (_call: unknown, cb: (err: null, resp: unknown) => void) =>
      cb(null, {
        serverVersion: "0.17.0",
        apiVersion: "eventstore.v1",
        backend: "memory",
        capabilities: [Capabilities.COMMIT_ORDERED_GLOBAL_NONCE],
      }),
  });
  const client = new EventStoreClientTS(srv.addr);
  try {
    const info = await client.serverInfo();
    assert.equal(info.legacy, false);
    assert.equal(info.serverVersion, "0.17.0");
    assert.equal(info.backend, "memory");
    await client.requireCapabilities([Capabilities.COMMIT_ORDERED_GLOBAL_NONCE]);
    await client.requireMinVersion("0.16.0");
    await assert.rejects(client.requireCapabilities(["not_real"]), (e: unknown) => {
      assert.ok(e instanceof CompatibilityError);
      assert.deepEqual(e.missing, ["not_real"]);
      return true;
    });
  } finally {
    client.close();
    await srv.stop();
  }
});

test("legacy server (no GetServerInfo method) lacks every capability", async () => {
  // A pre-0.17.0 server's service definition has no GetServerInfo at all.
  const { getServerInfo: _omit, ...legacyDef } = EventStoreService;
  const srv = await serve(legacyDef as ServiceDefinition, {});
  const client = new EventStoreClientTS(srv.addr);
  try {
    const info = await client.serverInfo();
    assert.equal(info.legacy, true);
    assert.equal(info.serverVersion, null);
    assert.deepEqual(info.capabilities, []);
    await assert.rejects(
      client.requireCapabilities([Capabilities.COMMIT_ORDERED_GLOBAL_NONCE]),
      (e: unknown) => {
        assert.ok(e instanceof CompatibilityError);
        assert.match(e.message, /no GetServerInfo/);
        assert.deepEqual(e.missing, [Capabilities.COMMIT_ORDERED_GLOBAL_NONCE]);
        return true;
      },
    );
    await assert.rejects(client.requireMinVersion("0.16.0"), CompatibilityError);
    await client.requireCapabilities([]);
  } finally {
    client.close();
    await srv.stop();
  }
});

test("non-UNIMPLEMENTED errors are not mistaken for legacy", async () => {
  const srv = await serve(EventStoreService, {
    getServerInfo: (_call: unknown, cb: (err: { code: number; details: string }) => void) =>
      cb({ code: status.UNAVAILABLE, details: "down" }),
  });
  const client = new EventStoreClientTS(srv.addr);
  try {
    await assert.rejects(client.serverInfo(), (e: { code?: number }) => {
      assert.equal(e.code, status.UNAVAILABLE);
      return true;
    });
  } finally {
    client.close();
    await srv.stop();
  }
});

// Live check against a real Rust server when EVENTSTORE_ADDR is set
// (e.g. `cd event-store && make run`).
test("live server advertises commit_ordered_global_nonce", { skip: !process.env.EVENTSTORE_ADDR }, async () => {
  const client = new EventStoreClientTS(process.env.EVENTSTORE_ADDR!);
  try {
    const info = await client.requireCapabilities([
      Capabilities.COMMIT_ORDERED_GLOBAL_NONCE,
      Capabilities.SUBSCRIPTION_ERRORS_SURFACED,
      Capabilities.UNDECODABLE_EVENTS_SURFACED,
      Capabilities.LITERAL_SUBSCRIPTION_PREFIX,
    ]);
    assert.equal(info.legacy, false);
    assert.equal(info.apiVersion, "eventstore.v1");
  } finally {
    client.close();
  }
});

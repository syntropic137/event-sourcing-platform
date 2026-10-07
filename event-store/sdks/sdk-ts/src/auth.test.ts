import { test } from "node:test";
import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { mkdtempSync, readFileSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { inspect } from "node:util";
import {
  Metadata,
  Server,
  ServerCredentials,
  credentials as grpcCredentials,
  status,
  type ServerWritableStream,
} from "@grpc/grpc-js";
import { EventStoreClient as GeneratedClient, EventStoreService } from "./gen/eventstore/v1/eventstore.js";
import { EventStoreClientTS } from "./client.js";
import { EventStoreClientRT } from "./runtime-client.js";
import {
  ConfigError,
  Credentials,
  SharedToken,
  UnauthenticatedError,
  authInterceptor,
  resolveConnection,
} from "./auth.js";

const BASIC = `Basic ${Buffer.from("admin:s3cret").toString("base64")}`;

/**
 * In-process stand-in for the ADR-024 gateway: every call must carry an
 * accepted `authorization` header, else UNAUTHENTICATED. Records what it saw.
 */
async function authServer(
  accept: (header: string | undefined) => boolean,
  creds: ServerCredentials = ServerCredentials.createInsecure(),
): Promise<{ addr: string; port: number; seen: Array<[string, string | undefined]>; stop: () => Promise<void> }> {
  const seen: Array<[string, string | undefined]> = [];
  const header = (md: Metadata) => {
    const v = md.get("authorization");
    return v.length === 0 ? undefined : String(v[0]);
  };
  const deny = { code: status.UNAUTHENTICATED, details: "basic auth failed" };
  const unary =
    (name: string, resp: unknown) =>
    (call: { metadata: Metadata }, cb: (err: unknown, resp?: unknown) => void) => {
      const h = header(call.metadata);
      seen.push([name, h]);
      if (!accept(h)) return cb(deny);
      cb(null, resp);
    };
  const server = new Server();
  server.addService(EventStoreService, {
    append: unary("append", { lastGlobalNonce: 1, lastAggregateNonce: 1 }),
    readStream: unary("readStream", { events: [], isEnd: true, nextFromAggregateNonce: 0 }),
    readAll: unary("readAll", { events: [], isEnd: true, nextFromGlobalNonce: 0 }),
    getServerInfo: unary("getServerInfo", {
      serverVersion: "0.17.0",
      apiVersion: "eventstore.v1",
      backend: "memory",
      capabilities: [],
    }),
    subscribe: (call: ServerWritableStream<unknown, unknown>) => {
      const h = header(call.metadata);
      seen.push(["subscribe", h]);
      if (!accept(h)) return call.emit("error", deny);
      call.write({});
      call.end();
    },
  });
  const port = await new Promise<number>((resolve, reject) =>
    server.bindAsync("127.0.0.1:0", creds, (err, p) => (err ? reject(err) : resolve(p))),
  );
  return {
    addr: `127.0.0.1:${port}`,
    port,
    seen,
    stop: () => new Promise((resolve) => server.tryShutdown(() => resolve())),
  };
}

async function drain(it: AsyncIterable<unknown>): Promise<number> {
  let n = 0;
  for await (const _ of it) n++;
  return n;
}

test("basic auth header is sent on unary calls and on subscribe", async () => {
  const srv = await authServer((h) => h === BASIC);
  const client = new EventStoreClientTS(srv.addr, { auth: Credentials.basic("admin", "s3cret") });
  try {
    await client.serverInfo();
    await client.readStream({ tenantId: "t", aggregateId: "a", fromAggregateNonce: 1, maxCount: 1, forward: true });
    await client.readAll({ tenantId: "t", fromGlobalNonce: 0, maxCount: 1, forward: true });
    assert.equal(await drain(client.subscribe({ tenantId: "t", aggregateIdPrefix: "", fromGlobalNonce: 0 })), 1);
    assert.deepEqual(
      srv.seen.map(([m]) => m),
      ["getServerInfo", "readStream", "readAll", "subscribe"],
    );
    for (const [m, h] of srv.seen) assert.equal(h, BASIC, m);
  } finally {
    client.close();
    await srv.stop();
  }
});

test("plain-object auth forms and the runtime client send the header too", async () => {
  const srv = await authServer((h) => h === BASIC || h === "Bearer tok");
  const a = new EventStoreClientTS(srv.addr, { auth: { basic: { username: "admin", password: "s3cret" } } });
  const b = new EventStoreClientTS(srv.addr, { auth: { bearerToken: "tok" } });
  const rt = new EventStoreClientRT(srv.addr, { auth: { bearerToken: "tok" } });
  try {
    await a.serverInfo();
    await b.serverInfo();
    await rt.readStream({ tenant_id: "t", aggregate_id: "a", from_aggregate_nonce: 1, max_count: 1, forward: true });
    assert.equal(await drain(rt.subscribe({ tenant_id: "t", aggregate_id_prefix: "", from_global_nonce: 0 })), 1);
    assert.deepEqual(
      srv.seen.map(([, h]) => h),
      [BASIC, "Bearer tok", "Bearer tok", "Bearer tok"],
    );
  } finally {
    a.close();
    b.close();
    await srv.stop();
  }
});

test("wrong credentials reject with UnauthenticatedError on unary and subscribe", async () => {
  const srv = await authServer((h) => h === BASIC);
  const client = new EventStoreClientTS(srv.addr, { auth: Credentials.basic("admin", "wrong-pw") });
  const none = new EventStoreClientTS(srv.addr);
  try {
    for (const p of [client.serverInfo(), none.serverInfo()]) {
      await assert.rejects(p, (e: unknown) => {
        assert.ok(e instanceof UnauthenticatedError);
        assert.equal(e.code, status.UNAUTHENTICATED);
        assert.equal(e.details, "basic auth failed");
        assert.ok(!String(e.message).includes("wrong-pw"));
        return true;
      });
    }
    await assert.rejects(
      drain(client.subscribe({ tenantId: "t", aggregateIdPrefix: "", fromGlobalNonce: 0 })),
      UnauthenticatedError,
    );
    assert.equal(srv.seen[1]![1], undefined, "no credentials configured, no header sent");
  } finally {
    client.close();
    none.close();
    await srv.stop();
  }
});

test("token provider is read per call, so rotation needs no reconnect", async () => {
  const srv = await authServer((h) => h === "Bearer one" || h === "Bearer two" || h === "Bearer async");
  const shared = new SharedToken("one");
  const client = new EventStoreClientTS(srv.addr, { auth: Credentials.tokenProvider(shared.provider) });
  const asyncClient = new EventStoreClientTS(srv.addr, {
    auth: { tokenProvider: async () => new Promise<string>((r) => setTimeout(() => r("async"), 5)) },
  });
  try {
    await client.serverInfo();
    shared.set("two");
    await client.serverInfo();
    assert.equal(await drain(client.subscribe({ tenantId: "t", aggregateIdPrefix: "", fromGlobalNonce: 0 })), 1);
    await asyncClient.serverInfo();
    assert.equal(await drain(asyncClient.subscribe({ tenantId: "t", aggregateIdPrefix: "", fromGlobalNonce: 0 })), 1);
    assert.deepEqual(
      srv.seen.map(([, h]) => h),
      ["Bearer one", "Bearer two", "Bearer two", "Bearer async", "Bearer async"],
    );
  } finally {
    client.close();
    asyncClient.close();
    await srv.stop();
  }
});

test("a failing token provider fails the call before anything is sent", async () => {
  const srv = await authServer(() => true);
  const sync = new EventStoreClientTS(srv.addr, {
    auth: Credentials.tokenProvider(() => {
      throw new Error("refresh failed: leaked-secret");
    }),
  });
  const rejected = new EventStoreClientTS(srv.addr, {
    auth: Credentials.tokenProvider(() => Promise.reject(new Error("leaked-secret"))),
  });
  const badValue = new EventStoreClientTS(srv.addr, { auth: Credentials.tokenProvider(() => "bad\r\nvalue") });
  try {
    for (const p of [sync.serverInfo(), rejected.serverInfo(), badValue.serverInfo()]) {
      await assert.rejects(p, (e: unknown) => {
        assert.ok(e instanceof UnauthenticatedError, String(e));
        assert.ok(!String(e.message).includes("leaked-secret"));
        return true;
      });
    }
    await assert.rejects(
      drain(rejected.subscribe({ tenantId: "t", aggregateIdPrefix: "", fromGlobalNonce: 0 })),
      UnauthenticatedError,
    );
    assert.equal(srv.seen.length, 0);
  } finally {
    sync.close();
    rejected.close();
    badValue.close();
    await srv.stop();
  }
});

test("cancelling a subscription while the token is pending never starts the call", async () => {
  const srv = await authServer(() => true);
  let release: (t: string) => void = () => {};
  const client = new EventStoreClientTS(srv.addr, {
    auth: Credentials.tokenProvider(() => new Promise<string>((r) => (release = r))),
  });
  try {
    const it = client.subscribe({ tenantId: "t", aggregateIdPrefix: "", fromGlobalNonce: 0 })[Symbol.asyncIterator]();
    const next = it.next();
    await it.return!();
    assert.deepEqual(await next, { value: undefined, done: true });
    release("late");
    await new Promise((r) => setTimeout(r, 50));
    assert.equal(srv.seen.length, 0);
  } finally {
    client.close();
    await srv.stop();
  }
});

test("credentials are refused over plaintext to a non-loopback host", () => {
  const auth = Credentials.basic("u", "p");
  for (const addr of ["es.example.com:8081", "http://10.0.0.5:8081", "dns:///es.example.com:8081", "0.0.0.0:1"]) {
    assert.throws(() => new EventStoreClientTS(addr, { auth }), (e: unknown) => {
      assert.ok(e instanceof ConfigError);
      assert.match(e.message, /plaintext/);
      return true;
    });
  }
  // Explicit opt-in, TLS, loopback and no-credentials are all allowed.
  new EventStoreClientTS("es.example.com:8081", { auth, allowInsecureCredentials: true }).close();
  new EventStoreClientTS("https://es.example.com", { auth: { bearerToken: "t" } }).close();
  new EventStoreClientTS("es.example.com:443", { auth, tls: true }).close();
  for (const local of ["localhost:1", "127.0.0.1:1", "127.8.9.10:1", "[::1]:1", "http://localhost:1"]) {
    new EventStoreClientTS(local, { auth }).close();
  }
  new EventStoreClientTS("es.example.com:8081").close();
});

test("endpoint forms resolve to the right target and transport", () => {
  const r = (addr: string, opts = {}) => {
    const c = resolveConnection(addr, opts);
    return [c.target, c.tls];
  };
  assert.deepEqual(r("127.0.0.1:50051"), ["127.0.0.1:50051", false]);
  assert.deepEqual(r("http://es:50051"), ["es:50051", false]);
  assert.deepEqual(r("https://es.example.com:443"), ["es.example.com:443", true]);
  assert.deepEqual(r("HTTPS://es:443"), ["es:443", true]);
  assert.deepEqual(r("es:443", { tls: true }), ["es:443", true]);
  assert.throws(() => r("http://es:443", { tls: true }), ConfigError);
  for (const bad of ["", "   ", "grpc://es:1", "http://", "http://es:1/path"]) {
    assert.throws(() => r(bad), ConfigError, bad);
  }
  assert.throws(
    () => r("https://admin:hunter2@es:443"),
    (e: unknown) => e instanceof ConfigError && !e.message.includes("hunter2"),
  );
  assert.throws(() => r("es:1", { auth: Credentials.basic("a:b", "p") }), ConfigError);
  assert.throws(() => r("es:1", { auth: { bearerToken: "" } }), ConfigError);
  assert.throws(
    () => r("localhost:1", { auth: { bearerToken: "sec\nret" } }),
    (e: unknown) => e instanceof ConfigError && !e.message.includes("sec"),
  );
});

test("secrets never appear in string forms", () => {
  const values = [
    Credentials.basic("user", "hunter2"),
    Credentials.bearer("tok-secret"),
    new SharedToken("shared-secret"),
    new EventStoreClientTS("localhost:1", { auth: Credentials.basic("user", "hunter2") }),
  ];
  const s = values.map((v) => `${String(v)} ${inspect(v, { depth: 10, showHidden: true })} ${JSON.stringify(v)}`).join(" ");
  assert.ok(s.includes("user"));
  for (const secret of ["hunter2", "tok-secret", "shared-secret", Buffer.from("user:hunter2").toString("base64")]) {
    assert.ok(!s.includes(secret), s);
  }
  (values[3] as EventStoreClientTS).close();
});

test("an inspected client with calls in flight does not expose the header", async () => {
  let release: (t: string) => void = () => {};
  const srv = await authServer(() => true);
  const a = new EventStoreClientTS(srv.addr, { auth: Credentials.basic("user", "hunter2") });
  const b = new EventStoreClientTS(srv.addr, {
    auth: Credentials.tokenProvider(() => new Promise<string>((r) => (release = r))),
  });
  const rt = new EventStoreClientRT(srv.addr, { auth: Credentials.bearer("tok-secret") });
  try {
    const calls = [a.serverInfo(), b.serverInfo(), rt.readStream({ tenant_id: "t", aggregate_id: "a" })];
    const sub = a.subscribe({ tenantId: "t", aggregateIdPrefix: "", fromGlobalNonce: 0 })[Symbol.asyncIterator]();
    const first = sub.next();
    const s = [a, b, rt].map((c) => `${inspect(c, { depth: 20, showHidden: true })} ${JSON.stringify(c)}`).join(" ");
    for (const secret of ["hunter2", "tok-secret", Buffer.from("user:hunter2").toString("base64")]) {
      assert.ok(!s.includes(secret), s);
    }
    release("late");
    await Promise.all([...calls, first]);
    await sub.return!();
  } finally {
    a.close();
    b.close();
    await srv.stop();
  }
});

test("a call deadline is enforced while the token provider is pending", async () => {
  const srv = await authServer(() => true);
  const client = new GeneratedClient(srv.addr, grpcCredentials.createInsecure(), {
    interceptors: [authInterceptor(Credentials.tokenProvider(() => new Promise<string>(() => {})))],
  });
  try {
    const err = await new Promise<{ code?: number } | null>((resolve) =>
      client.getServerInfo({}, new Metadata(), { deadline: Date.now() + 50 }, (e) => resolve(e)),
    );
    assert.equal(err?.code, status.DEADLINE_EXCEEDED);
    assert.equal(srv.seen.length, 0);
  } finally {
    client.close();
    await srv.stop();
  }
});

function openssl(): string | undefined {
  try {
    execFileSync("openssl", ["version"], { stdio: "ignore" });
    return "openssl";
  } catch {
    return undefined;
  }
}

test("TLS: https endpoint with a custom CA and server name override", { skip: !openssl() && "openssl not found" }, async () => {
  const dir = mkdtempSync(path.join(tmpdir(), "sdk-ts-tls-"));
  try {
    const f = (n: string) => path.join(dir, n);
    execFileSync("openssl", [
      "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-days", "1",
      "-keyout", f("key.pem"), "-out", f("cert.pem"),
      "-subj", "/CN=es.test", "-addext", "subjectAltName=DNS:es.test",
    ], { stdio: "ignore" });
    const cert = readFileSync(f("cert.pem"));
    const key = readFileSync(f("key.pem"));
    const srv = await authServer((h) => h === BASIC, ServerCredentials.createSsl(null, [{ cert_chain: cert, private_key: key }]));
    const ok = new EventStoreClientTS(`https://127.0.0.1:${srv.port}`, {
      tls: { rootCerts: cert, serverName: "es.test" },
      auth: Credentials.basic("admin", "s3cret"),
    });
    // Same server, default trust store: the self-signed cert must be rejected.
    const untrusted = new EventStoreClientTS(`https://127.0.0.1:${srv.port}`, {
      tls: { serverName: "es.test" },
      auth: Credentials.basic("admin", "s3cret"),
    });
    try {
      await ok.serverInfo();
      assert.equal(await drain(ok.subscribe({ tenantId: "t", aggregateIdPrefix: "", fromGlobalNonce: 0 })), 1);
      assert.deepEqual(srv.seen.map(([, h]) => h), [BASIC, BASIC]);
      await assert.rejects(untrusted.serverInfo(), (e: unknown) => (e as { code?: number }).code === status.UNAVAILABLE);
    } finally {
      ok.close();
      untrusted.close();
      await srv.stop();
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
